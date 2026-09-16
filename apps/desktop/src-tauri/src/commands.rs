use crate::dto::{ApiDto, DiagnosticDto, FileErrorDto, WorkspaceDto};
use crate::state::AppState;
use reqchain_core::model::Api;
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
        Ok(text) => Some(ApiDto::from_api(api, text)),
        Err(e) => {
            errors.push(FileErrorDto {
                path: path.display().to_string(),
                message: format!("re-reading {}: {e}", path.display()),
            });
            None
        }
    }
}

pub fn load_workspace_inner(state: &AppState) -> WorkspaceDto {
    state.reload();
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
