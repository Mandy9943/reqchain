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
    if (ep.auth.type === "inherit") return true;
    current = ep.auth.type === "chained" ? ep.auth.source.endpoint : null;
  }
  return false;
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
 *    detected").
 *
 * This is the ONLY set the source dropdown may ever offer — never free
 * text — so a cycle or self-reference is unrepresentable rather than
 * merely reported after the fact.
 */
export function validSourceEndpoints(
  api: Api,
  editingEndpointId: string | null,
): Endpoint[] {
  return api.endpoints.filter((candidate) => {
    if (editingEndpointId !== null) {
      if (candidate.id === editingEndpointId) return false;
      return !chainWalk(api, candidate.id).includes(editingEndpointId);
    }
    return !reachesInheritedEndpoint(api, candidate.id);
  });
}

// ---- Question 2: extract ----

export type ExtractChoice =
  | "default"
  | "body-jsonpath"
  | "body-regex"
  | "header"
  | "status";

export const DEFAULT_EXTRACT_JSON_PATH = "$.access_token";

/** Which of the five choices `extract` currently represents. `undefined`
 * (the field omitted from the file) is the documented default. Defensive
 * against an unrecognized `from` tag (falls back to `"default"`, same
 * spirit as `authTypeOf`). */
export function extractChoiceOf(extract: AuthExtract | undefined): ExtractChoice {
  if (extract === undefined) return "default";
  switch (extract.from) {
    case "body":
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
