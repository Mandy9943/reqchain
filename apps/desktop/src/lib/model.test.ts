import { describe, expect, it } from "vitest";
import { emptyApi, emptyEndpoint, parseApi, serializeApi } from "./model";

// Note: formatted to match the real pretty-printer (`JSON.stringify(api,
// null, 2)`, same shape `serde_json::to_string_pretty` produces on the
// Rust side) — every nested object gets its own lines, never collapsed
// onto one line, so this is what a genuine round trip actually yields.
const FILE = `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": [
    {
      "id": "token",
      "name": "Token",
      "method": "POST",
      "path": "/token"
    }
  ]
}
`;

describe("parseApi", () => {
  it("round-trips a file byte for byte when nothing is changed", () => {
    const parsed = parseApi(FILE);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(serializeApi(parsed.api)).toBe(FILE);
  });

  it("reports malformed JSON instead of throwing", () => {
    const parsed = parseApi("{ nope");
    expect(parsed.ok).toBe(false);
    if (parsed.ok) return;
    expect(parsed.error).toMatch(/JSON|token|position/i);
  });

  it("rejects a document that is not an API", () => {
    expect(parseApi("[1,2,3]").ok).toBe(false);
    expect(parseApi('{"schemaVersion":1}').ok).toBe(false); // no id/name/baseUrl/endpoints
  });

  it("preserves a chained auth block through a round trip", () => {
    const text = serializeApi({
      schemaVersion: 1, id: "a", name: "A", baseUrl: "https://x",
      endpoints: [
        { id: "token", name: "T", method: "POST", path: "/token" },
        { id: "biz", name: "B", method: "GET", path: "/b",
          auth: { type: "chained", source: { endpoint: "token" },
                  inject: { into: "header", name: "Authorization",
                            template: "Bearer {{value}}" }, retryOn: [401] } },
      ],
    });
    const back = parseApi(text);
    expect(back.ok).toBe(true);
    if (!back.ok) return;
    expect(back.api.endpoints[1].auth).toEqual({
      type: "chained", source: { endpoint: "token" },
      inject: { into: "header", name: "Authorization", template: "Bearer {{value}}" },
      retryOn: [401],
    });
  });
});

describe("skeletons", () => {
  it("emptyApi produces something that parses and has no endpoints", () => {
    const api = emptyApi("new-api", "New API");
    const back = parseApi(serializeApi(api));
    expect(back.ok).toBe(true);
    if (!back.ok) return;
    expect(back.api.endpoints).toEqual([]);
    expect(back.api.schemaVersion).toBe(1);
  });

  it("emptyEndpoint defaults to GET and a root path", () => {
    const ep = emptyEndpoint("ping", "Ping");
    expect(ep.method).toBe("GET");
    expect(ep.path.startsWith("/")).toBe(true);
  });
});
