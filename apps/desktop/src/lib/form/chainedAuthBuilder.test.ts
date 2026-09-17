import { describe, expect, it } from "vitest";
import {
  asChained,
  asRetryOn,
  buildExtract,
  buildInject,
  buildTtl,
  chainWalk,
  currentSourceProblem,
  DEFAULT_RETRY_ON,
  excludedSourceCandidates,
  extraRetryStatuses,
  extractChoiceOf,
  injectChoiceOf,
  MAX_DEPTH,
  reachesInheritedEndpoint,
  sourceExclusionReason,
  sourceProblemLabel,
  toggleRetryStatus,
  ttlChoiceOf,
  validSourceEndpoints,
  wouldExceedDepth,
  XPATH_UNSUPPORTED_MESSAGE,
} from "./chainedAuthBuilder";
import { emptyApi, emptyEndpoint, type Api, type Auth, type Endpoint } from "../model";

function ep(id: string, auth: Auth): Endpoint {
  return { ...emptyEndpoint(id, id), auth };
}

/** An endpoint whose `auth` key is genuinely ABSENT — not `undefined` typed
 * away, but literally missing, the way a hand-written or agent-written file
 * that never mentions `auth` at all parses (per `SPEC.md`, `model.rs`'s
 * `#[serde(default = "Auth::inherit")]`, and `parseApi`'s own test coverage
 * for exactly this). `model.ts`'s `Endpoint.auth: Auth` type claims this
 * can't happen; the cast below is the same "the compile-time type lies
 * about what `dto_for_api` can genuinely hand this module" gap `authEditor
 * .ts`'s `resolveAuth` and this module's `reachesInheritedEndpoint` guard
 * against. */
function epNoAuth(id: string): Endpoint {
  const withAuth = ep(id, { type: "inherit" });
  const raw = withAuth as unknown as Record<string, unknown>;
  delete raw.auth;
  return raw as unknown as Endpoint;
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

  it("does not throw and treats a genuinely absent auth key as inherit — CRITICAL: an agent-written file omitting `auth` is valid (SPEC.md, model.rs's serde default), and dto_for_api hands parseApi the raw on-disk text unnormalized", () => {
    const api = apiWith([epNoAuth("a"), ep("b", { type: "none" })], chainedTo("b"));
    expect(() => chainWalk(api, "a")).not.toThrow();
    expect(chainWalk(api, "a")).toEqual(["a", "b"]);
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

  it("does not throw and treats a genuinely absent auth key as inherit", () => {
    const api = apiWith([ep("a", chainedTo("b")), epNoAuth("b")]);
    expect(() => reachesInheritedEndpoint(api, "a")).not.toThrow();
    expect(reachesInheritedEndpoint(api, "a")).toBe(true);
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

  it("does NOT exclude a candidate whose own auth is inherit but whose resolved chain never cycles back", () => {
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

// HIGH review finding: depth was not modelled at all — a candidate whose
// chain is already at the engine's MAX_DEPTH limit was offered anyway,
// letting the builder construct exactly the configuration
// `reqchain validate` rejects as an error and `save_api` refuses to write.
// Topology below matches the SHIPPED fixture `tests/fixtures/chain-too-
// deep.json` exactly (a -> b -> c -> d -> e -> f, f terminal).
describe("depth — mirrors crates/core/src/chain.rs::MAX_DEPTH and validate.rs::check_cycles", () => {
  function deepChainApi(length: number): Api {
    const ids = Array.from({ length }, (_, i) => String.fromCharCode(97 + i)); // a, b, c, ...
    const endpoints = ids.map((id, i) =>
      i === ids.length - 1
        ? ep(id, { type: "none" })
        : ep(id, chainedTo(ids[i + 1])),
    );
    return apiWith(endpoints);
  }

  it("wouldExceedDepth is false when the resulting total is exactly MAX_DEPTH", () => {
    // a -> b -> c -> d -> e (5 endpoints, e terminal). `wouldExceedDepth`
    // measures "1 (the endpoint that would point at this candidate) +
    // candidate's own walk" — so the candidate whose own walk is exactly
    // 4 long (b -> c -> d -> e) makes a total of 5, still within
    // MAX_DEPTH, not one past it.
    const api = deepChainApi(5);
    expect(chainWalk(api, "b")).toHaveLength(4);
    expect(wouldExceedDepth(api, "b")).toBe(false);
  });

  it("wouldExceedDepth is true the instant a chain exceeds MAX_DEPTH", () => {
    // a -> b -> c -> d -> e -> f (6 endpoints) — one past the limit, exactly
    // tests/fixtures/chain-too-deep.json's topology.
    const api = deepChainApi(6);
    expect(wouldExceedDepth(api, "a")).toBe(true);
  });

  it("excludes a source that would recreate chain-too-deep.json's exact rejected configuration", () => {
    const api = deepChainApi(6); // a -> b -> c -> d -> e -> f
    // Editing `a`'s own auth (it already points at `b`): offering `b` again
    // would just be today's file, which the engine ALREADY rejects with
    // "auth chain deeper than 5 levels: a -> b -> c -> d -> e -> f".
    const sources = validSourceEndpoints(api, "a").map((e) => e.id);
    expect(sources).not.toContain("b");
  });

  it("wouldExceedDepth counts a dangling source, matching validate.rs's check_cycles (which pushes before checking existence)", () => {
    const api = apiWith([
      ep("a", chainedTo("b")),
      ep("b", chainedTo("c")),
      ep("c", chainedTo("d")),
      ep("d", chainedTo("e")),
      ep("e", chainedTo("does-not-exist")),
    ]);
    // chainWalk(api, "a") = [a, b, c, d, e, does-not-exist] — 6 entries.
    expect(wouldExceedDepth(api, "a")).toBe(true);
  });

  it("MAX_DEPTH is exactly 5, matching crates/core/src/chain.rs", () => {
    expect(MAX_DEPTH).toBe(5);
  });
});

// MEDIUM review finding: the dropdown silently rendered "Select an
// endpoint…" for a document whose CURRENT source already forms a cycle or
// dangles — telling the user there is no source when the file says
// otherwise. `currentSourceProblem`/`excludedSourceCandidates` are what let
// the component show the real, named problem instead of hiding it.
describe("currentSourceProblem / excludedSourceCandidates / sourceProblemLabel", () => {
  it("is null for an empty (unset) source", () => {
    const api = apiWith([ep("a", { type: "none" })]);
    expect(currentSourceProblem(api, "a", "")).toBeNull();
  });

  it("is null for a source that is already valid", () => {
    const api = apiWith([ep("a", { type: "none" }), ep("b", { type: "none" })]);
    expect(currentSourceProblem(api, "b", "a")).toBeNull();
  });

  it("reports \"missing\" for a dangling source — tests/fixtures/chain-missing.json's exact shape", () => {
    const api = apiWith([ep("business", chainedTo("nonexistent"))]);
    expect(currentSourceProblem(api, "business", "nonexistent")).toBe("missing");
    expect(sourceProblemLabel("nonexistent", "missing")).toBe(
      "nonexistent — no such endpoint",
    );
  });

  it("reports \"cycle\" for a source that already cycles — tests/fixtures/chain-cycle.json's exact shape (a <-> b)", () => {
    const api = apiWith([ep("a", chainedTo("b")), ep("b", chainedTo("a"))]);
    // Opening `a`'s own auth: its current source is `b`, and `b` already
    // points back at `a` — a real, already-on-file cycle.
    expect(currentSourceProblem(api, "a", "b")).toBe("cycle");
    expect(sourceProblemLabel("b", "cycle")).toBe("b — forms a cycle");
  });

  it("reports \"too-deep\" for a source already past MAX_DEPTH", () => {
    const ids = ["a", "b", "c", "d", "e", "f"];
    const endpoints = ids.map((id, i) =>
      i === ids.length - 1 ? ep(id, { type: "none" }) : ep(id, chainedTo(ids[i + 1])),
    );
    const api = apiWith(endpoints);
    expect(currentSourceProblem(api, "a", "b")).toBe("too-deep");
    expect(sourceProblemLabel("b", "too-deep")).toBe(
      "b — chain too deep (max 5 endpoints)",
    );
  });

  it("sourceExclusionReason distinguishes self from cycle, but sourceProblemLabel describes both as a cycle", () => {
    const api = apiWith([ep("a", { type: "inherit" })]);
    expect(sourceExclusionReason(api, "a", "a")).toBe("self");
    expect(sourceProblemLabel("a", "self")).toBe("a — forms a cycle");
  });

  it("excludedSourceCandidates names every excluded endpoint and reason, omitting the endpoint being edited itself", () => {
    const api = apiWith([
      ep("a", chainedTo("b")),
      ep("b", chainedTo("a")),
      ep("c", { type: "none" }),
    ]);
    // Editing `a`: `b` cycles back to `a`; `c` is fine (not excluded, so not
    // listed); `a` itself is never listed even though it "excludes" itself.
    const excluded = excludedSourceCandidates(api, "a");
    expect(excluded).toEqual([{ id: "b", name: "b", reason: "cycle" }]);
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

  // MEDIUM review finding: an xpath extract was silently shown as
  // "Body — JSONPath" with an empty field, misrepresenting
  // tests/fixtures/chain.json's `xpath-business` endpoint entirely, and
  // xpath was checked AFTER regex where `chain.rs`'s `extract_value` checks
  // it FIRST (xpath wins even when a regex is also present).
  describe("xpath — matches crates/core/src/chain.rs's extract_value precedence", () => {
    it("recognizes an xpath extract explicitly, not as JSONPath", () => {
      expect(extractChoiceOf({ from: "body", xpath: "//token" })).toBe(
        "body-xpath",
      );
    });

    it("xpath takes precedence over a simultaneously-present regex — matches the engine checking xpath first", () => {
      expect(
        extractChoiceOf({ from: "body", xpath: "//token", regex: "(.+)" }),
      ).toBe("body-xpath");
    });

    it("xpath takes precedence over a simultaneously-present jsonPath", () => {
      expect(
        extractChoiceOf({ from: "body", xpath: "//token", jsonPath: "$.x" }),
      ).toBe("body-xpath");
    });

    it("buildExtract never manufactures an xpath extract on its own (not a normal, selectable choice)", () => {
      const built = buildExtract("body-xpath");
      expect(built).not.toHaveProperty("xpath");
    });

    it("XPATH_UNSUPPORTED_MESSAGE matches the engine's exact wording", () => {
      expect(XPATH_UNSUPPORTED_MESSAGE).toBe(
        "xpath extraction is not implemented yet — use jsonPath or regex",
      );
    });
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

  // LOW review finding: a retryOn value outside the eight curated chips
  // (e.g. a JSON-tab edit adding 504) was preserved by every function here
  // but had no chip to render it — invisible in the UI while still real in
  // the file.
  describe("extraRetryStatuses", () => {
    it("is empty when every status has a chip", () => {
      expect(extraRetryStatuses([401, 403])).toEqual([]);
    });

    it("surfaces a status with no chip", () => {
      expect(extraRetryStatuses([401, 504])).toEqual([504]);
    });

    it("defaults an absent value the same way asRetryOn does (no extras)", () => {
      expect(extraRetryStatuses(undefined)).toEqual([]);
    });
  });
});
