// Typed IPC layer for the Tauri backend. Mirrors the Rust DTOs in
// apps/desktop/src-tauri/src/dto.rs exactly (camelCase over the wire, per
// #[serde(rename_all = "camelCase")]). One thin async function per command,
// no logic beyond `invoke` — see ipc-surface.md for the exact call
// signatures this was generated against.
import { invoke } from "@tauri-apps/api/core";

export interface WorkspaceDto {
  apis: ApiDto[];
  errors: FileErrorDto[];
}

export interface ApiDto {
  id: string;
  name: string;
  baseUrl: string;
  environments: string[];
  endpoints: EndpointDto[];
  text: string;
  /** The file this API was loaded from, as a display string (Workspace::path_of). */
  path: string;
}

export type AuthKind =
  | "inherit"
  | "none"
  | "basic"
  | "bearer"
  | "header"
  | "computed"
  | "chained";

export interface EndpointDto {
  id: string;
  name: string;
  method: string;
  path: string;
  authKind: AuthKind;
}

export interface FileErrorDto {
  path: string;
  message: string;
}

export type Severity = "error" | "warning";

export interface DiagnosticDto {
  severity: Severity;
  path: string;
  message: string;
}

export interface EffectiveDto {
  method: string;
  url: string;
  headers: [string, string][];
  body: string | null;
}

export interface AuthStepDto {
  endpointId: string;
  request: EffectiveDto | null;
  /** `null` means a cache hit — render "from cache", never a status of 0. */
  status: number | null;
  body: string;
  fromCache: boolean;
}

export interface RunDto {
  status: number;
  elapsedMs: number;
  sizeBytes: number;
  headers: [string, string][];
  body: string;
  bodyIsBinary: boolean;
  /** `body` holds only the first slice of a very large response. */
  bodyTruncated: boolean;
  effective: EffectiveDto;
  authTrace: AuthStepDto[];
}

export interface HistoryEntry {
  at: string;
  status: number;
  elapsedMs: number;
  sizeBytes: number;
  method: string;
  url: string;
  requestBody?: string;
  responseBody?: string;
}

export const WORKSPACE_CHANGED_EVENT = "workspace-changed";

export async function loadWorkspace(): Promise<WorkspaceDto> {
  return invoke("load_workspace");
}

export async function lint(text: string): Promise<DiagnosticDto[]> {
  return invoke("lint", { text });
}

export async function saveApi(
  apiId: string,
  text: string,
): Promise<DiagnosticDto[]> {
  return invoke("save_api", { apiId, text });
}

/** Deletes the file `apiId` was loaded from. Rejects if `apiId` is unknown. */
export async function deleteApi(apiId: string): Promise<void> {
  return invoke("delete_api", { apiId });
}

export async function runEndpoint(
  apiId: string,
  endpointId: string,
  env: string | null,
): Promise<RunDto> {
  return invoke("run_endpoint", { apiId, endpointId, env });
}

export async function previewEndpoint(
  apiId: string,
  endpointId: string,
  env: string | null,
): Promise<EffectiveDto> {
  return invoke("preview_endpoint", { apiId, endpointId, env });
}

export async function curlCommand(
  apiId: string,
  endpointId: string,
  env: string | null,
): Promise<string> {
  return invoke("curl_command", { apiId, endpointId, env });
}

export async function history(
  apiId: string,
  endpointId: string,
): Promise<HistoryEntry[]> {
  return invoke("history", { apiId, endpointId });
}
