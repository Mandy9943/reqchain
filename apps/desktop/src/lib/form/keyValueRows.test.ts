import { describe, expect, it } from "vitest";
import {
  isIntegerLikeKey,
  recordToRows,
  recordsEqual,
  resolvedKey,
  rowsToRecord,
  withTrailingBlank,
  type Row,
} from "./keyValueRows";

/** A row loaded from a real record — `hadKey` starts equal to `key`,
 * exactly what `recordToRows` produces. */
function loaded(key: string, value: string): Row {
  return { key, value, hadKey: key };
}

/** A row that has never held committed content — the permanent trailing
 * blank, or a brand-new row nothing has been typed into yet. */
function blank(key = "", value = ""): Row {
  return { key, value, hadKey: null };
}

describe("recordToRows", () => {
  it("preserves the record's own key order and sets hadKey to each loaded key", () => {
    const rows = recordToRows({ b: "2", a: "1" });
    expect(rows).toEqual([
      { key: "b", value: "2", hadKey: "b" },
      { key: "a", value: "1", hadKey: "a" },
    ]);
  });

  it("returns an empty array for an empty record", () => {
    expect(recordToRows({})).toEqual([]);
  });

  it("treats non-object runtime input (missing/null/array) as empty instead of throwing", () => {
    // `parseApi` only shallow-validates; a JSON-tab edit can hand this a
    // value that isn't an object at all despite the TS type.
    expect(recordToRows(undefined as unknown as Record<string, string>)).toEqual([]);
    expect(recordToRows(null as unknown as Record<string, string>)).toEqual([]);
    expect(recordToRows([] as unknown as Record<string, string>)).toEqual([]);
  });
});

describe("resolvedKey", () => {
  it("is the row's own key when non-empty", () => {
    expect(resolvedKey(loaded("Accept", "json"))).toBe("Accept");
  });

  it("falls back to hadKey when the key is cleared (mid-rename)", () => {
    expect(resolvedKey({ key: "", value: "json", hadKey: "Accept" })).toBe("Accept");
  });

  it("is null for a row that never had content and has no key", () => {
    expect(resolvedKey(blank())).toBeNull();
  });

  it("trims whitespace before resolving", () => {
    expect(resolvedKey({ key: "  Accept  ", value: "json", hadKey: null })).toBe("Accept");
  });
});

describe("rowsToRecord", () => {
  it("builds a record in row order, dropping the true blank placeholder row", () => {
    const rows: Row[] = [
      loaded("Accept", "json"),
      blank(), // the permanent trailing blank row
      loaded("X-Trace", "1"),
    ];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({ Accept: "json", "X-Trace": "1" });
    expect(Object.keys(result.record)).toEqual(["Accept", "X-Trace"]);
  });

  it("keeps a row with an empty value (only an empty resolved KEY is a placeholder)", () => {
    const rows: Row[] = [loaded("X-Empty", "")];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({ "X-Empty": "" });
  });

  it("refuses to build a record when two rows resolve to the same non-empty key", () => {
    const rows: Row[] = [loaded("Accept", "json"), loaded("Accept", "xml")];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.kind).toBe("duplicate");
    expect(result.keys).toEqual(["Accept"]);
  });

  it("does not treat two blank placeholder rows as a duplicate", () => {
    const rows: Row[] = [blank(), blank()];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({});
  });

  it("renaming a key in place preserves order (positional edit, not delete+append)", () => {
    // Simulates what the component does on every keystroke in a key field:
    // it edits the row at its existing index, it never removes and re-adds.
    const before: Row[] = [loaded("a", "1"), loaded("b", "2"), loaded("c", "3")];
    // Rename the middle row's key ("b" -> "renamed").
    const after = before.map((r, i) => (i === 1 ? { ...r, key: "renamed" } : r));
    const result = rowsToRecord(after);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(Object.keys(result.record)).toEqual(["a", "renamed", "c"]);
    expect(result.record).toEqual({ a: "1", renamed: "2", c: "3" });
  });

  it("reports every key that collides, not just the first pair", () => {
    const rows: Row[] = [
      loaded("a", "1"),
      loaded("b", "2"),
      loaded("a", "3"),
      loaded("b", "4"),
    ];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.kind).toBe("duplicate");
    expect(new Set(result.keys)).toEqual(new Set(["a", "b"]));
  });

  // HIGH review finding: clearing an existing key to rename it must NOT
  // delete the entry, even if the rename is interrupted (tab switch, blur,
  // hitting Save) while the key field is empty.
  it("keeps an existing entry when its key is cleared mid-rename, instead of deleting it", () => {
    const rows: Row[] = [loaded("Accept", "json")];
    // User selects the key field and deletes it, about to retype — this is
    // exactly what a keystroke-by-keystroke `setKey(0, "")` produces.
    const midRename = [{ ...rows[0], key: "" }];
    const result = rowsToRecord(midRename);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    // The entry survives under its ORIGINAL key with its unchanged value —
    // nothing was deleted just because the field is momentarily empty.
    expect(result.record).toEqual({ Accept: "json" });
  });

  it("commits a rename once a new unique key is typed, replacing the old key", () => {
    const rows: Row[] = [loaded("Accept", "json")];
    const cleared = [{ ...rows[0], key: "" }];
    const renamed = [{ ...cleared[0], key: "Content-Type" }];
    const result = rowsToRecord(renamed);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({ "Content-Type": "json" });
    expect(result.record.Accept).toBeUndefined();
  });

  it("a truly untouched row (hadKey still null, key still empty) never resolves", () => {
    // The permanent blank row itself — nothing typed at all.
    expect(rowsToRecord([blank()])).toEqual({ ok: true, record: {} });
  });

  it("a freshly-typed key becomes sticky once committed, matching the component's hadKey-advance-on-commit behavior", () => {
    // Simulates two steps through the real component: (1) the user types
    // "X" into the permanent blank row and it commits successfully — see
    // KeyValueRows.svelte's `commit()`, which advances `hadKey` to whatever
    // was just resolved — then (2) the user clears the key field again
    // before typing a replacement. `rowsToRecord` alone doesn't advance
    // `hadKey` (only the component's `commit()` does); this row is
    // constructed as it would look right after step 1.
    const committed: Row = { key: "X", value: "", hadKey: "X" };
    const clearedAgain = { ...committed, key: "" };
    const result = rowsToRecord([clearedAgain]);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    // The entry is NOT lost — it survives under the key it was last
    // successfully committed under, same as any other mid-rename row.
    expect(result.record).toEqual({ X: "" });
  });

  it("refuses an integer-like key, since JSON key ordering would reorder it", () => {
    const rows: Row[] = [loaded("Accept", "json"), loaded("2", "value")];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.kind).toBe("integer-like");
    expect(result.keys).toEqual(["2"]);
  });

  it("trims whitespace from keys before committing and comparing", () => {
    const rows: Row[] = [loaded(" Accept ", "json")];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({ Accept: "json" });
  });

  it("treats whitespace-padded and bare versions of a key as the same key (duplicate)", () => {
    const rows: Row[] = [loaded("Accept", "json"), loaded(" Accept", "xml")];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.kind).toBe("duplicate");
    expect(result.keys).toEqual(["Accept"]);
  });
});

describe("isIntegerLikeKey", () => {
  it("is true for canonical non-negative integer strings", () => {
    expect(isIntegerLikeKey("0")).toBe(true);
    expect(isIntegerLikeKey("2")).toBe(true);
    expect(isIntegerLikeKey("4294967294")).toBe(true);
  });

  it("is false for ordinary header/param names", () => {
    expect(isIntegerLikeKey("Accept")).toBe(false);
    expect(isIntegerLikeKey("X-Trace-2")).toBe(false);
  });

  it("is false for non-canonical numeric-looking strings (JS treats these as ordinary string keys)", () => {
    expect(isIntegerLikeKey("01")).toBe(false); // leading zero
    expect(isIntegerLikeKey("-1")).toBe(false); // negative
    expect(isIntegerLikeKey("1.5")).toBe(false); // not an integer
    expect(isIntegerLikeKey("")).toBe(false);
  });

  it("is false just past the maximum valid array index", () => {
    expect(isIntegerLikeKey("4294967295")).toBe(false);
  });
});

describe("recordsEqual", () => {
  it("is true for two structurally identical records in the same order", () => {
    expect(recordsEqual({ a: "1", b: "2" }, { a: "1", b: "2" })).toBe(true);
  });

  it("is false when a value differs", () => {
    expect(recordsEqual({ a: "1" }, { a: "2" })).toBe(false);
  });

  it("is false when key order differs (mirrors JSON.stringify's own sensitivity)", () => {
    expect(recordsEqual({ a: "1", b: "2" }, { b: "2", a: "1" })).toBe(false);
  });

  it("is false when sizes differ", () => {
    expect(recordsEqual({ a: "1" }, { a: "1", b: "2" })).toBe(false);
  });

  it("treats non-object runtime input as an empty record instead of throwing", () => {
    expect(recordsEqual(undefined as unknown as Record<string, string>, {})).toBe(true);
    expect(recordsEqual(null as unknown as Record<string, string>, { a: "1" })).toBe(
      false,
    );
  });
});

describe("withTrailingBlank", () => {
  it("appends a blank row when the last row is not blank", () => {
    const rows: Row[] = [loaded("a", "1")];
    expect(withTrailingBlank(rows)).toEqual([loaded("a", "1"), blank()]);
  });

  it("appends a blank row to an empty list", () => {
    expect(withTrailingBlank([])).toEqual([blank()]);
  });

  it("does not append a second blank row when one is already trailing", () => {
    const rows: Row[] = [loaded("a", "1"), blank()];
    expect(withTrailingBlank(rows)).toBe(rows);
  });

  it("treats a row with an empty key but a non-empty value as not blank", () => {
    const rows: Row[] = [blank("", "still typing the value")];
    expect(withTrailingBlank(rows)).toEqual([
      blank("", "still typing the value"),
      blank(),
    ]);
  });

  // HIGH review finding, the core regression test: a row mid-rename of an
  // EXISTING entry (empty key, but hadKey set, and empty value too — the
  // user cleared both while retyping) must never be mistaken for the
  // permanent "add a row" blank, or a second real blank row would pile up
  // beside it and the row's identity tracking would be indistinguishable
  // from a fresh, never-had-content row.
  it("does not treat a cleared-but-previously-loaded row as the trailing blank", () => {
    const midRename: Row = { key: "", value: "", hadKey: "Accept" };
    const rows: Row[] = [midRename];
    expect(withTrailingBlank(rows)).toEqual([midRename, blank()]);
  });
});
