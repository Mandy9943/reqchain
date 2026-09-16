// Pure helpers behind `BodyEditor.svelte`, kept dependency-free so they can
// be unit-tested without a DOM (vitest's default node environment) — same
// split `KeyValueRows.svelte`/`keyValueRows.ts` uses, and for the same
// reason: this project has no jsdom, so anything worth a real test has to
// live outside the `.svelte` file.
//
// Runtime input is not bound by the TS `Body` type: `parseApi` validates the
// top-level shape of a `body` (see model.ts), but a corrupted or
// hand-written document can still reach this module in principle if that
// validation is ever loosened, and defense-in-depth is cheap here. Every
// function below treats a `Body` whose fields don't match its variant's
// expected shape defensively rather than throwing.

import { asRecord } from "./keyValueRows";
import type { Body } from "../model";

export type BodyType =
  | "none"
  | "json"
  | "form"
  | "multipart"
  | "text"
  | "xml"
  | "binary";

const BODY_TYPES: readonly BodyType[] = [
  "none",
  "json",
  "form",
  "multipart",
  "text",
  "xml",
  "binary",
];

function isBodyType(value: unknown): value is Exclude<BodyType, "none"> {
  return (
    typeof value === "string" &&
    (BODY_TYPES as readonly string[]).includes(value) &&
    value !== "none"
  );
}

/** The type driving the UI's selector — `"none"` for an absent body, and
 * `"none"` (never a crash) for a body whose `type` tag isn't one of the six
 * known variants, since that can only happen if `parseApi`'s own shape
 * check (which already rejects an invalid `body.type`) is ever loosened. */
export function bodyTypeOf(body: Body | undefined): BodyType {
  if (!body) return "none";
  return isBodyType(body.type) ? body.type : "none";
}

/** A stable string identity for a `Body | undefined`, used by `BodyEditor`
 * to tell "the document changed under us" (a different endpoint selected,
 * or an edit made in the JSON tab) apart from "this is just the echo of the
 * change we ourselves just committed" — the same distinction
 * `Editor.svelte`/`KeyValueRows.svelte` make for their own state, applied to
 * a whole `Body` instead of buffer text or a `Record<string,string>`. */
export function bodyKey(body: Body | undefined): string {
  return JSON.stringify(body ?? null);
}

/** Safe text coercion for a field the model types as `string` but that a
 * hand-edited buffer could have set to anything. */
function asString(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/**
 * Whether `body` currently holds anything a user would be upset to lose.
 * Used to decide whether switching to a different body type needs an
 * inline confirmation first. Defensive against every field a corrupted
 * buffer could have replaced with the wrong shape — never throws.
 */
export function isBodyEmpty(body: Body | undefined): boolean {
  if (!body) return true;
  switch (body.type) {
    case "json": {
      const content = body.content;
      if (content === null || content === undefined) return true;
      if (typeof content === "string") return content.trim() === "";
      if (Array.isArray(content)) return content.length === 0;
      if (typeof content === "object") {
        return Object.keys(content as Record<string, unknown>).length === 0;
      }
      // A bare number/boolean is real content.
      return false;
    }
    case "form":
      return Object.keys(asRecord(body.fields)).length === 0;
    case "multipart":
      return (
        Object.keys(asRecord(body.fields)).length === 0 &&
        Object.keys(asRecord(body.files)).length === 0
      );
    case "text":
    case "xml":
      return asString(body.content).trim() === "";
    case "binary":
      return asString(body.path).trim() === "";
    default:
      // Not a known variant at all (shouldn't happen post-parseApi) — treat
      // as empty so switching away from it is never blocked.
      return true;
  }
}

/** Builds a fresh, empty `Body` for `type` (or `undefined` for `"none"`).
 * Switching body type never migrates data between variants — the calling
 * component's confirmation step is what makes discarding the old body an
 * explicit choice instead of a silent one; this just builds the target
 * variant's empty shape, with fields in the exact order `model.rs`
 * declares them (`type` first via the tag, then each variant's own fields)
 * so a fresh switch serializes exactly like every other Body object in the
 * file. */
export function defaultBodyForType(type: BodyType): Body | undefined {
  switch (type) {
    case "none":
      return undefined;
    case "json":
      return { type: "json", content: "" };
    case "form":
      return { type: "form", fields: {} };
    case "multipart":
      return { type: "multipart", fields: {}, files: {} };
    case "text":
      return { type: "text", content: "" };
    case "xml":
      return { type: "xml", content: "" };
    case "binary":
      return { type: "binary", path: "" };
  }
}

/** Pretty-prints a `json` body's `content` (typed `unknown` — any JSON
 * value) for display in the CodeMirror sub-editor. Defensive against a
 * value `JSON.stringify` can't handle (a cycle, a BigInt) even though
 * nothing reachable through `JSON.parse` can actually produce one. */
export function jsonContentText(content: unknown): string {
  if (content === undefined) return "";
  try {
    return JSON.stringify(content, null, 2) ?? "";
  } catch {
    return "";
  }
}

export type JsonParseResult =
  | { ok: true; content: unknown }
  | { ok: false; error: string };

/** Parses the CodeMirror sub-editor's text back into `content`. Failure
 * must not be treated as "clear the body" — the caller keeps the previous
 * committed content and shows the error inline until the text parses. */
export function parseJsonContent(text: string): JsonParseResult {
  try {
    return { ok: true, content: JSON.parse(text) };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

/** Safe text coercion for `text`/`xml` bodies' `content` and `binary`'s
 * `path` — exported so `BodyEditor.svelte` doesn't hand a non-string
 * straight to `Editor.svelte`/an `<input>`, which both expect `value:
 * string`. */
export function bodyStringField(value: unknown): string {
  return asString(value);
}
