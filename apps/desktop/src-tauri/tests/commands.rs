use reqchain_core::paths::Paths;
use reqchain_desktop::commands;
use reqchain_desktop::state::AppState;

const GOOD: &str = r#"{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"https://api.example.com","endpoints":[{"id":"ping","name":"Ping","method":"GET","path":"/ping"}]}"#;

/// A chained auth that OMITS `extract`, `ttl` AND `retryOn` entirely —
/// exactly the shape `reqchain-visual-editing` task 5's report initially
/// (and wrongly) implied the frontend's byte-for-byte fixture match
/// generalized to. It does not: this is `tests/fixtures/chain.json`'s own
/// shape (minus `extract`/`ttl`, to isolate `retryOn` specifically).
const CHAINED_NO_RETRY: &str = r#"{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"https://api.example.com","endpoints":[{"id":"token","name":"Token","method":"POST","path":"/token","auth":{"type":"none"}},{"id":"biz","name":"Biz","method":"GET","path":"/biz","auth":{"type":"chained","source":{"endpoint":"token"},"inject":{"into":"header","name":"Authorization","template":"Bearer {{value}}"}}}]}"#;

fn state_with(file: &str, text: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    std::fs::write(paths.apis_dir().join(file), text).unwrap();
    let state = AppState::new(paths);
    (dir, state)
}

fn state_with_secrets(entries: &[(&str, &str)]) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let map: std::collections::BTreeMap<String, String> = entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    std::fs::write(paths.secrets_file(), serde_json::to_string(&map).unwrap()).unwrap();
    let state = AppState::new(paths);
    (dir, state)
}

#[tokio::test]
async fn load_workspace_lists_apis_and_endpoints() {
    let (_d, state) = state_with("demo.json", GOOD);
    let ws = commands::load_workspace_inner(&state).await;
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.apis[0].id, "demo");
    assert_eq!(ws.apis[0].endpoints[0].id, "ping");
    assert!(ws.errors.is_empty());
}

#[tokio::test]
async fn an_invalid_file_becomes_an_error_and_does_not_hide_the_valid_ones() {
    let (_d, state) = state_with("demo.json", GOOD);
    std::fs::write(state.paths.apis_dir().join("broken.json"), "{ nope").unwrap();
    let ws = commands::load_workspace_inner(&state).await;
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.errors.len(), 1);
    assert!(ws.errors[0].path.ends_with("broken.json"));
}

#[tokio::test]
async fn save_api_rejects_an_invalid_document_and_leaves_the_file_untouched() {
    let (_d, state) = state_with("demo.json", GOOD);
    let before = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    let diags = commands::save_api_inner(&state, "demo", "{ nope")
        .await
        .unwrap();
    assert!(diags.iter().any(|d| d.severity == "error"));
    let after = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn save_api_rejects_a_document_whose_id_does_not_match_the_file() {
    let (_d, state) = state_with("demo.json", GOOD);
    let renamed = GOOD.replace(r#""id":"demo""#, r#""id":"other""#);
    let err = commands::save_api_inner(&state, "demo", &renamed)
        .await
        .unwrap_err();
    assert!(err.contains("id"), "unhelpful error: {err}");
}

#[tokio::test]
async fn save_api_writes_canonical_json() {
    let (_d, state) = state_with("demo.json", GOOD);
    let diags = commands::save_api_inner(&state, "demo", GOOD)
        .await
        .unwrap();
    assert!(diags.iter().all(|d| d.severity != "error"));
    let written = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    assert!(written.ends_with('\n'));
    assert!(written.contains("\n  \"id\": \"demo\""));
}

#[tokio::test]
async fn load_workspace_and_save_api_use_the_file_the_api_was_loaded_from_not_the_id() {
    let (_d, state) = state_with("gateway.json", GOOD);
    let ws = commands::load_workspace_inner(&state).await;
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.apis[0].id, "demo");
    assert_eq!(
        ws.apis[0].text, GOOD,
        "must return the real file text, not empty"
    );

    let diags = commands::save_api_inner(&state, "demo", GOOD)
        .await
        .unwrap();
    assert!(diags.iter().all(|d| d.severity != "error"));
    let gateway_path = state.paths.apis_dir().join("gateway.json");
    let wrong_path = state.paths.apis_dir().join("demo.json");
    assert!(gateway_path.exists(), "must save back to gateway.json");
    assert!(
        !wrong_path.exists(),
        "must not invent a demo.json alongside gateway.json"
    );
}

#[tokio::test]
async fn save_api_writes_atomically_via_temp_file_and_rename() {
    let (_d, state) = state_with("demo.json", GOOD);
    let diags = commands::save_api_inner(&state, "demo", GOOD)
        .await
        .unwrap();
    assert!(diags.iter().all(|d| d.severity != "error"));
    // No stray temp files left behind in the apis dir after a successful save.
    let leftover: Vec<_> = std::fs::read_dir(state.paths.apis_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(leftover.is_empty(), "left temp files: {leftover:?}");
}

#[tokio::test]
async fn delete_api_removes_the_file_it_was_loaded_from() {
    let (_d, state) = state_with("gateway.json", GOOD); // id is `demo`, name is not
    let path = state.paths.apis_dir().join("gateway.json");
    assert!(path.exists());
    commands::delete_api_inner(&state, "demo").await.unwrap();
    assert!(!path.exists(), "deleted the wrong file, or none");
    assert!(commands::load_workspace_inner(&state).await.apis.is_empty());
}

#[tokio::test]
async fn delete_api_refuses_an_unknown_id_instead_of_deleting_nothing_quietly() {
    let (_d, state) = state_with("demo.json", GOOD);
    let err = commands::delete_api_inner(&state, "nope")
        .await
        .unwrap_err();
    // Pins the MECHANISM (an explicit `path_of` miss), not just the
    // symptom of "the string `nope` shows up somewhere": a regression that
    // guessed a `nope.json` path and let `remove_file`'s NotFound error
    // surface would also produce a message containing "nope", so a bare
    // `contains("nope")` would not have caught it.
    assert_eq!(err, "API `nope` not found");
    assert!(state.paths.apis_dir().join("demo.json").exists());
}

/// The frontend derives an id from a user-typed name via `slugify`, but that
/// is a UI convenience, not a security boundary — a user-typed name is the
/// first input that reaches this path at all (every id before this task came
/// from a file already on disk), so Rust must refuse a hostile id itself
/// rather than trust the client.
#[tokio::test]
async fn save_api_rejects_a_path_traversal_id_and_writes_nothing_outside_the_workspace() {
    let (_d, state) = state_with("demo.json", GOOD);
    let evil_id = "../../etc/passwd";
    let evil = GOOD.replace(r#""id":"demo""#, &format!(r#""id":"{evil_id}""#));
    let err = commands::save_api_inner(&state, evil_id, &evil)
        .await
        .unwrap_err();
    assert!(err.contains(".."), "unhelpful error: {err}");
    // Nothing was written outside the workspace's apis dir — this is the
    // concrete failure a naive `apis_dir().join(format!("{id}.json"))`
    // fallback would produce for this id (`apis_dir/../../etc/passwd.json`
    // resolves outside the workspace entirely).
    assert!(!std::path::Path::new("/etc/passwd.json").exists());
}

#[tokio::test]
async fn save_api_rejects_an_empty_id() {
    let (_d, state) = state_with("demo.json", GOOD);
    let err = commands::save_api_inner(&state, "", GOOD)
        .await
        .unwrap_err();
    assert!(err.contains("empty"), "unhelpful error: {err}");
}

#[tokio::test]
async fn delete_api_rejects_a_path_traversal_id() {
    let (_d, state) = state_with("demo.json", GOOD);
    let err = commands::delete_api_inner(&state, "../../etc/passwd")
        .await
        .unwrap_err();
    assert!(err.contains(".."), "unhelpful error: {err}");
    assert!(state.paths.apis_dir().join("demo.json").exists());
}

/// The whole point of `list_secrets` returning names: a screen that lists
/// secrets must not ship their values to the webview to do it.
#[tokio::test]
async fn listing_secrets_never_returns_a_value() {
    let (_d, state) = state_with_secrets(&[("GW_PASS", "hunter2")]);
    let names = commands::list_secrets_inner(&state);
    assert_eq!(names, vec!["GW_PASS".to_string()]);
    let json = serde_json::to_string(&names).unwrap();
    assert!(!json.contains("hunter2"));
}

#[tokio::test]
async fn revealing_returns_exactly_one_value_and_only_by_name() {
    let (_d, state) = state_with_secrets(&[("A", "one"), ("B", "two")]);
    assert_eq!(commands::reveal_secret_inner(&state, "A").unwrap(), "one");
    assert!(commands::reveal_secret_inner(&state, "nope").is_err());
}

/// A secret set through the UI must be usable by a run immediately — the
/// executor holds its own `Secrets` snapshot, and the rotation fix in
/// `AppState::reload` covers exactly this class of staleness. Setting one
/// here goes through the same reload path, so a run against an endpoint that
/// references it by name must succeed against a live mock server that only
/// accepts the new value.
#[tokio::test]
async fn a_secret_set_through_the_command_is_visible_to_the_next_run() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/secure"))
        .and(header("x-secret", "brand-new-value"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    let text = format!(
        r#"{{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"{}",
        "endpoints":[
          {{"id":"secure","name":"Secure","method":"GET","path":"/secure",
            "headers":{{"X-Secret":"{{{{secret:PASS}}}}"}}}}]}}"#,
        mock.uri()
    );
    std::fs::write(paths.apis_dir().join("demo.json"), text).unwrap();
    let state = AppState::new(paths);

    commands::set_secret_inner(&state, "PASS", "brand-new-value")
        .await
        .unwrap();

    let run = commands::run_endpoint_inner(&state, "demo", "secure", None)
        .await
        .unwrap();
    assert_eq!(run.status, 200);
}

#[tokio::test]
async fn set_secret_rejects_an_empty_name() {
    let (_d, state) = state_with_secrets(&[]);
    let err = commands::set_secret_inner(&state, "", "v")
        .await
        .unwrap_err();
    assert!(err.contains("empty"));
}

#[tokio::test]
async fn delete_secret_reports_an_unknown_name_instead_of_succeeding_quietly() {
    let (_d, state) = state_with_secrets(&[("A", "1")]);
    let err = commands::delete_secret_inner(&state, "nope")
        .await
        .unwrap_err();
    assert!(err.contains("nope"));
    assert_eq!(commands::list_secrets_inner(&state), vec!["A".to_string()]);
}

#[tokio::test]
async fn delete_secret_removes_it_and_persists() {
    let (dir, state) = state_with_secrets(&[("A", "1"), ("B", "2")]);
    commands::delete_secret_inner(&state, "A").await.unwrap();
    assert_eq!(commands::list_secrets_inner(&state), vec!["B".to_string()]);

    // Persisted to disk, not just the in-memory snapshot.
    let paths = Paths::at(dir.path());
    let reloaded = reqchain_core::secrets::Secrets::load(&paths);
    assert_eq!(reloaded.names(), vec!["B".to_string()]);
}

#[tokio::test]
async fn set_secret_persists_to_disk_with_0600_permissions() {
    let (dir, state) = state_with_secrets(&[]);
    commands::set_secret_inner(&state, "A", "value")
        .await
        .unwrap();

    let paths = Paths::at(dir.path());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(paths.secrets_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let reloaded = reqchain_core::secrets::Secrets::load(&paths);
    assert_eq!(reloaded.get("A"), Some("value"));
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

/// An endpoint with `"auth": "inherit"` (or no `auth` at all) under an API whose
/// own auth is chained must report `chained`, so the sidebar shows the `chain`
/// marker. Reporting the DECLARED auth here hid the marker on exactly the
/// endpoints the product exists to make visible.
#[tokio::test]
async fn an_endpoint_inheriting_a_chained_api_auth_reports_chained() {
    let text = r#"{"schemaVersion":1,"id":"demo","name":"Demo",
      "baseUrl":"https://api.example.com",
      "auth":{"type":"chained","source":{"endpoint":"token"},
        "inject":{"into":"header","name":"Authorization","template":"Bearer {{value}}"}},
      "endpoints":[
        {"id":"token","name":"Token","method":"POST","path":"/token","auth":{"type":"none"}},
        {"id":"inherits","name":"Inherits","method":"GET","path":"/biz"},
        {"id":"explicit","name":"Explicit","method":"GET","path":"/open","auth":{"type":"none"}}]}"#;
    let (_d, state) = state_with("demo.json", text);
    let ws = commands::load_workspace_inner(&state).await;
    let endpoints = &ws.apis[0].endpoints;

    let kind = |id: &str| {
        endpoints
            .iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("no endpoint {id}"))
            .auth_kind
            .clone()
    };
    assert_eq!(kind("inherits"), "chained", "the marker must be shown");
    assert_eq!(kind("token"), "none");
    assert_eq!(kind("explicit"), "none", "an explicit override still wins");
}

/// Task 5's report claimed the chained-auth builder's output matches the
/// shipped `example-gateway-test.json` fixture byte-for-byte — true, but
/// that fixture happens to write `extract`/`ttl`/`retryOn` explicitly. This
/// pins the DIFFERENT, general fact the report's wording glossed over:
/// `retry_on` has `#[serde(default = "default_retry_on")]` with NO
/// `skip_serializing_if`, so `Api::to_json_string` ALWAYS writes it — a
/// document that omits `retryOn` gains it on its very first save. `extract`
/// and `ttl` (both `#[serde(default, skip_serializing_if =
/// "Option::is_none")]`) behave oppositely and stay omitted if omitted,
/// pinned here too so the asymmetry is explicit rather than assumed.
#[tokio::test]
async fn save_api_adds_the_default_retry_on_but_leaves_omitted_extract_ttl_omitted() {
    let (_d, state) = state_with("demo.json", CHAINED_NO_RETRY);
    let diags = commands::save_api_inner(&state, "demo", CHAINED_NO_RETRY)
        .await
        .unwrap();
    assert!(
        diags.iter().all(|d| d.severity != "error"),
        "unexpected errors: {diags:?}"
    );
    let written = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    assert!(
        written.contains(r#""retryOn": ["#),
        "retryOn must be ADDED on save even though the input omitted it — got:\n{written}"
    );
    assert!(
        written.contains("401") && written.contains("403"),
        "the added retryOn must be the documented default [401, 403] — got:\n{written}"
    );
    assert!(
        !written.contains("\"extract\""),
        "extract has skip_serializing_if and must stay omitted when the input omitted it — got:\n{written}"
    );
    assert!(
        !written.contains("\"ttl\""),
        "ttl has skip_serializing_if and must stay omitted when the input omitted it — got:\n{written}"
    );
}
