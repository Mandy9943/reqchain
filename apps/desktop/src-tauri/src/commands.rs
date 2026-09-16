use crate::dto::{ApiDto, DiagnosticDto, EffectiveDto, FileErrorDto, RunDto, WorkspaceDto};
use crate::state::AppState;
use reqchain_core::chain::Executor;
use reqchain_core::history::{self, HistoryEntry};
use reqchain_core::model::Api;
use reqchain_core::shell::to_shell_command;
use reqchain_core::store::Workspace;
use std::path::Path;

/// Builds the DTO for one already-parsed `Api`, re-reading its raw text from
/// the file `workspace` recorded it as having been loaded from (never a name
/// derived from the id — see `store::Workspace::path_of`).
///
/// The re-read can fail even though `Workspace::load` just parsed this exact
/// API successfully moments earlier — the file can be deleted, permission
/// changed, or otherwise become unreadable in the window between the two
/// reads. That failure must not be swallowed into an empty editor buffer, so
/// it is reported as a `FileErrorDto` instead and the API is omitted from the
/// result (its content cannot be shown or safely edited without the source
/// text).
fn dto_for_api(workspace: &Workspace, api: &Api, errors: &mut Vec<FileErrorDto>) -> Option<ApiDto> {
    let Some(path) = workspace.path_of(&api.id) else {
        errors.push(FileErrorDto {
            path: format!("<no source recorded for `{}`>", api.id),
            message: "internal error: this API has no known source file".to_string(),
        });
        return None;
    };
    match std::fs::read_to_string(path) {
        Ok(text) => Some(ApiDto::from_api(api, text, path.display().to_string())),
        Err(e) => {
            errors.push(FileErrorDto {
                path: path.display().to_string(),
                message: format!("re-reading {}: {e}", path.display()),
            });
            None
        }
    }
}

pub async fn load_workspace_inner(state: &AppState) -> WorkspaceDto {
    state.reload().await;
    let workspace = state.workspace.lock().unwrap();
    let mut errors: Vec<FileErrorDto> = workspace.errors.iter().map(Into::into).collect();
    let apis = workspace
        .apis
        .iter()
        .filter_map(|api| dto_for_api(&workspace, api, &mut errors))
        .collect();
    WorkspaceDto { apis, errors }
}

pub fn lint_inner(text: &str) -> Vec<DiagnosticDto> {
    reqchain_core::validate::validate_text(text)
        .iter()
        .map(Into::into)
        .collect()
}

/// Writes `contents` to `path` atomically: to a temp file in the same
/// directory, then renamed over the target, so a crash or a full disk never
/// leaves a half-written or truncated workspace file. Mirrors
/// `history::append`.
fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension(format!(
        "{}.tmp.{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or("json"),
        std::process::id()
    ));
    std::fs::write(&tmp_path, contents)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

pub async fn save_api_inner(
    state: &AppState,
    api_id: &str,
    text: &str,
) -> Result<Vec<DiagnosticDto>, String> {
    let diags: Vec<DiagnosticDto> = lint_inner(text);
    if diags.iter().any(|d| d.severity == "error") {
        return Ok(diags); // reported, nothing written
    }
    let api = reqchain_core::model::Api::from_json(text).map_err(|e| e.to_string())?;
    if api.id != api_id {
        return Err(format!(
            "this file holds API `{api_id}`; the edited document has id `{}`. Rename via the file, not the editor.",
            api.id
        ));
    }
    // Write back to the file this API was loaded from, never to a name
    // derived from the id — a file's basename need not match its `id`. Only
    // fall back to the `<id>.json` convention when the workspace has no
    // record of a source file yet (e.g. a brand new API being saved for the
    // first time).
    let path = state
        .workspace
        .lock()
        .unwrap()
        .path_of(api_id)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| state.paths.apis_dir().join(format!("{api_id}.json")));
    write_atomically(&path, &api.to_json_string())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    state.reload().await;
    Ok(diags)
}

/// Deletes the file `api_id` was loaded from and reloads the workspace.
///
/// Resolves the path through `Workspace::path_of` — never
/// `apis_dir().join(format!("{id}.json"))` — because a file's basename need
/// not match the `id` inside it (commit bdda08f). Using the naming
/// convention here would silently remove (or fail to remove) the wrong
/// file whenever they differ.
pub async fn delete_api_inner(state: &AppState, api_id: &str) -> Result<(), String> {
    let path = state
        .workspace
        .lock()
        .unwrap()
        .path_of(api_id)
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("API `{api_id}` not found"))?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    state.reload().await;
    Ok(())
}

fn find_api(state: &AppState, api_id: &str) -> Result<Api, String> {
    state
        .workspace
        .lock()
        .unwrap()
        .api(api_id)
        .cloned()
        .ok_or_else(|| format!("API `{api_id}` not found"))
}

/// The mask for anything shaped like a REQUEST we built — the effective
/// request, the curl export, each auth-trace request: the executor's
/// chain-derived tokens plus every secret value. Static auth credential
/// values (a Basic blob, a `computed` header) are deliberately excluded —
/// they are masked in the request by header NAME instead
/// (`executor.auth_headers()`), and folding one into a global substring list
/// would corrupt unrelated request output (the phase 1 regression this
/// design avoids repeating). See `response_mask` for the broader list used
/// on anything a server sent back.
fn request_mask(state: &AppState, executor: &Executor) -> Vec<String> {
    let mut mask = executor.derived_values();
    mask.extend(state.secrets.lock().unwrap().values().cloned());
    mask
}

/// The mask for anything a SERVER sent back — the response body, response
/// headers, each auth-trace body, and what gets written to history:
/// `request_mask` extended with `executor.static_values()`, since a server
/// can echo a static auth credential verbatim in its response even though
/// that value never joins the request-display mask.
fn response_mask(state: &AppState, executor: &Executor) -> Vec<String> {
    let mut mask = request_mask(state, executor);
    mask.extend(executor.static_values());
    mask
}

/// Redacts an error string with whatever this executor knows is sensitive
/// AT THE TIME OF THE FAILURE — computed after the failing call returns, not
/// before, so a token the chain had already derived (and any static
/// credential it had already applied) before the failure is included. Two
/// concrete leaks this closes: `chain::RunError::Chain` can embed up to 200
/// raw characters of an auth endpoint's response body (which routinely
/// echoes back a rejected credential), and `exec::ExecError::Transport`'s
/// message is built from `reqwest`'s `Display`, which appends the request
/// URL — carrying a query-injected chain token in full.
fn redact_error(state: &AppState, executor: &Executor, error: impl std::fmt::Display) -> String {
    let mask = response_mask(state, executor);
    reqchain_core::request::redact_display(&error.to_string(), &mask)
}

pub async fn run_endpoint_inner(
    state: &AppState,
    api_id: &str,
    endpoint_id: &str,
    env: Option<String>,
) -> Result<RunDto, String> {
    let api = find_api(state, api_id)?;
    let mut executor = state.executor.lock().await;
    let result = match executor.run(&api, endpoint_id, env.as_deref()).await {
        Ok(result) => result,
        Err(e) => return Err(redact_error(state, &executor, e)),
    };

    let request_mask = request_mask(state, &executor);
    let response_mask = response_mask(state, &executor);
    let auth_headers = executor.auth_headers();
    let dto = RunDto::from_result(&result, &request_mask, &response_mask, &auth_headers);

    let masked_effective = result.effective.masked_with(&request_mask, &auth_headers);
    let store_bodies = api.history.as_ref().map(|h| h.store_bodies).unwrap_or(true);
    let entry = history::entry_from(&result, &masked_effective, &response_mask, store_bodies);
    // A failed history append is a non-fatal warning: it must never hide a
    // response the user is about to see behind an error instead.
    if let Err(e) = history::append(&state.paths, api_id, endpoint_id, &entry) {
        eprintln!("warning: failed to append history for {api_id}/{endpoint_id}: {e}");
    }

    Ok(dto)
}

pub async fn preview_endpoint_inner(
    state: &AppState,
    api_id: &str,
    endpoint_id: &str,
    env: Option<String>,
) -> Result<EffectiveDto, String> {
    let api = find_api(state, api_id)?;
    let mut executor = state.executor.lock().await;
    // `preview`, never `prepare`. The frontend fires this by itself whenever the
    // selection or the environment changes, so `prepare`'s chain resolution would
    // POST to a real token endpoint just because the user clicked around the
    // sidebar — a live request nobody asked for, invisible in the UI. `preview`
    // renders the chained value as a placeholder and performs no I/O.
    let req = match executor.preview(&api, endpoint_id, env.as_deref()).await {
        Ok(req) => req,
        Err(e) => return Err(redact_error(state, &executor, e)),
    };
    let mask = request_mask(state, &executor);
    let auth_headers = executor.auth_headers();
    Ok(EffectiveDto::from_masked(
        &req.masked_with(&mask, &auth_headers),
    ))
}

pub async fn curl_command_inner(
    state: &AppState,
    api_id: &str,
    endpoint_id: &str,
    env: Option<String>,
) -> Result<String, String> {
    let api = find_api(state, api_id)?;
    let mut executor = state.executor.lock().await;
    // `prepare`, not `preview`: a curl export is an explicit user action and must
    // carry the real effective request, chain resolved (spec §8).
    let req = match executor.prepare(&api, endpoint_id, env.as_deref()).await {
        Ok(req) => req,
        Err(e) => return Err(redact_error(state, &executor, e)),
    };
    let mask = request_mask(state, &executor);
    let auth_headers = executor.auth_headers();
    let masked = req.masked_with(&mask, &auth_headers);
    Ok(to_shell_command(&masked))
}

pub fn history_inner(state: &AppState, api_id: &str, endpoint_id: &str) -> Vec<HistoryEntry> {
    history::load(&state.paths, api_id, endpoint_id)
}

#[tauri::command]
pub async fn run_endpoint(
    state: tauri::State<'_, AppState>,
    api_id: String,
    endpoint_id: String,
    env: Option<String>,
) -> Result<RunDto, String> {
    run_endpoint_inner(&state, &api_id, &endpoint_id, env).await
}

#[tauri::command]
pub async fn preview_endpoint(
    state: tauri::State<'_, AppState>,
    api_id: String,
    endpoint_id: String,
    env: Option<String>,
) -> Result<EffectiveDto, String> {
    preview_endpoint_inner(&state, &api_id, &endpoint_id, env).await
}

#[tauri::command]
pub async fn curl_command(
    state: tauri::State<'_, AppState>,
    api_id: String,
    endpoint_id: String,
    env: Option<String>,
) -> Result<String, String> {
    curl_command_inner(&state, &api_id, &endpoint_id, env).await
}

#[tauri::command]
pub fn history(
    state: tauri::State<AppState>,
    api_id: String,
    endpoint_id: String,
) -> Vec<HistoryEntry> {
    history_inner(&state, &api_id, &endpoint_id)
}

#[tauri::command]
pub async fn load_workspace(state: tauri::State<'_, AppState>) -> Result<WorkspaceDto, String> {
    Ok(load_workspace_inner(&state).await)
}

#[tauri::command]
pub fn lint(text: String) -> Vec<DiagnosticDto> {
    lint_inner(&text)
}

#[tauri::command]
pub async fn save_api(
    state: tauri::State<'_, AppState>,
    api_id: String,
    text: String,
) -> Result<Vec<DiagnosticDto>, String> {
    save_api_inner(&state, &api_id, &text).await
}

#[tauri::command]
pub async fn delete_api(state: tauri::State<'_, AppState>, api_id: String) -> Result<(), String> {
    delete_api_inner(&state, &api_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqchain_core::paths::Paths;

    const GOOD: &str = r#"{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"https://api.example.com","endpoints":[{"id":"ping","name":"Ping","method":"GET","path":"/ping"}]}"#;

    /// Reproduces the window `dto_for_api` guards: `Workspace::load` parsed
    /// this API successfully (it's a legitimate entry in `workspace.apis`,
    /// with its source correctly recorded), but by the time the raw text is
    /// re-read, the file is gone. That must surface as a `FileErrorDto`, not
    /// an `ApiDto` with silently empty text.
    #[test]
    fn dto_for_api_reports_a_read_failure_instead_of_defaulting_to_empty_text() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        std::fs::create_dir_all(paths.apis_dir()).unwrap();
        std::fs::write(paths.apis_dir().join("demo.json"), GOOD).unwrap();
        let workspace = Workspace::load(&paths);
        assert_eq!(workspace.apis.len(), 1);

        // The file vanishes after Workspace::load already parsed it.
        std::fs::remove_file(paths.apis_dir().join("demo.json")).unwrap();

        let mut errors = Vec::new();
        let dto = dto_for_api(&workspace, &workspace.apis[0], &mut errors);
        assert!(dto.is_none(), "must not fabricate a DTO with empty text");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.ends_with("demo.json"));
    }
}
