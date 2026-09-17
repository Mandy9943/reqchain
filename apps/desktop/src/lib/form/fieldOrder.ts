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

import type { Api, Endpoint } from "../model";

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

/** `Endpoint`'s field order exactly as declared in `model.rs`'s `struct
 * Endpoint`. `headers`/`query`/`variables`/`auth` are all
 * `#[serde(default)]` with no `skip_serializing_if` — legally absent from
 * an agent-written or hand-written file (this product's second purpose)
 * even though `emptyEndpoint` always writes them — so a mutator that does
 * a plain `ep.field = value` on one of THOSE risks appending it at the end
 * instead of its declared position, exactly the bug `setApiField` exists
 * to prevent at the API level. `body` is last in this order (an `Option`
 * with `skip_serializing_if`), so it has no trailing sibling to be
 * misordered relative to and a plain assignment is always safe for it. */
export const ENDPOINT_FIELD_ORDER: readonly (keyof Endpoint)[] = [
  "id",
  "name",
  "method",
  "path",
  "headers",
  "query",
  "variables",
  "auth",
  "body",
];

/**
 * Sets `obj[key] = value`, inserting `key` at its declared `order` position
 * if it is not already present on `obj` — never at the end by accident.
 *
 * Implementation: if `key` is already present, this is a plain assignment
 * (updating a value in place never moves its key). If it is absent, every
 * field that `order` places AT OR AFTER `key` and that already exists on
 * `obj` is removed and remembered, `key` is set, and then those remembered
 * fields are set back in their own relative order — which re-inserts them
 * (and therefore `key`, ahead of them) at the very position `order` says
 * they belong, regardless of how many of them there were or which ones
 * existed.
 */
function setOrderedField<T extends object, K extends keyof T>(
  obj: T,
  order: readonly (keyof T)[],
  key: K,
  value: T[K],
): void {
  if (key in obj) {
    obj[key] = value;
    return;
  }
  const keyIndex = order.indexOf(key);
  const trailingFields = keyIndex === -1 ? [] : order.slice(keyIndex + 1);
  const saved: { field: keyof T; value: unknown }[] = [];
  for (const field of trailingFields) {
    if (field in obj) {
      saved.push({ field, value: obj[field] });
      delete (obj as Partial<T>)[field];
    }
  }
  obj[key] = value;
  for (const { field, value: fieldValue } of saved) {
    (obj as unknown as Record<string, unknown>)[field as string] = fieldValue;
  }
}

/** See `setOrderedField` — specialized to `Api`/`API_FIELD_ORDER`. */
export function setApiField<K extends keyof Api>(
  api: Api,
  key: K,
  value: Api[K],
): void {
  setOrderedField(api, API_FIELD_ORDER, key, value);
}

/** See `setOrderedField` — specialized to `Endpoint`/`ENDPOINT_FIELD_ORDER`.
 * `ApiForm.svelte`'s `variables`/`environments`/`auth`/`history` mutators
 * route through `setApiField` for this exact reason; `EndpointForm.svelte`'s
 * `headers`/`query`/`auth` mutators route through this for the same one,
 * one level down. */
export function setEndpointField<K extends keyof Endpoint>(
  endpoint: Endpoint,
  key: K,
  value: Endpoint[K],
): void {
  setOrderedField(endpoint, ENDPOINT_FIELD_ORDER, key, value);
}
