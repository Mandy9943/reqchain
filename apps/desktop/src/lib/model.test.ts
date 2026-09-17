import { describe, expect, it } from "vitest";
import { emptyApi, emptyEndpoint, parseApi, serializeApi, slugify } from "./model";

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

  // HIGH review finding: the Form tab (EndpointForm/KeyValueRows) does
  // `Object.entries(endpoint.headers)` etc. — a JSON-tab edit that sets one
  // of these to `null` (or any non-object) must be refused here, at parse
  // time, rather than being accepted as `ok: true` and crashing the form
  // the moment it renders.
  describe("per-endpoint headers/query/variables shape", () => {
    function apiWith(endpointExtra: string): string {
      return `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": [
    { "id": "e", "name": "E", "method": "GET", "path": "/", ${endpointExtra} }
  ]
}`;
    }

    it.each(["headers", "query", "variables"] as const)(
      "rejects %s set to null",
      (field) => {
        const parsed = parseApi(apiWith(`"${field}": null`));
        expect(parsed.ok).toBe(false);
        if (parsed.ok) return;
        expect(parsed.error).toContain(field);
      },
    );

    it.each(["headers", "query", "variables"] as const)(
      "rejects %s set to an array",
      (field) => {
        const parsed = parseApi(apiWith(`"${field}": []`));
        expect(parsed.ok).toBe(false);
      },
    );

    it.each(["headers", "query", "variables"] as const)(
      "rejects %s set to a string",
      (field) => {
        const parsed = parseApi(apiWith(`"${field}": "oops"`));
        expect(parsed.ok).toBe(false);
      },
    );

    it("accepts a document where headers/query/variables are simply absent (older/hand-written fixtures)", () => {
      // Deliberately lax here: this is what the FILE fixture above already
      // relies on (no headers/query/variables on its one endpoint at all).
      // Downstream (KeyValueRows' `asRecord`) treats an absent field as
      // `{}` defensively rather than requiring parseApi to reject it.
      const parsed = parseApi(apiWith('"unrelated": true'));
      expect(parsed.ok).toBe(true);
    });

    it("rejects a non-object entry inside endpoints", () => {
      const text = `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": ["not an object"]
}`;
      expect(parseApi(text).ok).toBe(false);
    });
  });

  describe("per-endpoint body shape", () => {
    function apiWithBody(body: string): string {
      return `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": [
    { "id": "e", "name": "E", "method": "GET", "path": "/", "body": ${body} }
  ]
}`;
    }

    it("accepts a document where body is simply absent", () => {
      const text = `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": [
    { "id": "e", "name": "E", "method": "GET", "path": "/" }
  ]
}`;
      expect(parseApi(text).ok).toBe(true);
    });

    it("rejects a body that is not an object", () => {
      expect(parseApi(apiWithBody('"oops"')).ok).toBe(false);
      expect(parseApi(apiWithBody("[]")).ok).toBe(false);
      expect(parseApi(apiWithBody("null")).ok).toBe(false);
    });

    it("rejects a body with an unknown type tag", () => {
      const parsed = parseApi(apiWithBody('{ "type": "yaml" }'));
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain("body.type");
    });

    it("accepts a json body with any content shape", () => {
      expect(
        parseApi(apiWithBody('{ "type": "json", "content": { "a": [1, null] } }')).ok,
      ).toBe(true);
      expect(parseApi(apiWithBody('{ "type": "json", "content": "x" }')).ok).toBe(
        true,
      );
    });

    it.each(["text", "xml"] as const)(
      "rejects a %s body whose content is not a string",
      (type) => {
        const parsed = parseApi(apiWithBody(`{ "type": "${type}", "content": 5 }`));
        expect(parsed.ok).toBe(false);
        if (parsed.ok) return;
        expect(parsed.error).toContain("content");
      },
    );

    it.each(["text", "xml"] as const)("accepts a well-shaped %s body", (type) => {
      expect(
        parseApi(apiWithBody(`{ "type": "${type}", "content": "hi" }`)).ok,
      ).toBe(true);
    });

    it("rejects a binary body whose path is not a string", () => {
      const parsed = parseApi(apiWithBody('{ "type": "binary", "path": 5 }'));
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain("path");
    });

    it("accepts a well-shaped binary body", () => {
      expect(
        parseApi(apiWithBody('{ "type": "binary", "path": "/tmp/x" }')).ok,
      ).toBe(true);
    });

    it.each(["null", "[]", '"oops"'])(
      "rejects a form body whose fields is %s",
      (fields) => {
        const parsed = parseApi(apiWithBody(`{ "type": "form", "fields": ${fields} }`));
        expect(parsed.ok).toBe(false);
      },
    );

    it("accepts a well-shaped form body", () => {
      expect(
        parseApi(apiWithBody('{ "type": "form", "fields": { "a": "1" } }')).ok,
      ).toBe(true);
    });

    it.each(["fields", "files"] as const)(
      "rejects a multipart body whose %s is not an object",
      (field) => {
        const parsed = parseApi(
          apiWithBody(`{ "type": "multipart", "fields": {}, "files": {}, "${field}": null }`),
        );
        expect(parsed.ok).toBe(false);
        if (parsed.ok) return;
        expect(parsed.error).toContain(field);
      },
    );

    it("accepts a well-shaped multipart body", () => {
      expect(
        parseApi(
          apiWithBody('{ "type": "multipart", "fields": { "a": "1" }, "files": { "f": "/tmp/x" } }'),
        ).ok,
      ).toBe(true);
    });

    // `model.rs`'s `Body::Json { content }`, `Body::Form { fields }` and
    // `Body::Multipart { fields }` have NO `#[serde(default)]` — serde
    // requires them, unlike headers/query/variables above (which are all
    // `#[serde(default)]` and so tolerate simply being absent). A document
    // missing one of these would still pass the old (pre-fix) shape check,
    // render fine in the form, and then fail at Save with a raw Rust
    // `missing field ...` error — this is the case these tests pin.
    it("rejects a json body with content simply absent", () => {
      const parsed = parseApi(apiWithBody('{ "type": "json" }'));
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain("content");
    });

    it("accepts a json body whose content is explicitly null (a real JSON value, not absence)", () => {
      expect(parseApi(apiWithBody('{ "type": "json", "content": null }')).ok).toBe(
        true,
      );
    });

    it("rejects a form body with fields simply absent", () => {
      const parsed = parseApi(apiWithBody('{ "type": "form" }'));
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain("fields");
    });

    it("rejects a multipart body with fields simply absent", () => {
      const parsed = parseApi(apiWithBody('{ "type": "multipart", "files": {} }'));
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain("fields");
    });

    it("accepts a multipart body with files simply absent (Multipart.files HAS #[serde(default)])", () => {
      expect(
        parseApi(apiWithBody('{ "type": "multipart", "fields": {} }')).ok,
      ).toBe(true);
    });
  });

  // Task 5's auth editor: same reasoning as the body-shape block above — a
  // JSON-tab edit can set `auth` (API-level or per-endpoint) to anything at
  // all, and `AuthEditor`/`ChainedAuthBuilder` must never be handed a
  // document this check would have caught.
  describe("auth shape", () => {
    function apiWithAuth(auth: string): string {
      return `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": [
    { "id": "e", "name": "E", "method": "GET", "path": "/", "auth": ${auth} }
  ]
}`;
    }

    it("accepts an endpoint with auth simply absent (defaults to inherit)", () => {
      const text = `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": [
    { "id": "e", "name": "E", "method": "GET", "path": "/" }
  ]
}`;
      expect(parseApi(text).ok).toBe(true);
    });

    it("accepts an API with auth simply absent (defaults to none)", () => {
      const text = `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": []
}`;
      expect(parseApi(text).ok).toBe(true);
    });

    it.each(['"oops"', "[]", "null", "1"])("rejects auth set to %s", (bad) => {
      expect(parseApi(apiWithAuth(bad)).ok).toBe(false);
    });

    it("rejects an unrecognized auth type tag", () => {
      const parsed = parseApi(apiWithAuth('{ "type": "made-up" }'));
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain("type");
    });

    it("accepts inherit and none with no other fields", () => {
      expect(parseApi(apiWithAuth('{ "type": "inherit" }')).ok).toBe(true);
      expect(parseApi(apiWithAuth('{ "type": "none" }')).ok).toBe(true);
    });

    it.each(["username", "password"])("rejects basic missing %s", (missing) => {
      const fields = { username: '"u"', password: '"p"' };
      delete (fields as Record<string, string>)[missing];
      const json = `{ "type": "basic", ${Object.entries(fields)
        .map(([k, v]) => `"${k}": ${v}`)
        .join(", ")} }`;
      const parsed = parseApi(apiWithAuth(json));
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain(missing);
    });

    it("accepts a well-shaped basic auth", () => {
      expect(
        parseApi(apiWithAuth('{ "type": "basic", "username": "u", "password": "p" }'))
          .ok,
      ).toBe(true);
    });

    it("rejects bearer missing token", () => {
      expect(parseApi(apiWithAuth('{ "type": "bearer" }')).ok).toBe(false);
    });

    it("rejects header auth whose headers is not an object", () => {
      expect(parseApi(apiWithAuth('{ "type": "header", "headers": null }')).ok).toBe(
        false,
      );
      expect(parseApi(apiWithAuth('{ "type": "header" }')).ok).toBe(false);
    });

    it("accepts a well-shaped header auth", () => {
      expect(
        parseApi(apiWithAuth('{ "type": "header", "headers": { "X-Api-Key": "1" } }'))
          .ok,
      ).toBe(true);
    });

    // MEDIUM review finding: header VALUES weren't checked to be strings —
    // `model.rs`'s `Header { headers: BTreeMap<String, String> }` requires
    // it, and a non-string value would render fine in this form (JS
    // coerces it) but fail at Save with a raw serde type error.
    it("rejects header auth whose headers has a non-string value", () => {
      const parsed = parseApi(
        apiWithAuth('{ "type": "header", "headers": { "X-Api-Key": 1 } }'),
      );
      expect(parsed.ok).toBe(false);
      if (parsed.ok) return;
      expect(parsed.error).toContain("X-Api-Key");
    });

    it.each(["name", "expression"])("rejects computed missing %s", (missing) => {
      const fields = { name: '"h"', expression: '"1"' };
      delete (fields as Record<string, string>)[missing];
      const json = `{ "type": "computed", ${Object.entries(fields)
        .map(([k, v]) => `"${k}": ${v}`)
        .join(", ")} }`;
      expect(parseApi(apiWithAuth(json)).ok).toBe(false);
    });

    describe("chained", () => {
      const source = '"source": { "endpoint": "token" }';
      const inject =
        '"inject": { "into": "header", "name": "Authorization", "template": "Bearer {{value}}" }';

      it("accepts the minimal shape — source and inject only, extract/ttl/retryOn all absent", () => {
        const parsed = parseApi(
          apiWithAuth(`{ "type": "chained", ${source}, ${inject} }`),
        );
        expect(parsed.ok).toBe(true);
      });

      it("rejects a missing source", () => {
        const parsed = parseApi(apiWithAuth(`{ "type": "chained", ${inject} }`));
        expect(parsed.ok).toBe(false);
        if (parsed.ok) return;
        expect(parsed.error).toContain("source");
      });

      it("rejects a source that is not an object", () => {
        expect(
          parseApi(
            apiWithAuth(`{ "type": "chained", "source": "token", ${inject} }`),
          ).ok,
        ).toBe(false);
      });

      it("rejects a source whose endpoint is not a string", () => {
        expect(
          parseApi(
            apiWithAuth(
              `{ "type": "chained", "source": { "endpoint": 1 }, ${inject} }`,
            ),
          ).ok,
        ).toBe(false);
      });

      it("rejects a missing inject", () => {
        const parsed = parseApi(apiWithAuth(`{ "type": "chained", ${source} }`));
        expect(parsed.ok).toBe(false);
        if (parsed.ok) return;
        expect(parsed.error).toContain("inject");
      });

      it.each([
        '{ "into": "header", "template": "Bearer {{value}}" }', // missing name
        '{ "into": "header", "name": "Authorization" }', // missing template
        '{ "into": "query" }', // missing name (template is optional)
        '{ "into": "body", "template": "{{value}}" }', // missing pointer
        '{ "into": "made-up" }',
      ])("rejects a malformed inject %s", (badInject) => {
        expect(
          parseApi(
            apiWithAuth(`{ "type": "chained", ${source}, "inject": ${badInject} }`),
          ).ok,
        ).toBe(false);
      });

      it("accepts inject.query with template absent (template HAS a serde default)", () => {
        expect(
          parseApi(
            apiWithAuth(
              `{ "type": "chained", ${source}, "inject": { "into": "query", "name": "token" } }`,
            ),
          ).ok,
        ).toBe(true);
      });

      it.each([
        '{ "from": "body", "jsonPath": "$.access_token" }',
        '{ "from": "body", "regex": "(.+)" }',
        '{ "from": "header", "name": "X-Token" }',
        '{ "from": "status" }',
      ])("accepts a well-shaped extract %s", (goodExtract) => {
        expect(
          parseApi(
            apiWithAuth(
              `{ "type": "chained", ${source}, "extract": ${goodExtract}, ${inject} }`,
            ),
          ).ok,
        ).toBe(true);
      });

      it.each([
        '{ "from": "header" }', // missing name
        '{ "from": "made-up" }',
      ])("rejects a malformed extract %s", (badExtract) => {
        expect(
          parseApi(
            apiWithAuth(
              `{ "type": "chained", ${source}, "extract": ${badExtract}, ${inject} }`,
            ),
          ).ok,
        ).toBe(false);
      });

      it.each([
        '{ "from": "body", "jsonPath": "$.expires_in" }',
        '{ "from": "body", "jsonPath": "$.expires_in", "unit": "milliseconds" }',
        '{ "from": "fixed", "seconds": 3600 }',
        '{ "from": "absolute", "jsonPath": "$.expiresAt" }',
      ])("accepts a well-shaped ttl %s", (goodTtl) => {
        expect(
          parseApi(
            apiWithAuth(`{ "type": "chained", ${source}, "ttl": ${goodTtl}, ${inject} }`),
          ).ok,
        ).toBe(true);
      });

      it.each([
        '{ "from": "body" }', // missing jsonPath
        '{ "from": "body", "jsonPath": "$.x", "unit": "days" }', // invalid unit
        '{ "from": "fixed" }', // missing seconds
        '{ "from": "made-up" }',
      ])("rejects a malformed ttl %s", (badTtl) => {
        expect(
          parseApi(
            apiWithAuth(`{ "type": "chained", ${source}, "ttl": ${badTtl}, ${inject} }`),
          ).ok,
        ).toBe(false);
      });

      // MEDIUM review finding: `ttl.fixed.seconds` is `u64` in `model.rs`
      // (a non-negative integer) but was only checked as `typeof ===
      // "number"`, so a negative or fractional value passed here and would
      // have failed only at Save time.
      it.each(['{ "from": "fixed", "seconds": -1 }', '{ "from": "fixed", "seconds": 3.5 }'])(
        "rejects a fixed ttl with a non-u64 seconds value: %s",
        (badTtl) => {
          expect(
            parseApi(
              apiWithAuth(`{ "type": "chained", ${source}, "ttl": ${badTtl}, ${inject} }`),
            ).ok,
          ).toBe(false);
        },
      );

      it("rejects retryOn that is not an array of numbers", () => {
        expect(
          parseApi(
            apiWithAuth(
              `{ "type": "chained", ${source}, ${inject}, "retryOn": "401" }`,
            ),
          ).ok,
        ).toBe(false);
        expect(
          parseApi(
            apiWithAuth(
              `{ "type": "chained", ${source}, ${inject}, "retryOn": [401, "403"] }`,
            ),
          ).ok,
        ).toBe(false);
      });

      // MEDIUM review finding: `retryOn` is `Vec<u16>` in `model.rs`, but
      // any number passed the old check — `-1`, `99999` and `401.5` all
      // would have rendered fine here and failed only at Save time.
      it.each(["[-1]", "[99999]", "[401.5]"])(
        "rejects retryOn with a value outside u16's range/integer-ness: %s",
        (badRetryOn) => {
          expect(
            parseApi(
              apiWithAuth(
                `{ "type": "chained", ${source}, ${inject}, "retryOn": ${badRetryOn} }`,
              ),
            ).ok,
          ).toBe(false);
        },
      );

      it("accepts retryOn at u16's exact boundaries (0 and 65535)", () => {
        expect(
          parseApi(
            apiWithAuth(
              `{ "type": "chained", ${source}, ${inject}, "retryOn": [0, 65535] }`,
            ),
          ).ok,
        ).toBe(true);
      });

      it("accepts retryOn simply absent (retry_on HAS a serde default)", () => {
        expect(
          parseApi(apiWithAuth(`{ "type": "chained", ${source}, ${inject} }`)).ok,
        ).toBe(true);
      });
    });

    it("rejects API-level auth with the same rules", () => {
      const text = `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "auth": { "type": "bearer" },
  "endpoints": []
}`;
      expect(parseApi(text).ok).toBe(false);
    });
  });

  it("preserves a chained auth block through a round trip", () => {
    const text = serializeApi({
      ...emptyApi("a", "A"),
      baseUrl: "https://x",
      endpoints: [
        { ...emptyEndpoint("token", "T"), method: "POST", path: "/token" },
        {
          ...emptyEndpoint("biz", "B"),
          path: "/b",
          auth: {
            type: "chained", source: { endpoint: "token" },
            inject: { into: "header", name: "Authorization",
                      template: "Bearer {{value}}" }, retryOn: [401],
          },
        },
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

// Pins the field set model.rs always writes (every `#[serde(default)]`
// field WITHOUT `skip_serializing_if`) so a fresh object already matches
// every sibling in the file, not just after a save round-trips it through
// Rust. Fails loudly if a default is ever dropped from either skeleton.
describe("Rust-always-present fields", () => {
  it("emptyApi carries every field Api always serializes", () => {
    const api = emptyApi("new-api", "New API");
    expect(Object.keys(api)).toEqual([
      "schemaVersion", "id", "name", "baseUrl",
      "variables", "environments", "auth", "endpoints",
    ]);
    expect(api.variables).toEqual({});
    expect(api.environments).toEqual([]);
    expect(api.auth).toEqual({ type: "none" });
  });

  it("emptyEndpoint carries every field Endpoint always serializes", () => {
    const ep = emptyEndpoint("ping", "Ping");
    expect(Object.keys(ep)).toEqual([
      "id", "name", "method", "path", "headers", "query", "variables", "auth",
    ]);
    expect(ep.headers).toEqual({});
    expect(ep.query).toEqual({});
    expect(ep.variables).toEqual({});
    expect(ep.auth).toEqual({ type: "inherit" });
  });
});

describe("slugify", () => {
  it("lowercases and hyphenates a plain name", () => {
    expect(slugify("Payments Gateway")).toBe("payments-gateway");
  });

  it("collapses runs of non-alphanumeric characters into one hyphen", () => {
    expect(slugify("  Foo___Bar!! Baz  ")).toBe("foo-bar-baz");
  });

  it("trims leading and trailing hyphens", () => {
    expect(slugify("-leading and trailing-")).toBe("leading-and-trailing");
  });

  it("returns an empty string for a name with no alphanumeric characters", () => {
    expect(slugify("***")).toBe("");
    expect(slugify("   ")).toBe("");
  });

  it("keeps digits", () => {
    expect(slugify("API v2")).toBe("api-v2");
  });
});
