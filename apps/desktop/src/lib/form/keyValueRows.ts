// Pure helpers behind `KeyValueRows.svelte`, kept dependency-free so they
// can be unit-tested without a DOM (vitest's default node environment).
//
// A `Record<string, string>` (the model's shape for headers/query/etc.) has
// no duplicate keys and no notion of row order or "a row being renamed" —
// only `KeyValueRows` needs those things, to let a user type into a key
// field without rows jumping around or a half-typed duplicate silently
// eating another row. So the component keeps an ordered `Row[]` as its own
// state and uses these functions to convert to/from the record it actually
// reads from and writes to the document.

export interface Row {
  key: string;
  value: string;
}

/** Record -> ordered rows, in the record's own (insertion) key order. */
export function recordToRows(record: Record<string, string>): Row[] {
  return Object.entries(record).map(([key, value]) => ({ key, value }));
}

export type RebuildResult =
  | { ok: true; record: Record<string, string> }
  | { ok: false; duplicateKeys: string[] };

/**
 * Rebuilds a record from ordered rows, preserving row order as key
 * insertion order. Rows with an empty key are placeholders (the permanent
 * blank row, or a row whose key the user hasn't typed yet) and never
 * contribute an entry.
 *
 * If two or more rows share the same non-empty key, this refuses to build a
 * record at all — returning the offending keys instead — rather than
 * silently letting the later row win and dropping the earlier one's value.
 * The caller must leave the previous record untouched and show the
 * collision, never rebuild with the duplicate "collapsed".
 */
export function rowsToRecord(rows: Row[]): RebuildResult {
  const counts = new Map<string, number>();
  for (const row of rows) {
    if (row.key === "") continue;
    counts.set(row.key, (counts.get(row.key) ?? 0) + 1);
  }
  const duplicateKeys = [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([key]) => key);
  if (duplicateKeys.length > 0) {
    return { ok: false, duplicateKeys };
  }
  const record: Record<string, string> = {};
  for (const row of rows) {
    if (row.key === "") continue;
    record[row.key] = row.value;
  }
  return { ok: true, record };
}

/** Structural equality for two `Record<string,string>`, order-sensitive
 * (mirrors how `JSON.stringify` would render them, since that's what
 * ultimately lands in the buffer). */
export function recordsEqual(
  a: Record<string, string>,
  b: Record<string, string>,
): boolean {
  const aKeys = Object.keys(a);
  const bKeys = Object.keys(b);
  if (aKeys.length !== bKeys.length) return false;
  for (let i = 0; i < aKeys.length; i++) {
    if (aKeys[i] !== bKeys[i] || a[aKeys[i]] !== b[bKeys[i]]) return false;
  }
  return true;
}

/**
 * Ensures the row list ends with exactly one blank editable row (the
 * permanently-present "add a row" affordance). Never mutates the input.
 */
export function withTrailingBlank(rows: Row[]): Row[] {
  const last = rows[rows.length - 1];
  if (last && last.key === "" && last.value === "") {
    return rows;
  }
  return [...rows, { key: "", value: "" }];
}
