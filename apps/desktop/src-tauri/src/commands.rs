use crate::dto::{ApiDto, DiagnosticDto, WorkspaceDto};
use crate::state::AppState;

pub fn load_workspace_inner(state: &AppState) -> WorkspaceDto {
    state.reload();
    let workspace = state.workspace.lock().unwrap();
    let apis = workspace
        .apis
        .iter()
        .map(|api| {
            // The workspace convention is `apis_dir()/<id>.json`; `Workspace::load`
            // itself discovers files by directory listing rather than by id, so a
            // file whose basename doesn't match its `id` would not round-trip
            // through this lookup. See task-4-report.md for details.
            let path = state.paths.apis_dir().join(format!("{}.json", api.id));
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            ApiDto::from_api(api, text)
        })
        .collect();
    let errors = workspace.errors.iter().map(Into::into).collect();
    WorkspaceDto { apis, errors }
}

pub fn lint_inner(text: &str) -> Vec<DiagnosticDto> {
    reqchain_core::validate::validate_text(text)
        .iter()
        .map(Into::into)
        .collect()
}

pub fn save_api_inner(
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
    let path = state.paths.apis_dir().join(format!("{api_id}.json"));
    std::fs::write(&path, api.to_json_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    state.reload();
    Ok(diags)
}

#[tauri::command]
pub fn load_workspace(state: tauri::State<AppState>) -> WorkspaceDto {
    load_workspace_inner(&state)
}

#[tauri::command]
pub fn lint(text: String) -> Vec<DiagnosticDto> {
    lint_inner(&text)
}

#[tauri::command]
pub fn save_api(
    state: tauri::State<AppState>,
    api_id: String,
    text: String,
) -> Result<Vec<DiagnosticDto>, String> {
    save_api_inner(&state, &api_id, &text)
}
