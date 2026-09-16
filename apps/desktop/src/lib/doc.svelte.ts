// The form/JSON buffer bridge. There is exactly one buffer per API
// (`ui.buffers[apiId]`, see state.svelte.ts) and exactly one serializer
// (`serializeApi` in model.ts, which itself only ever produces buffer text
// that the Rust backend re-parses and re-serializes on save). This module
// is how a form reads and writes that buffer without becoming a second
// source of truth.
import { parseApi, serializeApi, type Api } from "./model";
import { selectedApi, ui } from "./state.svelte";

/**
 * Parses the selected API's current buffer, falling back to its on-disk
 * text when no buffer has been created yet (nothing edited this session).
 * Returns `undefined` when nothing is selected.
 */
export function currentDoc():
  | { ok: true; api: Api }
  | { ok: false; error: string }
  | undefined {
  const api = selectedApi();
  if (!api) return undefined;
  const text = ui.buffers[api.id] ?? api.text;
  return parseApi(text);
}

/**
 * Applies `mutate` to the selected API's parsed document and writes the
 * result back into `ui.buffers[apiId]` as buffer text. If the buffer does
 * not currently parse, it is left untouched and `mutate` is never called —
 * a form must never silently rewrite a document it could not understand.
 */
export function updateDoc(mutate: (api: Api) => void): void {
  const api = selectedApi();
  if (!api) return;
  const text = ui.buffers[api.id] ?? api.text;
  const parsed = parseApi(text);
  if (!parsed.ok) return;
  mutate(parsed.api);
  ui.buffers[api.id] = serializeApi(parsed.api);
}
