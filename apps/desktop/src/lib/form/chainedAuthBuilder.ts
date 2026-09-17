// Pure helpers behind `ChainedAuthBuilder.svelte`: the guided four-question
// chained-auth editor, and — the part most worth pinning — the cycle and
// reachability computation behind question 1's endpoint dropdown.
//
// That dropdown must never disagree with the engine
// (`crates/core/src/chain.rs`'s `Executor::run_inner`, which walks a
// `visited` list of endpoint ids and rejects a repeat as a cycle): every
// walk here mirrors that walk exactly, including how `inherit` is resolved
// (`auth::resolve` in `crates/core/src/auth.rs`, mirrored here as
// `resolveAuth` in `authEditor.ts`). A builder that disagrees with the
// engine is worse than no check at all — see the task brief.
//
// Runtime input is not bound by the TS types — see `authEditor.ts`'s header
// comment; the same "validate in `parseApi` AND coerce defensively here"
// rule applies to every function below.

import type {
  Api,
  Auth,
  AuthExtract,
  AuthInject,
  AuthTtl,
  Endpoint,
} from "../model";
import { resolveAuth } from "./authEditor";

export type Chained = Extract<Auth, { type: "chained" }>;

/** Mirrors `crates/core/src/chain.rs::MAX_DEPTH` exactly — the maximum
 * number of DISTINCT endpoints a chain walk may visit before the engine
 * (and `reqchain validate`, via `crates/core/src/validate.rs::check_cycles`,
 * which `save_api` refuses to write past) reports "auth chain deeper than 5
 * levels" as a hard error. There is no cross-language way to literally
 * import the Rust constant here, so this is the ONE place in this module
 * that repeats the number `5` — every depth check below reads it from
 * here, never re-typed. If `crates/core/src/chain.rs::MAX_DEPTH` ever
 * changes, this must change with it. */
export const MAX_DEPTH = 5;

/** Defensive coercion: `auth` itself if it really is `chained`, else a
 * fresh default-shaped stand-in so the builder never has to null-check
 * every field it reads. `parseApi`'s `validateAuthShape` should make a
 * non-chained value here unreachable in practice — this is the second,
 * independent layer, same as `bodyEditor.ts`'s runtime guards. */
export function asChained(auth: Auth): Chained {
  if (auth.type === "chained") return auth;
  return {
    type: "chained",
    source: { endpoint: "" },
    extract: undefined,
    ttl: undefined,
    inject: {
      into: "header",
      name: "Authorization",
      template: "Bearer {{value}}",
    },
    retryOn: [401, 403],
  };
}

/**
 * Endpoint ids visited by walking chained-auth source pointers starting at
 * `startId`, resolving `inherit` at each step exactly like `auth::resolve`.
 * Mirrors `Executor::run_inner`'s `visited` walk in
 * `crates/core/src/chain.rs`.
 *
 * Stops — without re-adding it — the instant an id repeats: a pre-existing
 * cycle elsewhere in the document isn't this function's concern, only
 * whether continuing from `startId` would revisit something already seen
 * on THIS walk. Also stops (without error) at a dangling `source.endpoint`
 * naming no real endpoint — the engine's own walk ends there too (as an
 * "endpoint not found" error at run time, not a cycle).
 */
export function chainWalk(api: Api, startId: string): string[] {
  const visited: string[] = [];
  const seen = new Set<string>();
  let current: string | null = startId;
  while (current !== null && !seen.has(current)) {
    seen.add(current);
    visited.push(current);
    const ep = api.endpoints.find((e) => e.id === current);
    if (!ep) break;
    const resolved = resolveAuth(api, ep.auth);
    current = resolved.type === "chained" ? resolved.source.endpoint : null;
  }
  return visited;
}

/**
 * Whether following `startId`'s chain ever reaches an endpoint whose OWN
 * declared auth is `inherit` — i.e. whether setting the API-LEVEL auth to
 * a chained source of `startId` would create a cycle, since such an
 * endpoint's resolved auth IS the API-level auth being edited. Used for the
 * source dropdown when the auth being built lives at API level (no
 * endpoint id of its own to check against directly).
 */
export function reachesInheritedEndpoint(api: Api, startId: string): boolean {
  const seen = new Set<string>();
  let current: string | null = startId;
  while (current !== null && !seen.has(current)) {
    seen.add(current);
    const ep = api.endpoints.find((e) => e.id === current);
    if (!ep) return false;
    // `ep.auth` is typed as always present (`model.ts`'s `Endpoint.auth:
    // Auth`), but a hand-written or agent-written file may genuinely omit
    // it — `model.rs` defaults a missing `auth` to `Auth::inherit()`, and
    // `parseApi` accepts that (see `authEditor.ts`'s `resolveAuth`, which
    // this same coercion mirrors). Reading `ep.auth.type` directly here
    // would throw on exactly the files this product exists to let an agent
    // write.
    const declared: Auth = ep.auth ?? { type: "inherit" };
    if (declared.type === "inherit") return true;
    current = declared.type === "chained" ? declared.source.endpoint : null;
  }
  return false;
}

/** Whether selecting `candidateId` as a chained-auth source would make the
 * resulting chain longer than the engine allows. The total path length is
 * the endpoint being edited itself (1) plus everything `candidateId`'s own
 * chain walks through (`chainWalk`, which already includes `candidateId`).
 * This mirrors `crates/core/src/validate.rs::check_cycles`'s `path` growth
 * exactly (it pushes the starting endpoint, then each source in turn, and
 * errors the moment `path.len() > MAX_DEPTH`) — including counting a
 * dangling `source.endpoint` that names no real endpoint, since
 * `check_cycles` counts it too (it pushes the id before checking whether
 * the endpoint exists). */
export function wouldExceedDepth(api: Api, candidateId: string): boolean {
  return 1 + chainWalk(api, candidateId).length > MAX_DEPTH;
}

/** Why a candidate cannot be offered as a chained-auth source for the auth
 * currently being edited (`editingEndpointId`, or `null` for the API-level
 * auth) — `null` means it's a valid choice. The single source of truth
 * both `validSourceEndpoints` (the dropdown's actual options) and
 * `currentSourceProblem`/`excludedSourceCandidates` (explaining an
 * already-bad or an empty result) build on, so the two views can never
 * disagree with each other. */
export type ExclusionReason = "self" | "cycle" | "too-deep";

export function sourceExclusionReason(
  api: Api,
  editingEndpointId: string | null,
  candidateId: string,
): ExclusionReason | null {
  if (wouldExceedDepth(api, candidateId)) return "too-deep";
  if (editingEndpointId !== null) {
    if (candidateId === editingEndpointId) return "self";
    return chainWalk(api, candidateId).includes(editingEndpointId) ? "cycle" : null;
  }
  return reachesInheritedEndpoint(api, candidateId) ? "cycle" : null;
}

/**
 * The endpoints this API offers as a valid chained-auth source for the auth
 * currently being edited — `editingEndpointId` for an endpoint's own auth,
 * or `null` for the API-level auth. Excludes:
 *
 *  - the endpoint being edited itself (a source that IS what it is meant to
 *    authenticate — the trivial one-step cycle);
 *  - any endpoint whose chain, once selected as the source, would walk back
 *    around to the thing being edited — the cycle the engine's depth-first
 *    walk would reject at run time (`RunError::Chain`, "auth cycle
 *    detected");
 *  - any endpoint whose chain, once selected, would make the total path
 *    longer than the engine allows (`wouldExceedDepth` — "auth chain
 *    deeper than 5 levels", which `reqchain validate` reports as an ERROR
 *    and `save_api` refuses to write past). A concrete repro this excludes:
 *    `tests/fixtures/chain-too-deep.json` (a→b→c→d→e→f) offering `b` as a
 *    valid source for `a` would recreate exactly the 6-endpoint chain the
 *    engine already rejects.
 *
 * This is the ONLY set the source dropdown may ever offer — never free
 * text — so a cycle, self-reference or too-deep chain is unrepresentable
 * rather than merely reported after the fact.
 */
export function validSourceEndpoints(
  api: Api,
  editingEndpointId: string | null,
): Endpoint[] {
  return api.endpoints.filter(
    (candidate) => sourceExclusionReason(api, editingEndpointId, candidate.id) === null,
  );
}

/**
 * Every OTHER endpoint excluded from `validSourceEndpoints`, with why —
 * used to explain an EMPTY dropdown by naming the actual offending
 * endpoints and reasons, rather than a generic "nothing available". Omits
 * the endpoint being edited itself (excluded for the trivially obvious
 * `"self"` reason, not something worth naming as a "problem").
 */
export function excludedSourceCandidates(
  api: Api,
  editingEndpointId: string | null,
): { id: string; name: string; reason: ExclusionReason }[] {
  const out: { id: string; name: string; reason: ExclusionReason }[] = [];
  for (const candidate of api.endpoints) {
    if (candidate.id === editingEndpointId) continue;
    const reason = sourceExclusionReason(api, editingEndpointId, candidate.id);
    if (reason !== null) out.push({ id: candidate.id, name: candidate.name, reason });
  }
  return out;
}

export type SourceProblem = "missing" | "cycle" | "too-deep";

/**
 * Diagnoses the CURRENT `source.endpoint` value already on file — used to
 * render it as a disabled, named, extra option when it isn't among
 * `validSourceEndpoints`'s offerings, so an existing bad configuration
 * (`tests/fixtures/chain-cycle.json`, `chain-missing.json`,
 * `chain-too-deep.json`) is shown for what it is instead of silently
 * disappearing into the dropdown's "Select an endpoint…" placeholder —
 * which would tell the user there is no source when the file says
 * otherwise. Returns `null` when the current value is empty (nothing
 * chosen yet — not a "problem", an unset field) or is already one of the
 * valid choices.
 */
export function currentSourceProblem(
  api: Api,
  editingEndpointId: string | null,
  currentSourceId: string,
): SourceProblem | null {
  if (currentSourceId === "") return null;
  const exists = api.endpoints.some((e) => e.id === currentSourceId);
  if (!exists) return "missing";
  const reason = sourceExclusionReason(api, editingEndpointId, currentSourceId);
  if (reason === null) return null;
  // "self" folds into "cycle" for display: a source naming the endpoint
  // being edited IS the one-step cycle, described the same way.
  return reason === "too-deep" ? "too-deep" : "cycle";
}

/** Human text for `currentSourceProblem`'s (or `excludedSourceCandidates`'s)
 * result, for the disabled extra `<option>` / the empty-state explanation —
 * names the actual endpoint id and the actual problem rather than a generic
 * "invalid". */
export function sourceProblemLabel(
  id: string,
  problem: SourceProblem | ExclusionReason,
): string {
  switch (problem) {
    case "missing":
      return `${id} — no such endpoint`;
    case "cycle":
    case "self":
      return `${id} — forms a cycle`;
    case "too-deep":
      return `${id} — chain too deep (max ${MAX_DEPTH} endpoints)`;
  }
}

// ---- Question 2: extract ----

export type ExtractChoice =
  | "default"
  | "body-jsonpath"
  | "body-regex"
  | "body-xpath"
  | "header"
  | "status";

export const DEFAULT_EXTRACT_JSON_PATH = "$.access_token";

/** The engine's exact refusal message for an xpath extract
 * (`crates/core/src/chain.rs`'s `extract_value` and
 * `crates/core/src/validate.rs`'s matching validator diagnostic both use
 * this wording) — shown verbatim rather than a paraphrase, so what the
 * user reads here is what `reqchain validate`/a failed run would say. */
export const XPATH_UNSUPPORTED_MESSAGE =
  "xpath extraction is not implemented yet — use jsonPath or regex";

/**
 * Which of the six choices `extract` currently represents. `undefined`
 * (the field omitted from the file) is the documented default. Checks
 * `xpath` BEFORE `regex`/`jsonPath` — matching `chain.rs`'s own
 * `extract_value` precedence exactly (`if xpath.is_some() { return
 * Err(...) }` runs before the regex check), so an extract that has both an
 * xpath and, say, a regex is represented the same way the engine would
 * actually treat it (xpath wins, and fails), not silently as regex.
 * `"body-xpath"` exists to make an ALREADY-PRESENT xpath extract
 * (`tests/fixtures/chain.json`'s `xpath-business` endpoint) representable
 * without misreporting it as JSONPath with an empty field — it is never
 * something the selector itself offers as a new choice (see
 * `ChainedAuthBuilder.svelte`, which renders it as a disabled, informational
 * option only when it's already the current value).
 *
 * Defensive against an unrecognized `from` tag (falls back to
 * `"default"`, same spirit as `authTypeOf`). */
export function extractChoiceOf(extract: AuthExtract | undefined): ExtractChoice {
  if (extract === undefined) return "default";
  switch (extract.from) {
    case "body":
      if (extract.xpath !== undefined) return "body-xpath";
      return extract.regex !== undefined ? "body-regex" : "body-jsonpath";
    case "header":
      return "header";
    case "status":
      return "status";
    default:
      return "default";
  }
}

/** Fresh default `extract` value for a choice. Never migrates data from
 * whatever `extract` held under a different choice — see
 * `defaultAuthForType`'s doc comment for why. `"default"` returns
 * `undefined`, which is what makes the file omit `extract` entirely (and
 * is exactly what the validator's "relies on the default" warning is
 * about). */
export function buildExtract(choice: ExtractChoice): AuthExtract | undefined {
  switch (choice) {
    case "default":
      return undefined;
    case "body-jsonpath":
      return { from: "body", jsonPath: DEFAULT_EXTRACT_JSON_PATH };
    case "body-regex":
      return { from: "body", regex: "" };
    case "body-xpath":
      // Never actually reached through the selector — see
      // `extractChoiceOf`'s doc comment: `"body-xpath"` is rendered as a
      // disabled option the user cannot choose FROM, only see. Kept for
      // `switch` exhaustiveness; falls back to the JSONPath shape rather
      // than silently manufacturing a new xpath extract if it is ever
      // reached some other way.
      return { from: "body", jsonPath: DEFAULT_EXTRACT_JSON_PATH };
    case "header":
      return { from: "header", name: "" };
    case "status":
      return { from: "status" };
  }
}

// ---- Question 3: ttl ----

export type TtlChoice = "default" | "body" | "fixed" | "absolute";

export const DEFAULT_TTL_JSON_PATH = "$.expires_in";

/** Which of the four choices `ttl` currently represents. `undefined` is the
 * documented default. Defensive against an unrecognized `from` tag. */
export function ttlChoiceOf(ttl: AuthTtl | undefined): TtlChoice {
  if (ttl === undefined) return "default";
  return ttl.from === "body" || ttl.from === "fixed" || ttl.from === "absolute"
    ? ttl.from
    : "default";
}

/** Fresh default `ttl` value for a choice. Same "never migrate data across
 * a type switch" rule as `buildExtract`. */
export function buildTtl(choice: TtlChoice): AuthTtl | undefined {
  switch (choice) {
    case "default":
      return undefined;
    case "body":
      return { from: "body", jsonPath: DEFAULT_TTL_JSON_PATH, unit: "seconds" };
    case "fixed":
      return { from: "fixed", seconds: 3600 };
    case "absolute":
      return { from: "absolute", jsonPath: "$.expiresAt" };
  }
}

// ---- Question 4: inject ----

export type InjectChoice = "header" | "query" | "body";

/** Which of the three choices `inject` currently represents. Defensive
 * against an unrecognized `into` tag (falls back to `"header"`, the
 * documented default injection point). */
export function injectChoiceOf(inject: AuthInject): InjectChoice {
  return inject.into === "query" || inject.into === "body" ? inject.into : "header";
}

/** Fresh default `inject` value for a choice. Same "never migrate data
 * across a type switch" rule as `buildExtract`/`buildTtl`. */
export function buildInject(choice: InjectChoice): AuthInject {
  switch (choice) {
    case "header":
      return { into: "header", name: "Authorization", template: "Bearer {{value}}" };
    case "query":
      return { into: "query", name: "", template: "{{value}}" };
    case "body":
      return { into: "body", pointer: "", template: "{{value}}" };
  }
}

// ---- retryOn ----

/** A small, curated set of statuses worth one click — not an exhaustive
 * enumeration of the valid 100-599 range `model.rs` accepts. `401`/`403`
 * are the documented default (design spec §7). */
export const RETRY_STATUS_CHOICES = [400, 401, 403, 408, 429, 500, 502, 503] as const;

export const DEFAULT_RETRY_ON: readonly number[] = [401, 403];

/** Runtime-safe coercion for `retryOn`. `Auth::Chained.retry_on` has a
 * serde default (`#[serde(default = "default_retry_on")]`), so a
 * hand-written file — or an older fixture — may omit it entirely; a
 * JSON-tab edit may also set it to anything at all. Filters out anything
 * that isn't a valid HTTP status (`model.rs` accepts 100-599). */
export function asRetryOn(value: number[] | undefined): number[] {
  if (!Array.isArray(value)) return [...DEFAULT_RETRY_ON];
  return value.filter(
    (n) => typeof n === "number" && Number.isInteger(n) && n >= 100 && n <= 599,
  );
}

/** Any currently-set `retryOn` statuses that aren't among
 * `RETRY_STATUS_CHOICES`'s chips — e.g. a `504` a JSON-tab edit added
 * directly. These are still real, still preserved by every builder
 * function above (nothing here ever drops them), but with no chip to
 * render they'd otherwise be invisible in the UI while quietly staying in
 * the file. The component renders these read-only alongside the toggle
 * chips instead of hiding them. */
export function extraRetryStatuses(current: number[] | undefined): number[] {
  const chipSet = new Set<number>(RETRY_STATUS_CHOICES);
  return asRetryOn(current).filter((s) => !chipSet.has(s));
}

/** Adds `status` to `current` if absent, removes it if present, and keeps
 * the result sorted ascending (display-stable, and irrelevant to
 * `model.rs`'s validation, which treats `retryOn` as a set). */
export function toggleRetryStatus(current: number[] | undefined, status: number): number[] {
  const safe = asRetryOn(current);
  const next = safe.includes(status)
    ? safe.filter((s) => s !== status)
    : [...safe, status];
  return next.sort((a, b) => a - b);
}
