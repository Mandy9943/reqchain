// Shared UI state (Svelte 5 runes). This is the contract tasks 8-10 build
// against — keep the exported names stable.
import { listen } from "@tauri-apps/api/event";
import { nextEnvSelection } from "./envSelection";
import {
  loadWorkspace,
  runEndpoint,
  WORKSPACE_CHANGED_EVENT,
  type ApiDto,
  type EndpointDto,
  type RunDto,
  type WorkspaceDto,
} from "./ipc";

export const ui = $state({
  workspace: { apis: [], errors: [] } as WorkspaceDto,
  // `endpointId: null` means the API itself is selected (its header row in
  // the sidebar) rather than one of its endpoints — the state an API with
  // zero endpoints is always in right after creation. Every consumer below
  // must treat that as "no endpoint selected", never crash on it, and never
  // resolve it to some endpoint by accident.
  selected: null as { apiId: string; endpointId: string | null } | null,
  // Set by reload() when `selected` pointed at an endpoint that no longer
  // exists after a reload. Holds the last-known api/endpoint so the panel
  // can keep showing it (marked removed) instead of going blank. Cleared
  // whenever a fresh, still-valid selection is made (see `select`) or the
  // endpoint reappears.
  removedSelected: null as { api: ApiDto; endpoint: EndpointDto } | null,
  env: {} as Record<string, string | null>, // apiId -> environment name
  buffers: {} as Record<string, string>, // apiId -> unsaved editor text
  // apiId -> true when the on-disk text changed (via hot reload) while the
  // buffer for that api held unsaved edits. Cleared on discard or on a
  // clean save. Never implies the buffer itself was touched — reload()
  // never overwrites a dirty buffer.
  diskChanged: {} as Record<string, boolean>,
  // apiId -> true while a `save_api` call is in flight for that api —
  // RequestPanel's Save button/Ctrl+S, or Sidebar's New endpoint/Delete
  // endpoint (both edit-then-save the same document). apiId -> true while a
  // `delete_api` call (Sidebar's Delete) is in flight for that api. Shared
  // here — not component-local state — because these actions live in two
  // different components (RequestPanel, Sidebar) that all read or write the
  // same file: without a shared registry, any two of them racing on the
  // same api can silently drop one write (a just-added endpoint vanishing,
  // an edit in the JSON tab being overwritten) or resurrect a file
  // `delete_api` just removed, with no error from either side. See
  // `canWriteApiDoc` below, the one place that reads both maps.
  savingIds: {} as Record<string, boolean>,
  deletingIds: {} as Record<string, boolean>,
  search: "",
  response: null as RunDto | null,
  runError: null as string | null,
  running: false,
  error: null as string | null,
});

// Monotonic counter bumped on every selection change, so a `run_endpoint`
// response that arrives after the user has moved on to a different
// endpoint is dropped instead of being applied to the wrong selection —
// same sequence-counter pattern used elsewhere (RequestPanel's lint/preview
// calls) rather than a third mechanism.
let runSeq = 0;

/**
 * Select an endpoint, an API on its own (`endpointId: null`), or clear the
 * selection — resetting reload-tracked flags.
 */
export function select(
  selection: { apiId: string; endpointId: string | null } | null,
): void {
  ui.selected = selection;
  ui.removedSelected = null;
  ui.response = null;
  ui.runError = null;
  runSeq++;
}

/**
 * True while the current selection still resolves to a real endpoint in
 * the workspace. False once a hot reload has marked it removed (see
 * `ui.removedSelected`) — anything that would call the backend with these
 * ids (`run_endpoint`, `curl_command`, `preview_endpoint`, `save_api`) must
 * gate on this, not on `ui.selected` alone, since a removed endpoint's ids
 * no longer resolve on the Rust side either.
 */
export function isSelectionLive(): boolean {
  return selectedEndpoint() !== undefined;
}

/** True while a send can actually be triggered for the current selection. */
export function canSendSelected(): boolean {
  return isSelectionLive() && !ui.running;
}

/**
 * Whether a NEW `saveApi(apiId, ...)` call may safely be started for this
 * api id right now. The one check shared by every path that writes an
 * API's document: RequestPanel's Save button/Ctrl+S, and Sidebar's New
 * endpoint and Delete endpoint (both mutate the buffer via `updateDoc` and
 * then save it, exactly like a manual JSON edit would). Refuses while
 * another save for the same api is already in flight (`savingIds`) or while
 * the whole api is being deleted (`deletingIds`) — without this, two writes
 * racing on the same file can silently drop one of them (whichever
 * `saveApi` resolves last wins, with no error from the loser), or a save
 * can write a file back moments after `delete_api` removed it.
 */
export function canWriteApiDoc(apiId: string): boolean {
  return !ui.savingIds[apiId] && !ui.deletingIds[apiId];
}

/**
 * Runs the selected endpoint, sharing one implementation (and one sequence
 * counter) between the Send button and the Ctrl+Enter shortcut so the
 * capture-before-await / stale-response-drop logic exists exactly once.
 */
export async function sendSelected(): Promise<void> {
  if (!canSendSelected()) return;
  const sel = ui.selected!;
  // Capture everything the continuation needs off the reactive graph now —
  // the selection can change while the request is in flight, and the
  // response must never land against a different one.
  // `canSendSelected()` (via `isSelectionLive()`) already guarantees
  // `selectedEndpoint()` resolves, i.e. `sel.endpointId` is not null here —
  // read the id off the resolved endpoint rather than re-widening the type.
  const apiId = sel.apiId;
  const endpointId = selectedEndpoint()!.id;
  const env = ui.env[apiId] ?? null;
  const seq = runSeq;

  ui.running = true;
  ui.runError = null;
  try {
    const result = await runEndpoint(apiId, endpointId, env);
    if (seq === runSeq) {
      ui.response = result;
      ui.runError = null;
    }
  } catch (e) {
    if (seq === runSeq) {
      ui.response = null;
      ui.runError = e instanceof Error ? e.message : String(e);
    }
  } finally {
    // Always release the lock — a stale response only skips *applying* its
    // result, it must never leave Send permanently disabled.
    ui.running = false;
  }
}

/**
 * Reloads the workspace from the backend, surfacing any failure via
 * `ui.error`. Implements the hot-reload contract (spec §9):
 *  - the selected endpoint survives; if it no longer resolves, it is kept
 *    on screen via `ui.removedSelected` instead of being cleared;
 *  - a dirty buffer is never overwritten — if its on-disk text changed
 *    underneath it, `ui.diskChanged[apiId]` is set so the UI can offer
 *    "discard mine"; a clean buffer is refreshed to the new on-disk text;
 *  - `ui.response` is never touched here, so it survives.
 */
export async function reload(): Promise<void> {
  try {
    const previousApis = new Map(ui.workspace.apis.map((a) => [a.id, a]));
    const prevSelectedApi = selectedApi();
    const prevSelectedEndpoint = selectedEndpoint();

    const workspace = await loadWorkspace();
    ui.workspace = workspace;
    ui.error = null;

    // Default each API's environment selection to its first environment,
    // without clobbering a choice the user already made — unless that
    // choice no longer names a real environment (e.g. it was removed from
    // the file), in which case it would otherwise linger as a value with no
    // matching <option>. `nextEnvSelection` is the ONE place this rule
    // lives — `ApiForm.svelte`'s environment add/rename/remove (see
    // `envSelection.ts`'s doc comment) applies the identical rule to the
    // buffer immediately after an edit, before any save; both call this
    // same function so the two can't drift apart.
    for (const api of workspace.apis) {
      ui.env[api.id] = nextEnvSelection(api.environments, ui.env[api.id]);
    }

    // Selected endpoint (or, for an API-only selection, the API itself)
    // survives, marked "removed" if it no longer resolves.
    if (ui.selected) {
      if (selectionResolves(workspace, ui.selected)) {
        ui.removedSelected = null;
      } else if (prevSelectedApi && prevSelectedEndpoint) {
        // Keep `ui.selected` and `ui.response` as they are — do not clear
        // the panel — and remember the last-known api/endpoint to render.
        ui.removedSelected = {
          api: prevSelectedApi,
          endpoint: prevSelectedEndpoint,
        };
      } else {
        // The selection didn't resolve even before this reload (shouldn't
        // normally happen) — nothing sensible to keep showing.
        ui.selected = null;
        ui.removedSelected = null;
        ui.response = null;
      }
    }

    // Buffers: never overwrite a dirty one. A clean buffer (matches the
    // previously-known on-disk text) is refreshed to the new text so it
    // keeps reflecting the file. A dirty one whose on-disk text changed
    // underneath it is flagged via `diskChanged` instead of being touched.
    for (const api of workspace.apis) {
      const buffer = ui.buffers[api.id];
      if (buffer === undefined) continue;
      const prev = previousApis.get(api.id);
      const prevText = prev?.text;
      const wasDirtyBefore = prevText !== undefined && buffer !== prevText;
      if (!wasDirtyBefore) {
        // No local edits relative to the previously-known text — safe to
        // just adopt the new on-disk text, no conflict to flag.
        ui.buffers[api.id] = api.text;
        ui.diskChanged[api.id] = false;
      } else if (buffer === api.text) {
        // The buffer's local edits happen to already match the new on-disk
        // text (e.g. the same change was made both places) — no conflict.
        ui.diskChanged[api.id] = false;
      } else if (prevText !== api.text) {
        // Buffer is dirty and the on-disk text actually moved underneath
        // it — flag the conflict, but never touch the buffer itself.
        ui.diskChanged[api.id] = true;
      }
    }
  } catch (e) {
    ui.error = e instanceof Error ? e.message : String(e);
  }
}

/** Discards a dirty buffer's local edits, adopting the current on-disk text. */
export function discardLocalChanges(apiId: string): void {
  const api = ui.workspace.apis.find((a) => a.id === apiId);
  if (!api) return;
  ui.buffers[apiId] = api.text;
  ui.diskChanged[apiId] = false;
}

/**
 * Whether `selected` still resolves against `workspace`: for an endpoint
 * selection, the endpoint must still exist under its API; for an API-only
 * selection (`endpointId: null`), the API itself existing is enough — there
 * is no endpoint to look up, and there must never be one pretended into
 * existence.
 */
function selectionResolves(
  workspace: WorkspaceDto,
  selected: { apiId: string; endpointId: string | null },
): boolean {
  const api = workspace.apis.find((a) => a.id === selected.apiId);
  if (!api) return false;
  if (selected.endpointId === null) return true;
  return api.endpoints.some((ep) => ep.id === selected.endpointId);
}

export function selectedApi(): ApiDto | undefined {
  if (!ui.selected) return undefined;
  return ui.workspace.apis.find((a) => a.id === ui.selected!.apiId);
}

/** `undefined` both when nothing is selected and for an API-only selection
 * (`endpointId: null`) — the latter is deliberate: it must never be
 * mistaken for a resolved endpoint. */
export function selectedEndpoint(): EndpointDto | undefined {
  if (!ui.selected || ui.selected.endpointId === null) return undefined;
  const endpointId = ui.selected.endpointId;
  return selectedApi()?.endpoints.find((ep) => ep.id === endpointId);
}

/** `selectedApi()`, falling back to the last-known snapshot of a removed selection. */
export function selectedApiOrRemoved(): ApiDto | undefined {
  return selectedApi() ?? ui.removedSelected?.api;
}

/** `selectedEndpoint()`, falling back to the last-known snapshot of a removed selection. */
export function selectedEndpointOrRemoved(): EndpointDto | undefined {
  return selectedEndpoint() ?? ui.removedSelected?.endpoint;
}

// The backend has already reloaded its own state by the time this fires —
// just re-invoke load_workspace.
//
// Deliberately module-scope with no unlisten: this is a singleton store for
// a single-window app, so it lives exactly as long as the app does — there
// is no component lifecycle to tear it down against, and no scenario where
// it should stop listening while the window is open. Do not "fix" this into
// a component-owned listener with an unlisten; that would just make it stop
// working the moment whatever component registered it unmounts.
listen(WORKSPACE_CHANGED_EVENT, () => {
  void reload();
});
