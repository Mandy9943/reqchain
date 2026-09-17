import { describe, expect, it } from "vitest";
import { setApiField, setEndpointField } from "./fieldOrder";
import type { Api, Endpoint } from "../model";

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

// A hand/agent-written endpoint as minimal as `Endpoint.headers`/`.query`/
// `.variables`/`.auth` legally allow (all `#[serde(default)]`, no
// `skip_serializing_if`) — none of these keys have ever been inserted, so a
// naive `ep.field = value` would append every one of them at the very end,
// after `body` would go (or wherever it currently sits), reordering the
// whole endpoint object in `git diff` the moment the Form tab touches it.
function minimalEndpoint(): Endpoint {
  return JSON.parse(
    JSON.stringify({
      id: "e",
      name: "E",
      method: "GET",
      path: "/",
    }),
  );
}

describe("setEndpointField", () => {
  it('inserts a missing "headers" between "path" and "query"', () => {
    const ep = minimalEndpoint();
    setEndpointField(ep, "headers", { Accept: "json" });
    expect(Object.keys(ep)).toEqual(["id", "name", "method", "path", "headers"]);
  });

  it('inserts a missing "query" after "headers" (and before "variables"/"auth" if present)', () => {
    const ep = minimalEndpoint();
    setEndpointField(ep, "headers", {});
    setEndpointField(ep, "query", { verbose: "true" });
    expect(Object.keys(ep)).toEqual(["id", "name", "method", "path", "headers", "query"]);
  });

  it('inserts a missing "auth" before "body" when body already exists', () => {
    const ep = minimalEndpoint();
    (ep as Endpoint).body = { type: "text", content: "hi" };
    setEndpointField(ep, "auth", { type: "none" });
    const keys = Object.keys(ep);
    expect(keys.indexOf("auth")).toBeLessThan(keys.indexOf("body"));
    expect(keys).toEqual(["id", "name", "method", "path", "auth", "body"]);
  });

  it("inserting headers/query/variables/auth in an arbitrary order still lands them in declared order relative to each other and to an existing body", () => {
    const ep = minimalEndpoint();
    (ep as Endpoint).body = { type: "text", content: "hi" };
    setEndpointField(ep, "auth", { type: "none" });
    setEndpointField(ep, "variables", {});
    setEndpointField(ep, "query", {});
    setEndpointField(ep, "headers", {});
    expect(Object.keys(ep)).toEqual([
      "id",
      "name",
      "method",
      "path",
      "headers",
      "query",
      "variables",
      "auth",
      "body",
    ]);
  });

  it("updates an already-present field in place, without moving its key", () => {
    const ep: Endpoint = {
      id: "e",
      name: "E",
      method: "GET",
      path: "/",
      headers: { old: "1" },
      query: {},
      variables: {},
      auth: { type: "inherit" },
    };
    const keysBefore = Object.keys(ep);
    setEndpointField(ep, "headers", { fresh: "2" });
    expect(Object.keys(ep)).toEqual(keysBefore);
    expect(ep.headers).toEqual({ fresh: "2" });
  });

  it("correctly places a new field even when an intermediate declared field is itself absent", () => {
    // `query` (between `headers` and `variables` in declared order) is
    // absent here, along with `variables` — only `headers` and `auth`
    // exist. Inserting `variables` must still land it between `headers`
    // and `auth`, not merely at the end.
    const ep = JSON.parse(
      JSON.stringify({
        id: "e",
        name: "E",
        method: "GET",
        path: "/",
        headers: {},
        auth: { type: "inherit" },
      }),
    ) as Endpoint;
    setEndpointField(ep, "variables", {});
    expect(Object.keys(ep)).toEqual([
      "id",
      "name",
      "method",
      "path",
      "headers",
      "variables",
      "auth",
    ]);
  });
});
