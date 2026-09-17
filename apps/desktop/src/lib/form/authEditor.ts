// Pure helpers behind `AuthEditor.svelte`. Mirrors `crates/core/src/model.rs`'s
// `Auth` enum and `crates/core/src/auth.rs`'s `resolve` function exactly, so
// the form never disagrees with what the engine will actually do.
//
// Runtime input is not bound by the TS types: a JSON-tab edit can hand this
// module an `Auth` whose `type` tag is anything at all, even though
// `model.ts`'s `Auth` type claims seven closed variants. `model.ts`'s
// `parseApi` rejects a shape this malformed before a form ever mounts (see
// `validateAuthShape` there), but every function here still defends against
// a corrupted runtime value on its own — the same "validate AND coerce"
// rule tasks 3 and 4 were bitten by skipping.

import type { Api, Auth } from "../model";

export type AuthType =
  | "inherit"
  | "none"
  | "basic"
  | "bearer"
  | "header"
  | "computed"
  | "chained";

const AUTH_TYPES: readonly AuthType[] = [
  "inherit",
  "none",
  "basic",
  "bearer",
  "header",
  "computed",
  "chained",
];

/**
 * The type driving the selector. Falls back to `fallback` for a runtime
 * `auth` that is `undefined` or whose `type` isn't one of the seven known
 * tags.
 *
 * `fallback` is REQUIRED, not defaulted, because "absent" means different
 * things at the two levels `model.rs` allows `auth` to be missing from: an
 * `Endpoint` with no `auth` key defaults to `Auth::inherit()`, while an
 * `Api` with no `auth` key defaults to `Auth::none()` (both `#[serde(default
 * = ...)]`, both real, agent-writable files per `SPEC.md`). One
 * level-agnostic default would be right for one level and wrong for the
 * other — this bit us once already (an endpoint with no `auth` key showed
 * "None" while the engine actually applied the API-level auth): pass the
 * caller's own level-appropriate fallback instead of guessing here.
 */
export function authTypeOf(auth: Auth | undefined, fallback: AuthType): AuthType {
  const t = (auth as { type?: unknown } | undefined)?.type;
  return (AUTH_TYPES as readonly string[]).includes(t as string)
    ? (t as AuthType)
    : fallback;
}

/** Resolves `inherit` exactly like `auth::resolve` in
 * `crates/core/src/auth.rs`: an endpoint's `inherit` auth resolves to the
 * API's own auth; an API whose OWN auth is `inherit` resolves to `none` —
 * there is nothing above it to inherit from.
 *
 * Defensive against both of `auth::resolve`'s inputs being genuinely absent
 * at runtime, not just malformed: `model.rs` gives both `Endpoint.auth` and
 * `Api.auth` a `#[serde(default...)]` with no `skip_serializing_if`, so a
 * hand-written or agent-written file omitting either key is VALID per
 * `SPEC.md` and accepted by `parseApi` — but `model.ts`'s `Api`/`Endpoint`
 * types mark both fields non-optional (per task 1's rule: they're always
 * WRITTEN by Rust, so the TS type says "always there"), which means the
 * compile-time type lies about a value `dto_for_api` can genuinely hand
 * this module: the raw on-disk text, unnormalized. `auth` defaults to
 * `Auth::inherit()` (endpoint level's serde default); `api.auth` defaults
 * to `Auth::none()` (API level's serde default) — exactly `model.rs`'s own
 * fallbacks, not a third guess. */
export function resolveAuth(api: Api, auth: Auth | undefined): Auth {
  const a: Auth = auth ?? { type: "inherit" };
  if (a.type !== "inherit") return a;
  const apiAuth: Auth = api.auth ?? { type: "none" };
  return apiAuth.type === "inherit" ? { type: "none" } : apiAuth;
}

/** The label for the `inherit` option at endpoint level, e.g.
 * `"inherit (chained, from the API)"` — computed with the same resolution
 * the executor uses (`resolveAuth`, above), never a second guess at what
 * `inherit` means. */
export function inheritLabel(api: Api): string {
  const resolved = resolveAuth(api, { type: "inherit" });
  return `inherit (${resolved.type}, from the API)`;
}

/**
 * Fresh default value for each auth type, in `model.rs`'s declared field
 * order (tag first, then each variant's own fields) so a type switch
 * serializes like every other `Auth` in the file. Never migrates data
 * between types — same "switching never preserves the old shape" rule
 * `bodyEditor.ts`'s `defaultBodyForType` follows, and for the same reason:
 * the shapes don't correspond to each other, so preserving fields would
 * mean guessing which ones happen to share a name.
 *
 * `chainedSource` is the endpoint id to prefill `chained`'s source with —
 * pass the first entry `validSourceEndpoints` (chainedAuthBuilder.ts) would
 * offer, or leave it `undefined` when there is none (an API with no other
 * endpoints yet). An empty source is a validation error the user resolves
 * by picking a real one once a candidate exists.
 */
export function defaultAuthForType(type: AuthType, chainedSource?: string): Auth {
  switch (type) {
    case "inherit":
      return { type: "inherit" };
    case "none":
      return { type: "none" };
    case "basic":
      return { type: "basic", username: "", password: "" };
    case "bearer":
      return { type: "bearer", token: "" };
    case "header":
      return { type: "header", headers: {} };
    case "computed":
      return { type: "computed", name: "", expression: "" };
    case "chained":
      return {
        type: "chained",
        source: { endpoint: chainedSource ?? "" },
        extract: { from: "body", jsonPath: "$.access_token" },
        ttl: { from: "body", jsonPath: "$.expires_in", unit: "seconds" },
        inject: {
          into: "header",
          name: "Authorization",
          template: "Bearer {{value}}",
        },
        retryOn: [401, 403],
      };
  }
}

/** The closed function set the `computed` expression language accepts
 * (design spec §6). An unknown function is a validation error naming the
 * function and this exact set — this constant is what the form shows next
 * to the expression field so the user never has to discover that the hard
 * way. */
export const COMPUTED_FUNCTIONS = [
  "md5(x)",
  "sha1(x)",
  "sha256(x)",
  "base64(x)",
  "now(format)",
  "+ (string concatenation)",
] as const;
