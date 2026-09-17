import { describe, expect, it } from "vitest";
import { setApiField } from "./fieldOrder";
import type { Api } from "../model";

// A document as minimal as a hand-written file can legally be — every
// `#[serde(default)]` field (`variables`, `environments`, `auth`,
// `history`) genuinely absent, exactly like model.test.ts's own `FILE`
// fixture. This is the shape that most exposes the bug: none of these
// fields have ever been inserted, so a naive `api.field = value` would
// append every one of them at the very end, after `endpoints`.
function minimalApi(): Api {
  return JSON.parse(
    JSON.stringify({
      schemaVersion: 1,
      id: "gw",
      name: "Gateway",
      baseUrl: "https://api.example.com",
      endpoints: [],
    }),
  );
}

describe("setApiField", () => {
  it("inserts a missing \"history\" between \"auth\" and \"endpoints\"", () => {
    const api = minimalApi();
    setApiField(api, "history", { storeBodies: false });
    expect(Object.keys(api)).toEqual([
      "schemaVersion",
      "id",
      "name",
      "baseUrl",
      "history",
      "endpoints",
    ]);
    // The finding this test exists to catch: history must NOT land after
    // endpoints.
    expect(Object.keys(api).indexOf("history")).toBeLessThan(
      Object.keys(api).indexOf("endpoints"),
    );
  });

  it("inserts a missing \"variables\" before \"environments\", \"auth\" and \"endpoints\"", () => {
    const api = minimalApi();
    setApiField(api, "variables", { base: "1" });
    const keys = Object.keys(api);
    expect(keys).toEqual(["schemaVersion", "id", "name", "baseUrl", "variables", "endpoints"]);
    expect(keys.indexOf("variables")).toBeLessThan(keys.indexOf("endpoints"));
  });

  it("inserts a missing \"environments\" before \"auth\" and \"endpoints\"", () => {
    const api = minimalApi();
    setApiField(api, "environments", [{ name: "prod", variables: {} }]);
    const keys = Object.keys(api);
    expect(keys.indexOf("environments")).toBeLessThan(keys.indexOf("endpoints"));
  });

  it("inserts a missing \"auth\" before \"endpoints\"", () => {
    const api = minimalApi();
    setApiField(api, "auth", { type: "none" });
    const keys = Object.keys(api);
    expect(keys.indexOf("auth")).toBeLessThan(keys.indexOf("endpoints"));
  });

  it("inserting all four in an arbitrary order still lands them in declared order relative to each other and to endpoints", () => {
    const api = minimalApi();
    setApiField(api, "history", { storeBodies: false });
    setApiField(api, "auth", { type: "none" });
    setApiField(api, "variables", {});
    setApiField(api, "environments", []);
    const keys = Object.keys(api);
    expect(keys).toEqual([
      "schemaVersion",
      "id",
      "name",
      "baseUrl",
      "variables",
      "environments",
      "auth",
      "history",
      "endpoints",
    ]);
  });

  it("updates an already-present field in place, without moving its key", () => {
    const api: Api = {
      schemaVersion: 1,
      id: "gw",
      name: "Gateway",
      baseUrl: "https://api.example.com",
      variables: { old: "1" },
      environments: [],
      auth: { type: "none" },
      history: { storeBodies: true },
      endpoints: [],
    };
    const keysBefore = Object.keys(api);
    setApiField(api, "variables", { fresh: "2" });
    expect(Object.keys(api)).toEqual(keysBefore);
    expect(api.variables).toEqual({ fresh: "2" });
  });

  it("correctly places a new field even when an intermediate declared field is itself absent", () => {
    // `environments` (between `variables` and `auth` in declared order) is
    // absent here, along with `variables` and `history` — only `auth` and
    // `endpoints` exist. Inserting `variables` must still land it AHEAD of
    // `auth`, not merely at the end: this function moves every EXISTING
    // field at or after the insertion point out of the way and back, so a
    // gap left by an absent field in between never breaks the relative
    // order of the fields that do exist.
    const api = JSON.parse(
      JSON.stringify({
        schemaVersion: 1,
        id: "gw",
        name: "Gateway",
        baseUrl: "https://api.example.com",
        auth: { type: "none" },
        endpoints: [],
      }),
    ) as Api;
    setApiField(api, "variables", {});
    expect(Object.keys(api)).toEqual([
      "schemaVersion",
      "id",
      "name",
      "baseUrl",
      "variables",
      "auth",
      "endpoints",
    ]);
  });
});
