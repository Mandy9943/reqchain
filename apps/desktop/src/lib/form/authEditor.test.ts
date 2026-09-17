import { describe, expect, it } from "vitest";
import {
  authTypeOf,
  COMPUTED_FUNCTIONS,
  defaultAuthForType,
  inheritLabel,
  resolveAuth,
  type AuthType,
} from "./authEditor";
import { emptyApi, type Api, type Auth } from "../model";

function apiWithAuth(auth: Auth): Api {
  return { ...emptyApi("a", "A"), auth };
}

describe("authTypeOf", () => {
  it("is \"none\" for an undefined auth", () => {
    expect(authTypeOf(undefined)).toBe("none");
  });

  it.each([
    "inherit",
    "none",
    "basic",
    "bearer",
    "header",
    "computed",
    "chained",
  ] as const)("recognizes the %s variant", (type) => {
    const auth = defaultAuthForType(type, "token");
    expect(authTypeOf(auth)).toBe(type);
  });

  it("falls back to \"none\" for an unrecognized runtime type tag", () => {
    expect(authTypeOf({ type: "totally-made-up" } as unknown as Auth)).toBe(
      "none",
    );
  });

  it("falls back to \"none\" for a missing type tag entirely", () => {
    expect(authTypeOf({} as unknown as Auth)).toBe("none");
  });
});

describe("resolveAuth", () => {
  it("returns a non-inherit auth unchanged", () => {
    const api = apiWithAuth({ type: "none" });
    const bearer: Auth = { type: "bearer", token: "x" };
    expect(resolveAuth(api, bearer)).toBe(bearer);
  });

  it("resolves inherit to the API's own auth", () => {
    const api = apiWithAuth({ type: "bearer", token: "api-level" });
    expect(resolveAuth(api, { type: "inherit" })).toEqual({
      type: "bearer",
      token: "api-level",
    });
  });

  it("resolves inherit to none when the API's own auth is ALSO inherit (nothing above it)", () => {
    const api = apiWithAuth({ type: "inherit" });
    expect(resolveAuth(api, { type: "inherit" })).toEqual({ type: "none" });
  });
});

describe("inheritLabel", () => {
  it("names what inherit resolves to", () => {
    const chainedApi = apiWithAuth({
      type: "chained",
      source: { endpoint: "token" },
      inject: { into: "header", name: "Authorization", template: "Bearer {{value}}" },
      retryOn: [401, 403],
    });
    expect(inheritLabel(chainedApi)).toBe("inherit (chained, from the API)");
    expect(inheritLabel(apiWithAuth({ type: "none" }))).toBe(
      "inherit (none, from the API)",
    );
  });

  it("says none when the API's own auth is inherit (nothing above it)", () => {
    expect(inheritLabel(apiWithAuth({ type: "inherit" }))).toBe(
      "inherit (none, from the API)",
    );
  });
});

describe("defaultAuthForType", () => {
  it("builds inherit/none with no fields", () => {
    expect(defaultAuthForType("inherit")).toEqual({ type: "inherit" });
    expect(defaultAuthForType("none")).toEqual({ type: "none" });
  });

  it("builds basic with empty username/password in declared field order", () => {
    const auth = defaultAuthForType("basic");
    expect(auth).toEqual({ type: "basic", username: "", password: "" });
    expect(Object.keys(auth)).toEqual(["type", "username", "password"]);
  });

  it("builds bearer with an empty token", () => {
    expect(defaultAuthForType("bearer")).toEqual({ type: "bearer", token: "" });
  });

  it("builds header with empty headers", () => {
    const auth = defaultAuthForType("header");
    expect(auth).toEqual({ type: "header", headers: {} });
  });

  it("builds computed with empty name/expression", () => {
    expect(defaultAuthForType("computed")).toEqual({
      type: "computed",
      name: "",
      expression: "",
    });
  });

  it("builds chained with the documented defaults, explicit (not omitted), and the given source", () => {
    const auth = defaultAuthForType("chained", "token");
    expect(auth).toEqual({
      type: "chained",
      source: { endpoint: "token" },
      extract: { from: "body", jsonPath: "$.access_token" },
      ttl: { from: "body", jsonPath: "$.expires_in", unit: "seconds" },
      inject: {
        into: "header",
        name: "Authorization",
        template: "Bearer {{value}}",
      },
      retryOn: [401, 403],
    });
    expect(Object.keys(auth)).toEqual([
      "type",
      "source",
      "extract",
      "ttl",
      "inject",
      "retryOn",
    ]);
  });

  it("builds chained with an empty source when no candidate is given", () => {
    const auth = defaultAuthForType("chained");
    expect(auth.type).toBe("chained");
    if (auth.type === "chained") {
      expect(auth.source).toEqual({ endpoint: "" });
    }
  });

  it("every type builder produces a value authTypeOf recognizes as itself", () => {
    const types: AuthType[] = [
      "inherit",
      "none",
      "basic",
      "bearer",
      "header",
      "computed",
      "chained",
    ];
    for (const t of types) {
      expect(authTypeOf(defaultAuthForType(t))).toBe(t);
    }
  });
});

describe("COMPUTED_FUNCTIONS", () => {
  it("lists the closed function set from design spec §6", () => {
    const joined = COMPUTED_FUNCTIONS.join(" ");
    for (const fn of ["md5", "sha1", "sha256", "base64", "now"]) {
      expect(joined).toContain(fn);
    }
  });
});
