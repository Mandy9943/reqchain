use reqchain_core::paths::Paths;
use reqchain_desktop::commands;
use reqchain_desktop::state::AppState;

const GOOD: &str = r#"{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"https://api.example.com","endpoints":[{"id":"ping","name":"Ping","method":"GET","path":"/ping"}]}"#;

fn state_with(file: &str, text: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    std::fs::write(paths.apis_dir().join(file), text).unwrap();
    let state = AppState::new(paths);
    (dir, state)
}

#[test]
fn load_workspace_lists_apis_and_endpoints() {
    let (_d, state) = state_with("demo.json", GOOD);
    let ws = commands::load_workspace_inner(&state);
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.apis[0].id, "demo");
    assert_eq!(ws.apis[0].endpoints[0].id, "ping");
    assert!(ws.errors.is_empty());
}

#[test]
fn an_invalid_file_becomes_an_error_and_does_not_hide_the_valid_ones() {
    let (_d, state) = state_with("demo.json", GOOD);
    std::fs::write(state.paths.apis_dir().join("broken.json"), "{ nope").unwrap();
    let ws = commands::load_workspace_inner(&state);
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.errors.len(), 1);
    assert!(ws.errors[0].path.ends_with("broken.json"));
}

#[test]
fn save_api_rejects_an_invalid_document_and_leaves_the_file_untouched() {
    let (_d, state) = state_with("demo.json", GOOD);
    let before = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    let diags = commands::save_api_inner(&state, "demo", "{ nope").unwrap();
    assert!(diags.iter().any(|d| d.severity == "error"));
    let after = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn save_api_rejects_a_document_whose_id_does_not_match_the_file() {
    let (_d, state) = state_with("demo.json", GOOD);
    let renamed = GOOD.replace(r#""id":"demo""#, r#""id":"other""#);
    let err = commands::save_api_inner(&state, "demo", &renamed).unwrap_err();
    assert!(err.contains("id"), "unhelpful error: {err}");
}

#[test]
fn save_api_writes_canonical_json() {
    let (_d, state) = state_with("demo.json", GOOD);
    let diags = commands::save_api_inner(&state, "demo", GOOD).unwrap();
    assert!(diags.iter().all(|d| d.severity != "error"));
    let written = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    assert!(written.ends_with('\n'));
    assert!(written.contains("\n  \"id\": \"demo\""));
}

#[test]
fn lint_reports_the_chained_default_warning() {
    let text = r#"{"schemaVersion":1,"id":"d","name":"D","baseUrl":"https://x","endpoints":[
      {"id":"token","name":"T","method":"POST","path":"/token"},
      {"id":"biz","name":"B","method":"GET","path":"/b","auth":{"type":"chained","source":{"endpoint":"token"},
       "inject":{"into":"header","name":"Authorization","template":"Bearer {{value}}"}}}]}"#;
    let diags = commands::lint_inner(text);
    assert!(diags.iter().any(|d| d.severity == "warning"));
}
