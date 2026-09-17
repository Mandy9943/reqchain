// Typed TS mirror of crates/core/src/model.rs. This is the ONLY frontend
// representation of the Api document — forms mutate an `Api`, never a
// second parallel shape. Field order in every object-building function
// below (`emptyApi`, `emptyEndpoint`, and any mutator elsewhere) MUST match
// the field declaration order in model.rs, because `serializeApi` is a
// plain `JSON.stringify` and JS object key order is insertion order. Get
// this wrong and a single-field form edit reorders (and thus diffs) the
// whole file.
//
// `serializeApi` is not a second serializer: it produces buffer text that
// `save_api` (Rust) parses and re-serializes with `Api::to_json_string`.
// The bytes that land on disk always come from Rust. See
// crates/core/tests/fixtures_roundtrip.rs, which pins the Rust side's own
// round-trip stability.
//
// Optionality: a field is optional here (`?`) only if `model.rs` marks it
// `#[serde(default, skip_serializing_if = "Option::is_none")]` — i.e. Rust
// itself omits it when empty. Every other `#[serde(default = ...)]` field
// (no `skip_serializing_if`) is REQUIRED here, because Rust always writes
// it, and every real fixture on disk carries it (`"variables": {}`,
// `"headers": {}`, an explicit `auth` block, ...). Marking those required
// makes the compiler catch a builder that forgets one — instead of the
// buffer looking unlike every sibling object in the file until the next
// save, and instead of a form reading e.g. `endpoint.headers` right after
// creation and getting `undefined` instead of `{}`. `emptyApi` and
// `emptyEndpoint` (and any future builder) must populate these with the
// same defaults Rust does.

export type Method =
  | "GET"
  | "POST"
  | "PUT"
  | "PATCH"
  | "DELETE"
  | "HEAD"
  | "OPTIONS";

export type Body =
  | { type: "json"; content: unknown }
  | { type: "form"; fields: Record<string, string> }
  | {
      type: "multipart";
      fields: Record<string, string>;
      files: Record<string, string>;
    }
  | { type: "text"; content: string }
  | { type: "xml"; content: string }
  | { type: "binary"; path: string };

export interface ChainSource {
  endpoint: string;
}

export type AuthExtract =
  | {
      from: "body";
      jsonPath?: string;
      xpath?: string;
      regex?: string;
    }
  | { from: "header"; name: string; regex?: string }
  | { from: "status" };

export type TtlUnit = "seconds" | "milliseconds";

export type AuthTtl =
  | { from: "body"; jsonPath: string; unit: TtlUnit }
  | { from: "fixed"; seconds: number }
  | { from: "absolute"; jsonPath: string };

export type AuthInject =
  | { into: "header"; name: string; template: string }
  | { into: "query"; name: string; template: string }
  | { into: "body"; pointer: string; template: string };

export type Auth =
  | { type: "inherit" }
  | { type: "none" }
  | { type: "basic"; username: string; password: string }
  | { type: "bearer"; token: string }
  | { type: "header"; headers: Record<string, string> }
  | { type: "computed"; name: string; expression: string }
  | {
      type: "chained";
      source: ChainSource;
      extract?: AuthExtract;
      ttl?: AuthTtl;
      inject: AuthInject;
      retryOn: number[];
    };

export interface Endpoint {
  id: string;
  name: string;
  method: Method;
  path: string;
  headers: Record<string, string>;
  query: Record<string, string>;
  variables: Record<string, string>;
  auth: Auth;
  body?: Body;
}

export interface Environment {
  name: string;
  variables: Record<string, string>;
}

export interface HistoryConfig {
  storeBodies: boolean;
}

export interface Api {
  schemaVersion: 1;
  id: string;
  name: string;
  baseUrl: string;
  variables: Record<string, string>;
  environments: Environment[];
  auth: Auth;
  history?: HistoryConfig;
  endpoints: Endpoint[];
}

export type ParseResult =
  | { ok: true; api: Api }
  | { ok: false; error: string };

/**
 * Validates document SHAPE, not semantics — `schemaVersion === 1` and the
 * presence of the required top-level fields with the right primitive
 * types. Anything deeper (dangling endpoint refs, invalid auth chains,
 * etc.) is the validator's job (`lint`), not this function's.
 */
export function parseApi(text: string): ParseResult {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return { ok: false, error: "document is not a JSON object" };
  }
  const obj = value as Record<string, unknown>;
  if (obj.schemaVersion !== 1) {
    return { ok: false, error: "missing or unsupported schemaVersion" };
  }
  if (typeof obj.id !== "string" || obj.id.length === 0) {
    return { ok: false, error: "missing or empty \"id\"" };
  }
  if (typeof obj.name !== "string" || obj.name.length === 0) {
    return { ok: false, error: "missing or empty \"name\"" };
  }
  if (typeof obj.baseUrl !== "string" || obj.baseUrl.length === 0) {
    return { ok: false, error: "missing or empty \"baseUrl\"" };
  }
  if (!Array.isArray(obj.endpoints)) {
    return { ok: false, error: "missing or invalid \"endpoints\"" };
  }
  // Same "present but not an object" defence as the per-endpoint loop below
  // — `variables` is `#[serde(default)]` (Vars/BTreeMap) with no
  // `skip_serializing_if`, so it's absent only in older/hand-written files,
  // never in one Rust wrote; a JSON-tab edit setting it to `null`/an array
  // must not reach `ApiForm`'s `KeyValueRows.svelte`.
  if (
    "variables" in obj &&
    obj.variables !== undefined &&
    (typeof obj.variables !== "object" ||
      obj.variables === null ||
      Array.isArray(obj.variables))
  ) {
    return { ok: false, error: "\"variables\" must be an object" };
  }
  if ("environments" in obj && obj.environments !== undefined) {
    if (!Array.isArray(obj.environments)) {
      return { ok: false, error: "\"environments\" must be an array" };
    }
    for (const [i, env] of obj.environments.entries()) {
      const envError = validateEnvironmentShape(env, `environments[${i}]`);
      if (envError) {
        return { ok: false, error: envError };
      }
    }
  }
  if ("history" in obj && obj.history !== undefined) {
    const historyError = validateHistoryShape(obj.history);
    if (historyError) {
      return { ok: false, error: historyError };
    }
  }
  if ("auth" in obj && obj.auth !== undefined) {
    const authError = validateAuthShape(obj.auth, "auth");
    if (authError) {
      return { ok: false, error: authError };
    }
  }
  // Deliberately narrow, not a full re-validation of every endpoint field:
  // this exists only to stop a JSON-tab edit that deletes/corrupts
  // `headers`/`query`/`variables` from silently producing `ok: true` and
  // then crashing the form the moment it does `Object.entries(...)` on a
  // value that isn't an object. A field that is simply ABSENT is left
  // alone (older/hand-written fixtures may omit it; the form treats a
  // missing record defensively as empty, see keyValueRows.ts's
  // `asRecord`) — only a field that is PRESENT but not a plain object
  // (`null`, an array, a string, ...) is rejected here.
  for (const [i, ep] of (obj.endpoints as unknown[]).entries()) {
    if (typeof ep !== "object" || ep === null || Array.isArray(ep)) {
      return { ok: false, error: `endpoints[${i}] is not an object` };
    }
    const epObj = ep as Record<string, unknown>;
    for (const field of ["headers", "query", "variables"] as const) {
      const fieldValue = epObj[field];
      if (
        field in epObj &&
        (typeof fieldValue !== "object" ||
          fieldValue === null ||
          Array.isArray(fieldValue))
      ) {
        return {
          ok: false,
          error: `endpoints[${i}].${field} must be an object`,
        };
      }
    }
    if ("body" in epObj && epObj.body !== undefined) {
      const bodyError = validateBodyShape(epObj.body, i);
      if (bodyError) {
        return { ok: false, error: bodyError };
      }
    }
    if ("auth" in epObj && epObj.auth !== undefined) {
      const authError = validateAuthShape(epObj.auth, `endpoints[${i}].auth`);
      if (authError) {
        return { ok: false, error: authError };
      }
    }
  }
  return { ok: true, api: obj as unknown as Api };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireStringField(
  obj: Record<string, unknown>,
  field: string,
  path: string,
): string | null {
  if (typeof obj[field] !== "string") {
    return `${path}.${field} must be a string`;
  }
  return null;
}

function requireOptionalStringField(
  obj: Record<string, unknown>,
  field: string,
  path: string,
): string | null {
  if (field in obj && obj[field] !== undefined && typeof obj[field] !== "string") {
    return `${path}.${field} must be a string`;
  }
  return null;
}

/**
 * `Environment`, `model.rs`'s `#[serde(deny_unknown_fields)]` struct with
 * `name: String` (no serde default — required whenever the entry exists at
 * all) and `variables: Vars` (`#[serde(default)]`, so optional whenever
 * present). Same purpose as the per-endpoint headers/query/variables loop
 * above: a JSON-tab edit could otherwise hand `ApiForm` an `environments`
 * entry with a missing/non-string `name` or a non-object `variables`,
 * either crashing the form outright or looking fine here and failing at
 * Save time with a raw serde error instead.
 */
function validateEnvironmentShape(value: unknown, path: string): string | null {
  if (!isPlainObject(value)) return `${path} must be an object`;
  const nameError = requireStringField(value, "name", path);
  if (nameError) return nameError;
  if (
    "variables" in value &&
    value.variables !== undefined &&
    (typeof value.variables !== "object" ||
      value.variables === null ||
      Array.isArray(value.variables))
  ) {
    return `${path}.variables must be an object`;
  }
  return null;
}

/**
 * `HistoryConfig`, `model.rs`'s `#[serde(deny_unknown_fields)]` struct with
 * one field, `storeBodies: bool` (`#[serde(default = "default_true")]` —
 * optional whenever present, same "absent is fine, present-but-wrong is
 * not" rule as everything else in this file). `history` itself is
 * `Option<HistoryConfig>` on `Api`, so it may be absent entirely; this is
 * only reached when it is present.
 */
function validateHistoryShape(value: unknown): string | null {
  if (!isPlainObject(value)) return "\"history\" must be an object";
  if (
    "storeBodies" in value &&
    value.storeBodies !== undefined &&
    typeof value.storeBodies !== "boolean"
  ) {
    return "\"history\".storeBodies must be a boolean";
  }
  return null;
}

function validateChainSourceShape(value: unknown, path: string): string | null {
  if (!isPlainObject(value)) return `${path} must be an object`;
  return requireStringField(value, "endpoint", path);
}

/** `AuthExtract`, discriminated by `from` (model.rs's `#[serde(tag =
 * "from")]`). Mirrors `Extract`'s field requirements in SPEC.md exactly:
 * only `header.name` has no serde default. */
function validateExtractShape(value: unknown, path: string): string | null {
  if (!isPlainObject(value)) return `${path} must be an object`;
  switch (value.from) {
    case "body":
      return (
        requireOptionalStringField(value, "jsonPath", path) ??
        requireOptionalStringField(value, "xpath", path) ??
        requireOptionalStringField(value, "regex", path)
      );
    case "header":
      return (
        requireStringField(value, "name", path) ??
        requireOptionalStringField(value, "regex", path)
      );
    case "status":
      return null;
    default:
      return `${path}.from must be one of body, header, status`;
  }
}

/** `AuthTtl`, discriminated by `from`. `body.jsonPath`, `fixed.seconds` and
 * `absolute.jsonPath` all have no serde default — required whenever
 * present at all. */
function validateTtlShape(value: unknown, path: string): string | null {
  if (!isPlainObject(value)) return `${path} must be an object`;
  switch (value.from) {
    case "body": {
      const jsonPathError = requireStringField(value, "jsonPath", path);
      if (jsonPathError) return jsonPathError;
      if (
        "unit" in value &&
        value.unit !== undefined &&
        value.unit !== "seconds" &&
        value.unit !== "milliseconds"
      ) {
        return `${path}.unit must be "seconds" or "milliseconds"`;
      }
      return null;
    }
    case "fixed":
      // `model.rs`'s `Fixed { seconds: u64 }` — a non-negative integer.
      // `typeof === "number"` alone would accept `-1` or `3.5`, both of
      // which Rust's own `u64` deserializer rejects outright at Save time.
      if (
        typeof value.seconds !== "number" ||
        !Number.isInteger(value.seconds) ||
        value.seconds < 0
      ) {
        return `${path}.seconds must be a non-negative integer`;
      }
      return null;
    case "absolute":
      return requireStringField(value, "jsonPath", path);
    default:
      return `${path}.from must be one of body, fixed, absolute`;
  }
}

/** `AuthInject`, discriminated by `into`. `header.name`, `header.template`
 * and `body.pointer` have no serde default; `query`/`body`'s `template`
 * does (defaults to `"{{value}}"`), so it's optional whenever present. */
function validateInjectShape(value: unknown, path: string): string | null {
  if (!isPlainObject(value)) return `${path} must be an object`;
  switch (value.into) {
    case "header":
      return (
        requireStringField(value, "name", path) ??
        requireStringField(value, "template", path)
      );
    case "query":
      return (
        requireStringField(value, "name", path) ??
        requireOptionalStringField(value, "template", path)
      );
    case "body":
      return (
        requireStringField(value, "pointer", path) ??
        requireOptionalStringField(value, "template", path)
      );
    default:
      return `${path}.into must be one of header, query, body`;
  }
}

/**
 * Shallow shape check for `auth` (API-level or per-endpoint), same spirit
 * and purpose as `validateBodyShape`: a JSON-tab edit can set `auth` to
 * anything at all, even though `model.ts`'s `Auth` type claims seven closed
 * variants — this exists to stop such a document from producing `ok: true`
 * and then either crashing `AuthEditor`/`ChainedAuthBuilder` (e.g. reading
 * `.username` off a value that was never a `basic` auth at all) or, just as
 * bad, producing a document that LOOKS fine to this form but that Rust's
 * `Api::from_json` rejects outright at Save time with a raw `missing
 * field ...` error.
 *
 * Field requirements below mirror exactly which fields `model.rs`'s `Auth`
 * (and its `ChainSource`/`AuthExtract`/`AuthTtl`/`AuthInject` satellites)
 * mark `#[serde(default...)]` — a field WITHOUT that attribute has no
 * fallback on load, so its presence is required here; a field WITH it
 * (`chained.extract`, `chained.ttl`, `chained.retryOn`, `body-ttl.unit`,
 * query/body-inject's `template`, `multipart.files`'s cousins in
 * `validateBodyShape`) is only checked when present, never required — the
 * same "absent is fine, present-but-wrong is not" rule the
 * headers/query/variables checks above already follow.
 *
 * Not a full re-validation of every variant's semantics (an invalid
 * `jsonPath`, an unknown `computed` function, a dangling chained
 * `source.endpoint`, a cycle) — that stays `lint`'s job.
 *
 * Deliberately does NOT mirror `#[serde(deny_unknown_fields)]`: an extra,
 * unrecognized key on an otherwise well-shaped auth object is left alone
 * here. That is a considered choice, not an oversight — the two kinds of
 * defect this function exists to catch are not symmetric:
 *  - a MISSING required field crashes a JS code path in this file (or a
 *    sibling `AuthEditor`/`ChainedAuthBuilder` render) with an
 *    `undefined`-is-not-an-object exception, which is what every check
 *    above prevents;
 *  - an EXTRA field crashes nothing here — every field access in this
 *    codebase reads specific named properties, never enumerates "all of
 *    them" — and is caught safely at Save time by `Api::from_json`'s own
 *    `deny_unknown_fields`, which rejects it with a comprehensible `unknown
 *    field ...` error and leaves the on-disk file untouched
 *    (`save_api_rejects_an_invalid_document_and_leaves_the_file_untouched`).
 * Mirroring `deny_unknown_fields` here would mean hand-maintaining a second
 * exhaustive allow-list of every field name per variant, purely to convert
 * an already-safe Save-time rejection into an earlier one — a real cost
 * (one more thing to keep in sync with `model.rs`) for no crash this
 * function doesn't already prevent some other way.
 */
function validateAuthShape(value: unknown, path: string): string | null {
  if (!isPlainObject(value)) return `${path} must be an object`;
  switch (value.type) {
    case "inherit":
    case "none":
      return null;
    case "basic":
      return (
        requireStringField(value, "username", path) ??
        requireStringField(value, "password", path)
      );
    case "bearer":
      return requireStringField(value, "token", path);
    case "header": {
      // `model.rs`'s `Header { headers: BTreeMap<String, String> }` — every
      // VALUE must be a string too, not just the object shape itself.
      // Without this, `{"headers": {"X-Api-Key": 1}}` would pass here,
      // render fine (JS coerces a number into a string in the input's
      // `value` binding), and then fail at Save with a raw serde "invalid
      // type: integer, expected a string" error.
      if (!("headers" in value) || !isPlainObject(value.headers)) {
        return `${path}.headers must be an object`;
      }
      for (const [k, v] of Object.entries(value.headers)) {
        if (typeof v !== "string") {
          return `${path}.headers.${k} must be a string`;
        }
      }
      return null;
    }
    case "computed":
      return (
        requireStringField(value, "name", path) ??
        requireStringField(value, "expression", path)
      );
    case "chained": {
      if (!("source" in value)) return `${path}.source is required`;
      const sourceError = validateChainSourceShape(value.source, `${path}.source`);
      if (sourceError) return sourceError;
      if ("extract" in value && value.extract !== undefined) {
        const extractError = validateExtractShape(value.extract, `${path}.extract`);
        if (extractError) return extractError;
      }
      if ("ttl" in value && value.ttl !== undefined) {
        const ttlError = validateTtlShape(value.ttl, `${path}.ttl`);
        if (ttlError) return ttlError;
      }
      if (!("inject" in value)) return `${path}.inject is required`;
      const injectError = validateInjectShape(value.inject, `${path}.inject`);
      if (injectError) return injectError;
      if ("retryOn" in value && value.retryOn !== undefined) {
        // `model.rs`'s `retry_on: Vec<u16>` — every element must be an
        // integer in u16's range (0-65535). Checking only `typeof ===
        // "number"` would accept `-1`, `99999` or `401.5`, all of which
        // Rust's own `u16` deserializer rejects at Save time (SPEC.md's
        // 100-599 HTTP-status restriction is a separate, semantic `lint`
        // check, not part of the type itself, and stays out of scope here
        // — same "shape, not semantics" boundary `validateBodyShape`
        // already draws).
        if (
          !Array.isArray(value.retryOn) ||
          !value.retryOn.every(
            (n) => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 65535,
          )
        ) {
          return `${path}.retryOn must be an array of integers in 0-65535`;
        }
      }
      return null;
    }
    default:
      return `${path}.type must be one of inherit, none, basic, bearer, header, computed, chained`;
  }
}

/**
 * Shallow shape check for one endpoint's `body`, same spirit as the
 * headers/query/variables loop above: this exists to stop a JSON-tab edit
 * that corrupts `body` from producing `ok: true` and then either crashing
 * `BodyEditor` (e.g. `Object.entries(undefined)` on a missing
 * `fields`/`files`, or handing a non-string straight to CodeMirror) OR —
 * the case that matters just as much — producing a document that LOOKS
 * fine to this form but that Rust's `Api::from_json` will reject outright
 * at Save time with a raw `missing field ...` error, for an endpoint the
 * user may never have touched the body editor on at all.
 *
 * `model.rs`'s `Body` variants are NOT uniformly defaulted:
 * `Json.content`, `Form.fields` and `Multipart.fields` have no
 * `#[serde(default)]` at all, so serde requires them to be present —
 * unlike `Multipart.files`, which does have `#[serde(default)]` and may be
 * absent. So "the field is simply missing" is NOT the safe/lenient case
 * here the way it is for `headers`/`query`/`variables` above — presence is
 * required for exactly the fields Rust requires it for, mirroring the
 * `text`/`xml`/`binary` cases below (which already implicitly require
 * `content`/`path` by rejecting anything that isn't a string, `undefined`
 * included).
 *
 * Not a full re-validation of every variant's semantics — that stays
 * `lint`'s job.
 */
function validateBodyShape(value: unknown, endpointIndex: number): string | null {
  const prefix = `endpoints[${endpointIndex}].body`;
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return `${prefix} must be an object`;
  }
  const body = value as Record<string, unknown>;

  function requireObjectField(field: "fields" | "files"): string | null {
    if (!(field in body)) {
      return `${prefix}.${field} is required`;
    }
    const fieldValue = body[field];
    if (
      typeof fieldValue !== "object" ||
      fieldValue === null ||
      Array.isArray(fieldValue)
    ) {
      return `${prefix}.${field} must be an object`;
    }
    return null;
  }

  switch (body.type) {
    case "json":
      // `content` is `unknown` — any JSON value is a valid value for it —
      // but `model.rs`'s `Json { content }` has no `#[serde(default)]`, so
      // the KEY itself must be present (even `null` counts as present: a
      // JSON `null` is a real value for `serde_json::Value`).
      if (!("content" in body)) {
        return `${prefix}.content is required`;
      }
      return null;
    case "text":
    case "xml":
      if (typeof body.content !== "string") {
        return `${prefix}.content must be a string`;
      }
      return null;
    case "binary":
      if (typeof body.path !== "string") {
        return `${prefix}.path must be a string`;
      }
      return null;
    case "form":
      return requireObjectField("fields");
    case "multipart": {
      const fieldsError = requireObjectField("fields");
      if (fieldsError) return fieldsError;
      // Unlike `fields`, `model.rs`'s `Multipart.files` DOES have
      // `#[serde(default)]` — genuinely optional, so only checked when
      // present (same leniency as `headers`/`query`/`variables`).
      if (
        "files" in body &&
        (typeof body.files !== "object" ||
          body.files === null ||
          Array.isArray(body.files))
      ) {
        return `${prefix}.files must be an object`;
      }
      return null;
    }
    default:
      return `${prefix}.type must be one of json, form, multipart, text, xml, binary`;
  }
}

/** `JSON.stringify(api, null, 2) + "\n"` — the buffer text, not the on-disk bytes. */
export function serializeApi(api: Api): string {
  return JSON.stringify(api, null, 2) + "\n";
}

/**
 * Field order matches `Api` in model.rs. Every field Rust always
 * serializes (see the optionality note above) is populated with Rust's own
 * default, so the result already looks like every other object in the
 * file — no waiting for a save to normalize it.
 */
export function emptyApi(id: string, name: string): Api {
  return {
    schemaVersion: 1,
    id,
    name,
    baseUrl: "https://",
    variables: {},
    environments: [],
    auth: { type: "none" }, // Auth::none(), Api's #[serde(default)]
    endpoints: [],
  };
}

/**
 * Field order matches `Endpoint` in model.rs. Every field Rust always
 * serializes is populated with Rust's own default — see `emptyApi`.
 */
export function emptyEndpoint(id: string, name: string): Endpoint {
  return {
    id,
    name,
    method: "GET",
    path: "/",
    headers: {},
    query: {},
    variables: {},
    auth: { type: "inherit" }, // Auth::inherit(), Endpoint's #[serde(default)]
  };
}

/**
 * Derives an id from a user-typed name: lowercase, non-alphanumeric runs
 * collapsed to a single `-`, leading/trailing `-` trimmed. Used by the
 * sidebar's "New API"/"New endpoint" rows so the user never has to type an
 * id by hand. Can return `""` for a name with no alphanumeric characters at
 * all (e.g. "***") — callers must treat that as "no valid id" and refuse to
 * create anything, never fall back to an empty id.
 */
export function slugify(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}
