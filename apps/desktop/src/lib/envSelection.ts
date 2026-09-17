// The one rule for "is a UI environment-selection value still sane", shared
// by every place that needs it: `reload()` (state.svelte.ts) applies it to
// `ApiDto.environments: string[]` (the on-disk list, once a fresh workspace
// load arrives) and `ApiForm.svelte`'s environment add/rename/remove
// (via `environments.ts`) applies it to the buffer's own `Environment[]`,
// immediately after an edit — before any save. Both call THIS function so
// the rule cannot drift between the two call sites; if you change it here,
// both callers pick up the change automatically. (If a future third
// consumer ever needs a genuinely different rule, split it there — don't
// let a special case creep back into this one.)

/**
 * Whether `current` still names a real environment in `names`; if not,
 * falls back to the first one, or `null` if there are none at all.
 * `current` may be `undefined` (a selection that was never set) — treated
 * exactly like `null`.
 */
export function nextEnvSelection(
  names: string[],
  current: string | null | undefined,
): string | null {
  if (current !== null && current !== undefined && names.includes(current)) {
    return current;
  }
  return names[0] ?? null;
}
