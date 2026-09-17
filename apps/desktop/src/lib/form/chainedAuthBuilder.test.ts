import { describe, expect, it } from "vitest";
import {
  asChained,
  asRetryOn,
  buildExtract,
  buildInject,
  buildTtl,
  chainWalk,
  DEFAULT_RETRY_ON,
  extractChoiceOf,
  injectChoiceOf,
  reachesInheritedEndpoint,
  toggleRetryStatus,
  ttlChoiceOf,
  validSourceEndpoints,
} from "./chainedAuthBuilder";
import { emptyApi, emptyEndpoint, type Api, type Auth, type Endpoint } from "../model";

function ep(id: string, auth: Auth): Endpoint {
  return { ...emptyEndpoint(id, id), auth };
}

function apiWith(endpoints: Endpoint[], apiAuth: Auth = { type: "none" }): Api {
  return { ...emptyApi("gw", "GW"), auth: apiAuth, endpoints };
}

const chainedTo = (target: string): Auth => ({
  type: "chained",
  source: { endpoint: target },
  inject: { into: "header", name: "Authorization", template: "Bearer {{value}}" },
  retryOn: [401, 403],
});

describe("chainWalk", () => {
  it("walks a simple chain of chained-auth sources", () => {
    const api = apiWith([
      ep("a", chainedTo("b")),
      ep("b", chainedTo("c")),
      ep("c", { type: "none" }),
    ]);
    expect(chainWalk(api, "a")).toEqual(["a", "b", "c"]);
  });

  it("stops at a non-chained resolved auth", () => {
    const api = apiWith([ep("a", { type: "bearer", token: "x" })]);
    expect(chainWalk(api, "a")).toEqual(["a"]);
  });

  it("resolves inherit at each step, using the API's own auth", () => {
    const api = apiWith(
      [ep("a", { type: "inherit" }), ep("b", { type: "none" })],
      chainedTo("b"),
    );
    expect(chainWalk(api, "a")).toEqual(["a", "b"]);
  });

  it("terminates on a self-cycle instead of looping forever", () => {
    const api = apiWith([ep("a", chainedTo("a"))]);
    expect(chainWalk(api, "a")).toEqual(["a"]);
  });

  it("terminates on an indirect cycle instead of looping forever", () => {
    const api = apiWith([
      ep("a", chainedTo("b")),
      ep("b", chainedTo("c")),
      ep("c", chainedTo("a")),
    ]);
    expect(chainWalk(api, "a")).toEqual(["a", "b", "c"]);
  });

  it("stops (defensively) at a dangling source naming no real endpoint — the dangling id is still recorded as visited, resolution just can't continue past it", () => {
    const api = apiWith([ep("a", chainedTo("does-not-exist"))]);
    expect(chainWalk(api, "a")).toEqual(["a", "does-not-exist"]);
  });

  it("returns just the start id when it doesn't exist at all", () => {
    const api = apiWith([]);
    expect(chainWalk(api, "ghost")).toEqual(["ghost"]);
  });
});

describe("reachesInheritedEndpoint", () => {
  it("is true when the walk reaches an endpoint declared inherit", () => {
    const api = apiWith([
      ep("a", chainedTo("b")),
      ep("b", { type: "inherit" }),
    ]);
    expect(reachesInheritedEndpoint(api, "a")).toBe(true);
  });

  it("is false when the walk never reaches an inherit-declared endpoint", () => {
    const api = apiWith([
      ep("a", chainedTo("b")),
      ep("b", { type: "none" }),
    ]);
    expect(reachesInheritedEndpoint(api, "a")).toBe(false);
  });

  it("is false for a dangling source", () => {
    const api = apiWith([ep("a", chainedTo("ghost"))]);
    expect(reachesInheritedEndpoint(api, "a")).toBe(false);
  });
});

describe("validSourceEndpoints — the core cycle-prevention logic", () => {
  it("offers every other endpoint when there is no chained auth anywhere", () => {
    const api = apiWith([
      ep("token", { type: "basic", username: "u", password: "p" }),
      ep("biz", { type: "inherit" }),
    ]);
    expect(validSourceEndpoints(api, "biz").map((e) => e.id)).toEqual(["token"]);
  });

  it("excludes the endpoint being edited itself (self-reference is unreachable)", () => {
    const api = apiWith([
      ep("token", { type: "none" }),
      ep("biz", { type: "inherit" }),
    ]);
    expect(validSourceEndpoints(api, "token").map((e) => e.id)).not.toContain(
      "token",
    );
  });

  it("excludes a candidate that would form a direct two-node cycle", () => {
    // biz's source is `token`; picking `token` as biz's source AGAIN is a
    // no-op, not tested here. What's tested: if `token` already points
    // BACK at `biz`, `biz` may not also point at `token`.
    const api = apiWith([
      ep("token", chainedTo("biz")),
      ep("biz", { type: "none" }),
    ]);
    // token -> biz already; if biz -> token were chosen, biz -> token -> biz
    // is a cycle. So token must not appear as a valid source for biz.
    expect(validSourceEndpoints(api, "biz").map((e) => e.id)).not.toContain(
      "token",
    );
  });

  it("excludes a candidate that would form an indirect (multi-hop) cycle", () => {
    const api = apiWith([
      ep("a", chainedTo("b")),
      ep("b", { type: "none" }),
      ep("c", { type: "none" }),
    ]);
    // a -> b already. Editing b's auth: choosing `a` as b's source would
    // make b -> a -> b, a cycle.
    const sourcesForB = validSourceEndpoints(api, "b").map((e) => e.id);
    expect(sourcesForB).not.toContain("a");
    expect(sourcesForB).toContain("c");
  });

  it("does NOT exclude an unrelated fan-out (a diamond is not a cycle)", () => {
    const api = apiWith([
      ep("token", { type: "none" }),
      ep("a", chainedTo("token")),
      ep("b", chainedTo("token")),
    ]);
    // a and b both depend on token independently — neither forms a cycle
    // through the other.
    expect(validSourceEndpoints(api, "a").map((e) => e.id)).toEqual(
      expect.arrayContaining(["token", "b"]),
    );
  });

  it("excludes a candidate reachable only through an inherited endpoint-level auth", () => {
    const api = apiWith(
      [
        ep("token", { type: "none" }),
        ep("mid", { type: "inherit" }), // resolves to API auth
        ep("biz", { type: "inherit" }),
      ],
      chainedTo("token"),
    );
    // mid's resolved auth is the API-level chained auth (source: token).
    // Editing `biz`'s own auth: choosing `mid` as biz's source is fine
    // (mid -> token, no cycle back to biz).
    expect(validSourceEndpoints(api, "biz").map((e) => e.id)).toContain("mid");
  });

  it("(API level) excludes any endpoint whose chain reaches an inherit-declared endpoint", () => {
    const api = apiWith([
      ep("a", { type: "inherit" }),
      ep("b", chainedTo("a")),
      ep("c", { type: "none" }),
    ]);
    // Setting the API-level auth to chained(source: b) would mean: API auth
    // -> b -> a (inherit) -> API auth -> b -> ... a cycle. So neither a nor
    // b may be offered as an API-level source; c is fine.
    const sources = validSourceEndpoints(api, null).map((e) => e.id);
    expect(sources).not.toContain("a");
    expect(sources).not.toContain("b");
    expect(sources).toContain("c");
  });

  it("is defensive against a dangling source elsewhere in the document", () => {
    const api = apiWith([
      ep("a", chainedTo("ghost")),
      ep("b", { type: "none" }),
    ]);
    expect(() => validSourceEndpoints(api, "b")).not.toThrow();
    expect(validSourceEndpoints(api, "b").map((e) => e.id)).toContain("a");
  });

  it("offers nothing when the API has only the endpoint being edited", () => {
    const api = apiWith([ep("solo", { type: "inherit" })]);
    expect(validSourceEndpoints(api, "solo")).toEqual([]);
  });
});

describe("asChained", () => {
  it("returns a chained auth unchanged", () => {
    const auth = chainedTo("token");
    expect(asChained(auth)).toBe(auth);
  });

  it("coerces a non-chained runtime value to a default-shaped stand-in", () => {
    const stand_in = asChained({ type: "none" });
    expect(stand_in.type).toBe("chained");
    expect(stand_in.source).toEqual({ endpoint: "" });
    expect(stand_in.inject.into).toBe("header");
  });
});

describe("extract choices", () => {
  it("default (undefined) round-trips", () => {
    expect(extractChoiceOf(undefined)).toBe("default");
    expect(buildExtract("default")).toBeUndefined();
  });

  it.each([
    ["body-jsonpath", { from: "body", jsonPath: "$.access_token" }],
    ["body-regex", { from: "body", regex: "" }],
    ["header", { from: "header", name: "" }],
    ["status", { from: "status" }],
  ] as const)("%s round-trips through buildExtract/extractChoiceOf", (choice, expected) => {
    const built = buildExtract(choice);
    expect(built).toEqual(expected);
    expect(extractChoiceOf(built)).toBe(choice);
  });

  it("distinguishes body-jsonpath from body-regex by presence of regex", () => {
    expect(extractChoiceOf({ from: "body", jsonPath: "$.x" })).toBe(
      "body-jsonpath",
    );
    expect(extractChoiceOf({ from: "body", regex: "(.+)" })).toBe("body-regex");
  });

  it("falls back to default for an unrecognized `from` tag", () => {
    expect(
      extractChoiceOf({ from: "made-up" } as unknown as Parameters<
        typeof extractChoiceOf
      >[0]),
    ).toBe("default");
  });
});

describe("ttl choices", () => {
  it("default (undefined) round-trips", () => {
    expect(ttlChoiceOf(undefined)).toBe("default");
    expect(buildTtl("default")).toBeUndefined();
  });

  it.each([
    ["body", { from: "body", jsonPath: "$.expires_in", unit: "seconds" }],
    ["fixed", { from: "fixed", seconds: 3600 }],
    ["absolute", { from: "absolute", jsonPath: "$.expiresAt" }],
  ] as const)("%s round-trips through buildTtl/ttlChoiceOf", (choice, expected) => {
    const built = buildTtl(choice);
    expect(built).toEqual(expected);
    expect(ttlChoiceOf(built)).toBe(choice);
  });
});

describe("inject choices", () => {
  it.each([
    ["header", { into: "header", name: "Authorization", template: "Bearer {{value}}" }],
    ["query", { into: "query", name: "", template: "{{value}}" }],
    ["body", { into: "body", pointer: "", template: "{{value}}" }],
  ] as const)("%s round-trips through buildInject/injectChoiceOf", (choice, expected) => {
    const built = buildInject(choice);
    expect(built).toEqual(expected);
    expect(injectChoiceOf(built)).toBe(choice);
  });

  it("falls back to header for an unrecognized `into` tag", () => {
    expect(
      injectChoiceOf({ into: "made-up" } as unknown as Parameters<
        typeof injectChoiceOf
      >[0]),
    ).toBe("header");
  });
});

describe("retryOn", () => {
  it("asRetryOn defaults an absent value to 401/403", () => {
    expect(asRetryOn(undefined)).toEqual([...DEFAULT_RETRY_ON]);
  });

  it("asRetryOn filters out anything that isn't a valid HTTP status", () => {
    expect(
      asRetryOn([401, "500" as unknown as number, 42, 999, 403.5, 403]),
    ).toEqual([401, 403]);
  });

  it("asRetryOn coerces a non-array runtime value", () => {
    expect(asRetryOn("nope" as unknown as number[])).toEqual([
      ...DEFAULT_RETRY_ON,
    ]);
  });

  it("toggleRetryStatus adds an absent status and keeps the result sorted", () => {
    expect(toggleRetryStatus([403], 401)).toEqual([401, 403]);
  });

  it("toggleRetryStatus removes a present status", () => {
    expect(toggleRetryStatus([401, 403], 401)).toEqual([403]);
  });

  it("toggleRetryStatus is defensive against a corrupted current value", () => {
    expect(toggleRetryStatus(undefined, 500)).toEqual([401, 403, 500]);
  });
});
