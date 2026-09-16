// Shared UI state (Svelte 5 runes). This is the contract tasks 8-10 build
// against — keep the exported names stable.
import { listen } from "@tauri-apps/api/event";
import {
  loadWorkspace,
  WORKSPACE_CHANGED_EVENT,
  type ApiDto,
  type EndpointDto,
  type RunDto,
  type WorkspaceDto,
} from "./ipc";

export const ui = $state({
  workspace: { apis: [], errors: [] } as WorkspaceDto,
  selected: null as { apiId: string; endpointId: string } | null,
  env: {} as Record<string, string | null>, // apiId -> environment name
  buffers: {} as Record<string, string>, // apiId -> unsaved editor text
  search: "",
  response: null as RunDto | null,
  running: false,
  error: null as string | null,
});

/** Reloads the workspace from the backend, surfacing any failure via `ui.error`. */
export async function reload(): Promise<void> {
  try {
    const workspace = await loadWorkspace();
    ui.workspace = workspace;
    ui.error = null;
    // Default each API's environment selection to its first environment,
    // without clobbering a choice the user already made.
    for (const api of workspace.apis) {
      if (!(api.id in ui.env)) {
        ui.env[api.id] = api.environments[0] ?? null;
      }
    }
    // Drop a selection that no longer resolves to a real endpoint (the file
    // was edited or the endpoint removed out from under us).
    if (ui.selected && !selectedEndpointIn(workspace, ui.selected)) {
      ui.selected = null;
      ui.response = null;
    }
  } catch (e) {
    ui.error = e instanceof Error ? e.message : String(e);
  }
}

function selectedEndpointIn(
  workspace: WorkspaceDto,
  selected: { apiId: string; endpointId: string },
): boolean {
  const api = workspace.apis.find((a) => a.id === selected.apiId);
  return api?.endpoints.some((ep) => ep.id === selected.endpointId) ?? false;
}

export function selectedApi(): ApiDto | undefined {
  if (!ui.selected) return undefined;
  return ui.workspace.apis.find((a) => a.id === ui.selected!.apiId);
}

export function selectedEndpoint(): EndpointDto | undefined {
  if (!ui.selected) return undefined;
  return selectedApi()?.endpoints.find(
    (ep) => ep.id === ui.selected!.endpointId,
  );
}

// The backend has already reloaded its own state by the time this fires —
// just re-invoke load_workspace.
listen(WORKSPACE_CHANGED_EVENT, () => {
  void reload();
});
