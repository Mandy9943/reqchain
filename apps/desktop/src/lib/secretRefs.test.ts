import { describe, expect, it } from "vitest";
import { missingSecretRefs, secretRefsIn } from "./secretRefs";

describe("secretRefsIn", () => {
  it("finds a reference nested anywhere in the raw text", () => {
    const text = `{"headers":{"X-Api-Key":"{{secret:GW_KEY}}"}}`;
    expect(secretRefsIn(text)).toEqual(["GW_KEY"]);
  });

  it("dedupes repeated references, keeping first-seen order", () => {
    const text = "{{secret:A}} ... {{secret:B}} ... {{secret:A}}";
    expect(secretRefsIn(text)).toEqual(["A", "B"]);
  });

  it("trims whitespace inside the braces, like the Rust interpolator", () => {
    const text = "{{ secret:  GW_KEY  }}";
    expect(secretRefsIn(text)).toEqual(["GW_KEY"]);
  });

  it("ignores an ordinary (non-secret) variable reference", () => {
    const text = "{{baseUrl}} {{secret:PASS}}";
    expect(secretRefsIn(text)).toEqual(["PASS"]);
  });

  it("returns nothing for text with no secret reference", () => {
    expect(secretRefsIn(`{"id":"demo"}`)).toEqual([]);
  });
});

describe("missingSecretRefs", () => {
  it("reports a referenced secret that has no stored entry", () => {
    const texts = [`{"h":"{{secret:GW_PASS}}"}`];
    expect(missingSecretRefs(texts, [])).toEqual(["GW_PASS"]);
  });

  it("omits a reference that is already defined", () => {
    const texts = [`{"h":"{{secret:GW_PASS}}"}`];
    expect(missingSecretRefs(texts, ["GW_PASS"])).toEqual([]);
  });

  it("merges references across multiple files, sorted and deduplicated", () => {
    const texts = [
      `{"h":"{{secret:ZEBRA}}"}`,
      `{"h":"{{secret:ALPHA}}"}`,
      `{"h":"{{secret:ZEBRA}}"}`,
    ];
    expect(missingSecretRefs(texts, [])).toEqual(["ALPHA", "ZEBRA"]);
  });

  it("returns an empty array when nothing is missing", () => {
    expect(missingSecretRefs([], ["A"])).toEqual([]);
  });
});
