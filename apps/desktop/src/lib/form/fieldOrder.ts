// The one place that knows how to insert a key into an `Api` object AT ITS
// DECLARED POSITION, not merely "somewhere" — see model.ts's top-of-file
// note and constraints.md's "one serializer" rule: `serializeApi` is a
// plain `JSON.stringify`, so JS object key insertion order IS the on-disk
// field order, and `model.rs`'s declared struct field order is the only
// correct one. A form field whose key may be genuinely ABSENT from the
// document being edited (any `#[serde(default)]` field with no
// `skip_serializing_if` — legally missing from a hand-written or
// older/agent-written file, even though `emptyApi` itself always writes
// it) cannot just do `api.field = value`: on an object that never had that
// key, a plain assignment inserts it at the END of the object's own
// current key order, not at its declared position — silently reordering
// every field written after it in the file the next time the buffer is
// serialized. `ApiForm.svelte`'s `variables`, `environments`, `auth` and
// `history` mutators all route through `setApiField` for exactly this
// reason (a review finding on the first version of this form caught it for
// `history` alone and asked for one shared fix covering all four, rather
// than four hand-patched call sites that could individually regress).
//
// Updating a key that's already present is a plain, order-preserving
// assignment either way — the "insert at declared position" logic below
// only does anything different when the key is genuinely new to this
// object.

import type { Api } from "../model";

/** `Api`'s field order exactly as declared in `model.rs`'s `struct Api`. */
export const API_FIELD_ORDER: readonly (keyof Api)[] = [
  "schemaVersion",
  "id",
  "name",
  "baseUrl",
  "variables",
  "environments",
  "auth",
  "history",
  "endpoints",
];

/**
 * Sets `api[key] = value`, inserting `key` at its `API_FIELD_ORDER`
 * position if it is not already present on `api` — never at the end by
 * accident.
 *
 * Implementation: if `key` is already present, this is a plain assignment
 * (updating a value in place never moves its key). If it is absent, every
 * field that `API_FIELD_ORDER` places AT OR AFTER `key` and that already
 * exists on `api` is removed and remembered, `key` is set, and then those
 * remembered fields are set back in their own relative order — which
 * re-inserts them (and therefore `key`, ahead of them) at the very
 * position `API_FIELD_ORDER` says they belong, regardless of how many of
 * them there were or which ones existed.
 */
export function setApiField<K extends keyof Api>(
  api: Api,
  key: K,
  value: Api[K],
): void {
  if (key in api) {
    api[key] = value;
    return;
  }
  const keyIndex = API_FIELD_ORDER.indexOf(key);
  const trailingFields =
    keyIndex === -1 ? [] : API_FIELD_ORDER.slice(keyIndex + 1);
  const saved: { field: keyof Api; value: unknown }[] = [];
  for (const field of trailingFields) {
    if (field in api) {
      saved.push({ field, value: api[field] });
      delete (api as Partial<Api>)[field];
    }
  }
  api[key] = value;
  for (const { field, value: fieldValue } of saved) {
    (api as unknown as Record<string, unknown>)[field] = fieldValue;
  }
}
