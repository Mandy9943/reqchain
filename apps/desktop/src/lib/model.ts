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
  }
  return { ok: true, api: obj as unknown as Api };
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
