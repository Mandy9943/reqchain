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
//
// Runtime input is not bound by the TS types: `parseApi` only shallow-
// validates, so a hand-edited JSON tab can hand this module a `headers`
// value that isn't an object at all (missing, `null`, an array, ...) even
// though `Endpoint.headers` is typed as `Record<string,string>`. Every
// function here that takes a "record" defends against that at runtime
// instead of trusting the compile-time type.
//
// Row order is a BUFFER/DISPLAY guarantee only, not an on-disk one.
// `headers`, `query`, `variables` and (see `bodyEditor.ts`) `form`/
// `multipart` body fields are all `BTreeMap<String, String>` in
// `model.rs` — `Api::to_json_string` (the Rust serializer that actually
// writes the file) therefore emits them in ALPHABETICAL key order on every
// save, regardless of what order `rowsToRecord` built them in here. This
// module's careful "renaming a key preserves its row position" behavior is
// real and correct for the in-memory buffer (what the JSON tab shows
// before a save, and what `git diff` sees between two saves if row order
// happens to already be alphabetical), but a save can still reorder keys
// out from under a row's displayed position — this is expected, not a bug
// in either this module or the Rust serializer, and it is deliberately
// NOT something either side works around (alphabetical order is
// deterministic and diff-friendly, which is arguably the better property
// to have on disk).

export interface Row {
  key: string;
  value: string;
  /**
   * The key this row was last known to occupy in a successfully-committed
   * record — `null` for a row that has never held committed content (the
   * permanent trailing "add a row" blank, or a brand-new row the user
   * hasn't finished typing a first valid key into yet).
   *
   * This is what lets clearing a key field to rename it NOT delete the
   * entry: while `key` is transiently empty, `resolvedKey` falls back to
   * `hadKey`, so the row keeps representing its last-committed identity
   * until either a new unique key is typed (renaming it) or the row is
   * removed outright via its own remove button. Only a row that has never
   * had committed content (`hadKey === null`) actually disappears from the
   * record when its key is empty — which is exactly the "permanently
   * blank row is not an entry" behavior, not a footgun.
   */
  hadKey: string | null;
}

/** Runtime-safe coercion: anything that isn't a genuine plain object
 * (missing, `null`, an array, a primitive) is treated as empty, since
 * that's the only safe reading of a value the JSON tab could have set to
 * anything at all. Exported for other form code (e.g. `bodyEditor.ts`)
 * that needs the exact same defensive coercion for a `Record<string,string>`
 * field it didn't get from `recordToRows`/`recordsEqual` directly — one
 * canonical guard, not a second copy of this logic. */
export function asRecord(value: Record<string, string>): Record<string, string> {
  if (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value)
  ) {
    return value;
  }
  return {};
}

/** Record -> ordered rows, in the record's own (insertion) key order. Every
 * loaded row's `hadKey` is its own key, since it already represents
 * committed content. */
export function recordToRows(record: Record<string, string>): Row[] {
  return Object.entries(asRecord(record)).map(([key, value]) => ({
    key,
    value,
    hadKey: key,
  }));
}

/** The key this row actually contributes to the record: its own (trimmed)
 * key if non-empty, otherwise its last-committed identity, otherwise
 * `null` (no entry — a true placeholder). Leading/trailing whitespace is
 * trimmed before this comparison, so `" Accept"` and `"Accept"` are the
 * same key, not a typo waiting to create two headers. */
export function resolvedKey(row: Row): string | null {
  const trimmed = row.key.trim();
  return trimmed !== "" ? trimmed : row.hadKey;
}

/**
 * A key JS itself would reorder ahead of every string key, regardless of
 * where it appears in the object literal (the "array index" property-key
 * rule: a canonical non-negative-integer string, `0` to `2**32 - 2`).
 * `JSON.stringify`/insertion order cannot be defended against this — the
 * ECMAScript spec mandates the reordering — so such a key is refused
 * outright rather than silently reshuffling every other header/param the
 * moment one is added.
 */
export function isIntegerLikeKey(key: string): boolean {
  if (!/^(0|[1-9]\d*)$/.test(key)) return false;
  return Number(key) <= 4294967294; // 2**32 - 2, the spec's max array index
}

export type RebuildResult =
  | { ok: true; record: Record<string, string> }
  | { ok: false; kind: "duplicate"; keys: string[] }
  | { ok: false; kind: "integer-like"; keys: string[] };

/**
 * Rebuilds a record from ordered rows, preserving row order as key
 * insertion order. A row whose `resolvedKey` is `null` (see above) is a
 * placeholder and never contributes an entry.
 *
 * Refuses — returning the offending keys instead of building anything —
 * when:
 *  - two or more rows resolve to the same non-empty key ("duplicate"): the
 *    caller must leave the previous record untouched and show the
 *    collision, never silently let the later row win.
 *  - any row resolves to an integer-like key ("integer-like"): JS would
 *    reorder the record out from under the user on the very next edit (see
 *    `isIntegerLikeKey`), which would look like data corruption.
 */
export function rowsToRecord(rows: Row[]): RebuildResult {
  const resolved = rows.map(resolvedKey);

  const counts = new Map<string, number>();
  for (const key of resolved) {
    if (key === null) continue;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  const duplicateKeys = [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([key]) => key);
  if (duplicateKeys.length > 0) {
    return { ok: false, kind: "duplicate", keys: duplicateKeys };
  }

  const integerLikeKeys = [
    ...new Set(
      resolved.filter((key): key is string => key !== null && isIntegerLikeKey(key)),
    ),
  ];
  if (integerLikeKeys.length > 0) {
    return { ok: false, kind: "integer-like", keys: integerLikeKeys };
  }

  const record: Record<string, string> = {};
  rows.forEach((row, i) => {
    const key = resolved[i];
    if (key === null) return;
    record[key] = row.value;
  });
  return { ok: true, record };
}

/** Structural equality for two `Record<string,string>`, order-sensitive
 * (mirrors how `JSON.stringify` would render them, since that's what
 * ultimately lands in the buffer). Defends against non-object runtime
 * input the same way `recordToRows` does. */
export function recordsEqual(
  a: Record<string, string>,
  b: Record<string, string>,
): boolean {
  const aSafe = asRecord(a);
  const bSafe = asRecord(b);
  const aKeys = Object.keys(aSafe);
  const bKeys = Object.keys(bSafe);
  if (aKeys.length !== bKeys.length) return false;
  for (let i = 0; i < aKeys.length; i++) {
    if (aKeys[i] !== bKeys[i] || aSafe[aKeys[i]] !== bSafe[bKeys[i]]) {
      return false;
    }
  }
  return true;
}

/**
 * Ensures the row list ends with exactly one blank editable row (the
 * permanently-present "add a row" affordance) — identified by
 * `resolvedKey(...) === null` (never had committed content), not merely by
 * `key === ""`, so a row that's mid-rename of an existing entry (empty
 * `key`, non-null `hadKey`) is never mistaken for the add-a-row blank.
 * Never mutates the input.
 */
export function withTrailingBlank(rows: Row[]): Row[] {
  const last = rows[rows.length - 1];
  if (last && resolvedKey(last) === null && last.value === "") {
    return rows;
  }
  return [...rows, { key: "", value: "", hadKey: null }];
}
