// Pure helper for the secrets screen: finds every `{{secret:NAME}}`
// reference in the workspace's raw file text so a missing secret is visible
// before a request fails on it (task-7 brief, step 4).
//
// Mirrors the tokenizing `crates/core/src/vars.rs::Scope::expand` does at
// request time — `{{`, then an optional `secret:` prefix, then a name,
// trimmed on both sides, up to the next `}}` — closely enough to find the
// same references, without re-implementing the full expression language
// (concatenation, `computed` functions, etc.) that isn't needed just to spot
// an unresolved name. This is intentionally a superset scan over raw text,
// not a parse of the JSON structure: it runs over `ApiDto.text` (the exact
// on-disk bytes), so a reference nested anywhere — headers, query, body,
// auth — is found without walking every field by hand.
const SECRET_REF = /\{\{\s*secret:\s*([^}]*?)\s*\}\}/g;

/** Every distinct secret name referenced in `text`, in first-seen order. */
export function secretRefsIn(text: string): string[] {
  const seen = new Set<string>();
  const names: string[] = [];
  for (const match of text.matchAll(SECRET_REF)) {
    const name = match[1];
    if (name && !seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  }
  return names;
}

/**
 * Every secret name referenced by any of `texts` (one per workspace file)
 * that is NOT present in `knownNames` — sorted, deduplicated. Used to warn
 * "this secret is referenced but not defined" before a request hits it.
 */
export function missingSecretRefs(
  texts: string[],
  knownNames: string[],
): string[] {
  const known = new Set(knownNames);
  const missing = new Set<string>();
  for (const text of texts) {
    for (const name of secretRefsIn(text)) {
      if (!known.has(name)) {
        missing.add(name);
      }
    }
  }
  return [...missing].sort();
}
