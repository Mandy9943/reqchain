// Pure helpers behind `ApiForm.svelte`'s environment editor (add, rename,
// remove, per-environment variables). Kept dependency-free so they can be
// unit-tested without a DOM, the same reason `keyValueRows.ts` and
// `authEditor.ts` exist as their own modules.
//
// Runtime input is not bound by the TS types: `parseApi`'s
// `validateEnvironmentShape` rejects a document whose `environments` entry
// has a missing/non-string `name` or a non-object `variables` before a form
// ever mounts (see model.ts) — but every function here still defends
// against a corrupted runtime value on its own, the same "validate AND
// coerce" rule tasks 3-5 were each bitten by skipping once already.

import type { Environment } from "../model";
import { nextEnvSelection as nextEnvSelectionByName } from "../envSelection";
import { asRecord } from "./keyValueRows";

/** Runtime-safe coercion of one `environments` entry. Anything that isn't a
 * genuine plain object becomes an environment with an empty name and no
 * variables — never thrown on. */
function asEnvironment(value: unknown): Environment {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return { name: "", variables: {} };
  }
  const obj = value as Record<string, unknown>;
  const name = typeof obj.name === "string" ? obj.name : "";
  const variables = asRecord(obj.variables as Record<string, string>);
  return { name, variables };
}

/** Runtime-safe coercion of `api.environments` as a whole — a non-array
 * value (missing, `null`, an object) becomes the empty list. */
export function asEnvironments(value: unknown): Environment[] {
  if (!Array.isArray(value)) return [];
  return value.map(asEnvironment);
}

/** Field order matches `Environment` in model.rs (`name`, then
 * `variables`) — see model.ts's field-order note; `emptyEnvironment`
 * follows the same rule its `emptyApi`/`emptyEndpoint` siblings do. */
export function emptyEnvironment(name: string): Environment {
  return { name, variables: {} };
}

/**
 * Whether `name` may be committed as an environment's name — either a new
 * one being added, or an existing one (at `ignoreIndex`) being renamed.
 * Trims before checking, so `" prod "` and `"prod"` collide as the same
 * name rather than silently creating two visually-identical environments —
 * the same whitespace policy `keyValueRows.ts`'s `resolvedKey` already
 * applies to header/query keys.
 *
 * Returns the trimmed name to commit on success, or an error message to
 * show instead of committing anything.
 */
export function validateEnvironmentName(
  environments: Environment[],
  name: string,
  ignoreIndex: number | null,
): { ok: true; name: string } | { ok: false; error: string } {
  const trimmed = name.trim();
  if (trimmed === "") {
    return { ok: false, error: "Enter a name." };
  }
  const collision = environments.some(
    (e, i) => i !== ignoreIndex && e.name === trimmed,
  );
  if (collision) {
    return { ok: false, error: `An environment named "${trimmed}" already exists.` };
  }
  return { ok: true, name: trimmed };
}

/**
 * Keeps a UI environment-selection value (`ui.env[apiId]`) sane after an
 * environment list changes (add/rename/remove) — mirrors exactly the
 * "stale value" check `reload()` (state.svelte.ts) already applies after a
 * fresh workspace load. Applying the same rule immediately after an edit
 * (rather than only once `reload()` next runs, which for an unsaved buffer
 * edit could be arbitrarily far in the future) means a removed or
 * renamed-away selection is never left dangling in the interim.
 *
 * A thin adapter over `../envSelection`'s `nextEnvSelection` — the actual
 * rule lives there ONCE, shared with `reload()`'s own call to it, so the
 * two can never quietly drift apart. This wrapper exists only because this
 * module's callers have `Environment[]` (the buffer's shape) on hand,
 * while `reload()` has `string[]` (`ApiDto.environments`) on hand.
 */
export function nextEnvSelection(
  environments: Environment[],
  current: string | null,
): string | null {
  return nextEnvSelectionByName(
    environments.map((e) => e.name),
    current,
  );
}
