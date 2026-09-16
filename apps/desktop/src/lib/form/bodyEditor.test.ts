import { describe, expect, it } from "vitest";
import {
  bodyKey,
  bodyStringField,
  bodyTypeOf,
  defaultBodyForType,
  isBodyEmpty,
  jsonContentText,
  parseJsonContent,
  type BodyType,
} from "./bodyEditor";
import type { Body } from "../model";

describe("bodyTypeOf", () => {
  it("is \"none\" for an absent body", () => {
    expect(bodyTypeOf(undefined)).toBe("none");
  });

  it.each([
    "json",
    "form",
    "multipart",
    "text",
    "xml",
    "binary",
  ] as const)("recognizes the %s variant", (type) => {
    const body = defaultBodyForType(type)!;
    expect(bodyTypeOf(body)).toBe(type);
  });

  it("falls back to \"none\" for a runtime value with an unknown type tag", () => {
    const corrupted = { type: "yaml", content: "x" } as unknown as Body;
    expect(bodyTypeOf(corrupted)).toBe("none");
  });

  it("falls back to \"none\" for a runtime value with a non-string type tag", () => {
    const corrupted = { type: 42 } as unknown as Body;
    expect(bodyTypeOf(corrupted)).toBe("none");
  });
});

describe("bodyKey", () => {
  it("is stable for structurally-equal bodies from different parses", () => {
    const a: Body = { type: "form", fields: { x: "1" } };
    const b: Body = JSON.parse(JSON.stringify(a));
    expect(bodyKey(a)).toBe(bodyKey(b));
  });

  it("differs for different bodies", () => {
    const a: Body = { type: "form", fields: { x: "1" } };
    const b: Body = { type: "form", fields: { x: "2" } };
    expect(bodyKey(a)).not.toBe(bodyKey(b));
  });

  it("treats undefined consistently", () => {
    expect(bodyKey(undefined)).toBe(bodyKey(undefined));
  });
});

describe("isBodyEmpty", () => {
  it("an absent body is empty", () => {
    expect(isBodyEmpty(undefined)).toBe(true);
  });

  describe("json", () => {
    it("empty: null content", () => {
      expect(isBodyEmpty({ type: "json", content: null })).toBe(true);
    });
    it("empty: blank string content", () => {
      expect(isBodyEmpty({ type: "json", content: "   " })).toBe(true);
    });
    it("empty: empty array content", () => {
      expect(isBodyEmpty({ type: "json", content: [] })).toBe(true);
    });
    it("empty: empty object content", () => {
      expect(isBodyEmpty({ type: "json", content: {} })).toBe(true);
    });
    it("non-empty: non-blank string", () => {
      expect(isBodyEmpty({ type: "json", content: "hi" })).toBe(false);
    });
    it("non-empty: object with a key", () => {
      expect(isBodyEmpty({ type: "json", content: { a: 1 } })).toBe(false);
    });
    it("non-empty: a bare number", () => {
      expect(isBodyEmpty({ type: "json", content: 0 })).toBe(false);
    });
  });

  it("form: empty with no fields", () => {
    expect(isBodyEmpty({ type: "form", fields: {} })).toBe(true);
  });

  it("form: non-empty with a field", () => {
    expect(isBodyEmpty({ type: "form", fields: { a: "1" } })).toBe(false);
  });

  it("form: defends against a runtime non-object fields value", () => {
    const corrupted = { type: "form", fields: null } as unknown as Body;
    expect(isBodyEmpty(corrupted)).toBe(true);
  });

  it("multipart: empty with no fields and no files", () => {
    expect(isBodyEmpty({ type: "multipart", fields: {}, files: {} })).toBe(
      true,
    );
  });

  it("multipart: non-empty via fields alone", () => {
    expect(
      isBodyEmpty({ type: "multipart", fields: { a: "1" }, files: {} }),
    ).toBe(false);
  });

  it("multipart: non-empty via files alone", () => {
    expect(
      isBodyEmpty({ type: "multipart", fields: {}, files: { f: "/tmp/x" } }),
    ).toBe(false);
  });

  it("text/xml: empty with blank content", () => {
    expect(isBodyEmpty({ type: "text", content: "  " })).toBe(true);
    expect(isBodyEmpty({ type: "xml", content: "" })).toBe(true);
  });

  it("text/xml: non-empty content", () => {
    expect(isBodyEmpty({ type: "text", content: "hello" })).toBe(false);
  });

  it("text: defends against a runtime non-string content value", () => {
    const corrupted = { type: "text", content: 5 } as unknown as Body;
    expect(isBodyEmpty(corrupted)).toBe(true);
  });

  it("binary: empty with a blank path", () => {
    expect(isBodyEmpty({ type: "binary", path: "" })).toBe(true);
  });

  it("binary: non-empty with a path", () => {
    expect(isBodyEmpty({ type: "binary", path: "/tmp/x" })).toBe(false);
  });
});

describe("defaultBodyForType", () => {
  it("none produces an absent body", () => {
    expect(defaultBodyForType("none")).toBeUndefined();
  });

  it.each([
    ["json", { type: "json", content: "" }],
    ["form", { type: "form", fields: {} }],
    ["multipart", { type: "multipart", fields: {}, files: {} }],
    ["text", { type: "text", content: "" }],
    ["xml", { type: "xml", content: "" }],
    ["binary", { type: "binary", path: "" }],
  ] as [BodyType, Body][])("%s", (type, expected) => {
    expect(defaultBodyForType(type)).toEqual(expected);
  });

  it.each(["json", "form", "multipart", "text", "xml", "binary"] as const)(
    "every fresh %s body is considered empty",
    (type) => {
      expect(isBodyEmpty(defaultBodyForType(type))).toBe(true);
    },
  );

  it.each(["json", "form", "multipart", "text", "xml", "binary"] as const)(
    "every fresh %s body has keys in model.rs's declared order",
    (type) => {
      const body = defaultBodyForType(type)!;
      const keys = Object.keys(body);
      expect(keys[0]).toBe("type");
    },
  );
});

describe("jsonContentText / parseJsonContent", () => {
  it("round-trips an object", () => {
    const text = jsonContentText({ a: 1, b: [true, null] });
    expect(text).toBe(JSON.stringify({ a: 1, b: [true, null] }, null, 2));
    const parsed = parseJsonContent(text);
    expect(parsed).toEqual({ ok: true, content: { a: 1, b: [true, null] } });
  });

  it("undefined content renders as an empty string", () => {
    expect(jsonContentText(undefined)).toBe("");
  });

  it("null content renders as the literal null", () => {
    expect(jsonContentText(null)).toBe("null");
  });

  it("reports a parse error without throwing", () => {
    const result = parseJsonContent("{ not json");
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error.length).toBeGreaterThan(0);
  });

  it("a blank editor is a parse error, not empty-object content", () => {
    expect(parseJsonContent("").ok).toBe(false);
  });

  it("re-stringified content is not always identical to the text that produced it", () => {
    // This is the root cause of the "cursor jumps to the start on every
    // keystroke" bug: `jsonContentText(parseJsonContent(text).content)` is a
    // canonical re-render, not the original input. A component that feeds
    // an editor widget `jsonContentText(content)` as its committed-value
    // source of truth (instead of tracking the exact text it last accepted)
    // will see this text differ from what the widget still holds after
    // every keystroke that happens to stay valid, and reset the widget.
    const typed = '{\n  "a": 1,\n\n\n  "b": 2\n}'; // valid JSON, unusual whitespace
    const parsed = parseJsonContent(typed);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    const reformatted = jsonContentText(parsed.content);
    expect(reformatted).not.toBe(typed);
  });

  it("round-trips back to the same text once already canonically formatted (no infinite drift)", () => {
    const canonical = jsonContentText({ a: 1, b: 2 });
    const parsed = parseJsonContent(canonical);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(jsonContentText(parsed.content)).toBe(canonical);
  });
});

describe("bodyStringField", () => {
  it("passes through a real string", () => {
    expect(bodyStringField("hello")).toBe("hello");
  });

  it.each([undefined, null, 5, {}, []])(
    "coerces a non-string runtime value (%j) to an empty string",
    (value) => {
      expect(bodyStringField(value)).toBe("");
    },
  );
});
