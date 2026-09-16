// apps/desktop/src-tauri/tests/run.rs — against wiremock, no network
use reqchain_core::paths::Paths;
use reqchain_desktop::{commands, state::AppState};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tok-abcdef-1234567890";

/// Workspace with a `token` endpoint and a `biz` endpoint chained onto it.
fn state_for(base_url: &str, extra: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    let text = format!(
        r#"{{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"{base_url}"{extra},
        "endpoints":[
          {{"id":"token","name":"Token","method":"POST","path":"/token"}},
          {{"id":"biz","name":"Biz","method":"GET","path":"/biz",
            "auth":{{"type":"chained","source":{{"endpoint":"token"}},
              "inject":{{"into":"header","name":"Authorization","template":"Bearer {{{{value}}}}"}}}}}}]}}"#
    );
    std::fs::write(paths.apis_dir().join("demo.json"), text).unwrap();
    (dir, AppState::new(paths))
}

/// Workspace with a single non-chained endpoint that echoes a secret into a
/// static header — used to prove secrets are masked outside the auth-chain
/// path too (e.g. in the curl export).
fn state_with_secret_header(base_url: &str, secret_value: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    let text = format!(
        r#"{{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"{base_url}",
        "endpoints":[
          {{"id":"secure","name":"Secure","method":"GET","path":"/secure",
            "headers":{{"X-Secret":"{{{{secret:PASS}}}}"}}}}]}}"#
    );
    std::fs::write(paths.apis_dir().join("demo.json"), text).unwrap();
    std::fs::write(
        paths.secrets_file(),
        serde_json::json!({ "PASS": secret_value }).to_string(),
    )
    .unwrap();
    (dir, AppState::new(paths))
}

#[tokio::test]
async fn a_chained_run_returns_the_business_response_and_an_auth_trace() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    let run = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    assert_eq!(run.status, 200);
    assert_eq!(run.body, "ok");
    assert_eq!(run.auth_trace.len(), 1);
    assert_eq!(run.auth_trace[0].endpoint_id, "token");
    assert!(!run.auth_trace[0].from_cache);
}

#[tokio::test]
async fn the_token_never_appears_unmasked_anywhere_in_the_run_dto() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    let run = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    // effective (business) request headers
    for [_, v] in &run.effective.headers {
        assert!(!v.contains(TOKEN), "token leaked in effective header: {v}");
    }
    assert!(!run.effective.url.contains(TOKEN));
    if let Some(b) = &run.effective.body {
        assert!(!b.contains(TOKEN));
    }

    // auth-trace response body (holds the token verbatim in the raw response)
    assert!(!run.auth_trace[0].body.contains(TOKEN));

    // auth-trace request headers (the token request itself, e.g. any echoed value)
    if let Some(req) = &run.auth_trace[0].request {
        for [_, v] in &req.headers {
            assert!(!v.contains(TOKEN), "token leaked in auth-trace header: {v}");
        }
        assert!(!req.url.contains(TOKEN));
    }

    // belt-and-braces: the token must not appear anywhere in the serialized DTO,
    // so a field we forgot to check above still fails this test.
    let serialized = serde_json::to_string(&run).unwrap();
    assert!(
        !serialized.contains(TOKEN),
        "token leaked somewhere in the serialized RunDto: {serialized}"
    );
}

#[tokio::test]
async fn a_secret_never_appears_unmasked_in_the_curl_command() {
    const SECRET: &str = "supersecretvalue1234567890";
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/secure"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&mock)
        .await;

    let (_d, state) = state_with_secret_header(&mock.uri(), SECRET);
    let curl = commands::curl_command_inner(&state, "demo", "secure", None)
        .await
        .unwrap();

    assert!(
        !curl.contains(SECRET),
        "secret leaked in curl command: {curl}"
    );
    assert!(curl.contains("***"));
}

#[tokio::test]
async fn preview_endpoint_does_not_send_the_business_request() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(0)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    let preview = commands::preview_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    assert_eq!(preview.method, "GET");
    assert!(preview.url.ends_with("/biz"));
}

#[tokio::test]
async fn a_second_run_reuses_the_cached_token() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(2)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    let first = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();
    let second = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    assert!(!first.auth_trace[0].from_cache);
    assert!(second.auth_trace[0].from_cache);
}

#[tokio::test]
async fn a_run_appends_one_history_entry() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    assert_eq!(commands::history_inner(&state, "demo", "biz").len(), 1);
}

#[tokio::test]
async fn store_bodies_false_keeps_bodies_out_of_the_history_file() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), r#","history":{"storeBodies":false}"#);
    commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    let entries = commands::history_inner(&state, "demo", "biz");
    assert_eq!(entries.len(), 1);
    assert!(entries[0].request_body.is_none());
    assert!(entries[0].response_body.is_none());

    let raw =
        std::fs::read_to_string(state.paths.history_dir().join("demo").join("biz.jsonl")).unwrap();
    assert!(!raw.contains("requestBody"));
    assert!(!raw.contains("responseBody"));
}

#[tokio::test]
async fn a_transport_failure_becomes_a_readable_error_not_a_panic() {
    // Port 1 is a reserved, unroutable port — the connection is refused.
    let (_d, state) = state_for("http://127.0.0.1:1", "");
    let err = commands::run_endpoint_inner(&state, "demo", "token", None)
        .await
        .unwrap_err();
    assert!(!err.is_empty());
}
