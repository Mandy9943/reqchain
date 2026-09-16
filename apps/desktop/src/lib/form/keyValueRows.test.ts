import { describe, expect, it } from "vitest";
import {
  recordToRows,
  recordsEqual,
  rowsToRecord,
  withTrailingBlank,
  type Row,
} from "./keyValueRows";

describe("recordToRows", () => {
  it("preserves the record's own key order", () => {
    const rows = recordToRows({ b: "2", a: "1" });
    expect(rows).toEqual([
      { key: "b", value: "2" },
      { key: "a", value: "1" },
    ]);
  });

  it("returns an empty array for an empty record", () => {
    expect(recordToRows({})).toEqual([]);
  });
});

describe("rowsToRecord", () => {
  it("builds a record in row order, dropping blank-key placeholder rows", () => {
    const rows: Row[] = [
      { key: "Accept", value: "json" },
      { key: "", value: "" }, // the permanent trailing blank row
      { key: "X-Trace", value: "1" },
    ];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({ Accept: "json", "X-Trace": "1" });
    expect(Object.keys(result.record)).toEqual(["Accept", "X-Trace"]);
  });

  it("keeps a row with an empty value (only an empty KEY is a placeholder)", () => {
    const rows: Row[] = [{ key: "X-Empty", value: "" }];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({ "X-Empty": "" });
  });

  it("refuses to build a record when two rows share a non-empty key", () => {
    const rows: Row[] = [
      { key: "Accept", value: "json" },
      { key: "Accept", value: "xml" },
    ];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.duplicateKeys).toEqual(["Accept"]);
  });

  it("does not treat two blank-key rows as a duplicate", () => {
    const rows: Row[] = [
      { key: "", value: "" },
      { key: "", value: "" },
    ];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.record).toEqual({});
  });

  it("renaming a key in place preserves order (positional edit, not delete+append)", () => {
    // Simulates what the component does on every keystroke in a key field:
    // it edits the row at its existing index, it never removes and re-adds.
    const before: Row[] = [
      { key: "a", value: "1" },
      { key: "b", value: "2" },
      { key: "c", value: "3" },
    ];
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
      { key: "a", value: "1" },
      { key: "b", value: "2" },
      { key: "a", value: "3" },
      { key: "b", value: "4" },
    ];
    const result = rowsToRecord(rows);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(new Set(result.duplicateKeys)).toEqual(new Set(["a", "b"]));
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
});

describe("withTrailingBlank", () => {
  it("appends a blank row when the last row is not blank", () => {
    const rows: Row[] = [{ key: "a", value: "1" }];
    expect(withTrailingBlank(rows)).toEqual([
      { key: "a", value: "1" },
      { key: "", value: "" },
    ]);
  });

  it("appends a blank row to an empty list", () => {
    expect(withTrailingBlank([])).toEqual([{ key: "", value: "" }]);
  });

  it("does not append a second blank row when one is already trailing", () => {
    const rows: Row[] = [
      { key: "a", value: "1" },
      { key: "", value: "" },
    ];
    expect(withTrailingBlank(rows)).toBe(rows);
  });

  it("treats a row with an empty key but a non-empty value as not blank", () => {
    const rows: Row[] = [{ key: "", value: "still typing the value" }];
    expect(withTrailingBlank(rows)).toEqual([
      { key: "", value: "still typing the value" },
      { key: "", value: "" },
    ]);
  });
});
