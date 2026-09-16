//! Regression tests for the findings of the final whole-branch review of phase 1.
//! Each test fails against the code as it stood before its fix.

use std::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_reqchain"))
}

fn workspace(api: &str, store: serde_json::Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let apis = dir.path().join("workspace/apis");
    std::fs::create_dir_all(&apis).unwrap();
    std::fs::write(apis.join("a.json"), api).unwrap();
    std::fs::write(dir.path().join("secrets.json"), store.to_string()).unwrap();
    dir
}

/// A one-API file with `base_url` and the given endpoint objects spliced in.
fn api_file(base_url: &str, endpoints: &str) -> String {
    format!(
        r#"{{
  "schemaVersion": 1,
  "id": "t",
  "name": "T",
  "baseUrl": "{base_url}",
  "variables": {{}},
  "endpoints": [{endpoints}]
}}"#
    )
}

// ---------------------------------------------------------------------------
// CRITICAL 1 — `--print-command` must not send the endpoint's own request.
// ---------------------------------------------------------------------------

/// The flag says it prints the request "instead of sending it", and the README
/// promises it works without network access. Before the fix the CLI called
/// `Executor::run()` — a real HTTP request — and only then printed, so running a
/// `DELETE` endpoint with `--print-command` really did delete.
#[tokio::test]
async fn print_command_does_not_send_the_endpoints_own_request() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/resource/42"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;

    let api = api_file(
        &server.uri(),
        r#"{
      "id": "destroy",
      "name": "Destroy",
      "method": "DELETE",
      "path": "/resource/42",
      "auth": { "type": "none" }
    }"#,
    );
    let dir = workspace(&api, serde_json::json!({}));

    let out = bin()
        .args(["run", "t", "destroy", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.starts_with("curl "), "got: {stdout}");
    assert!(stdout.contains("DELETE"), "got: {stdout}");

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "--print-command performed the request it claims to replace: {:?}",
        requests
            .iter()
            .map(|r| r.url.to_string())
            .collect::<Vec<_>>()
    );
}

/// The documented exception: resolving a chained auth still calls the AUTH
/// endpoint — the token does not exist until it has — but the endpoint's own
/// request is still never sent.
#[tokio::test]
async fn print_command_fetches_the_chain_token_but_never_sends_the_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tok-abc",
            "expires_in": 3600
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/resource/42"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;

    let api = api_file(&server.uri(), CHAINED_DELETE);
    let dir = workspace(&api, serde_json::json!({"PW": "hunter2"}));

    let out = bin()
        .args(["run", "t", "destroy", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let requests = server.received_requests().await.unwrap();
    let paths: Vec<String> = requests.iter().map(|r| r.url.path().to_string()).collect();
    assert_eq!(
        paths,
        vec!["/token".to_string()],
        "only the auth request may happen under --print-command"
    );
}

const CHAINED_DELETE: &str = r#"{
      "id": "token",
      "name": "Token",
      "method": "POST",
      "path": "/token",
      "auth": { "type": "bearer", "token": "{{secret:PW}}" }
    },
    {
      "id": "destroy",
      "name": "Destroy",
      "method": "DELETE",
      "path": "/resource/42",
      "auth": {
        "type": "chained",
        "source": { "endpoint": "token" },
        "extract": { "from": "body", "jsonPath": "$.access_token" },
        "ttl": { "from": "fixed", "seconds": 3600 },
        "inject": { "into": "header", "name": "Authorization", "template": "Bearer {{value}}" }
      }
    }"#;

// ---------------------------------------------------------------------------
// CRITICAL 2 — a value interpolated into the URL is stored percent-encoded, so
// `masked()`'s literal-substring match never found it.
// ---------------------------------------------------------------------------

const URL_UNSAFE_SECRET: &str = "p@ss/w+rd=";
const URL_UNSAFE_SECRET_ENCODED: &str = "p%40ss%2Fw%2Brd%3D";

#[tokio::test]
async fn print_command_masks_a_url_unsafe_secret_in_a_query_parameter() {
    let server = MockServer::start().await;
    let api = api_file(
        &server.uri(),
        r#"{
      "id": "search",
      "name": "Search",
      "method": "GET",
      "path": "/search",
      "query": { "key": "{{secret:PW}}" },
      "auth": { "type": "none" }
    }"#,
    );
    let dir = workspace(&api, serde_json::json!({ "PW": URL_UNSAFE_SECRET }));

    let out = bin()
        .args(["run", "t", "search", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("curl "), "got: {stdout}");
    assert!(
        !stdout.contains(URL_UNSAFE_SECRET),
        "raw secret leaked: {stdout}"
    );
    assert!(
        !stdout.contains(URL_UNSAFE_SECRET_ENCODED),
        "percent-encoded secret leaked: {stdout}"
    );
    assert!(
        stdout.contains("key=***"),
        "expected the query value redacted, got: {stdout}"
    );
}

#[tokio::test]
async fn print_command_masks_a_url_unsafe_chain_token_injected_into_the_query() {
    let server = MockServer::start().await;
    let issued = "abc+def/ghi=";
    let issued_encoded = "abc%2Bdef%2Fghi%3D";
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": issued,
            "expires_in": 3600
        })))
        .mount(&server)
        .await;

    let api = api_file(
        &server.uri(),
        r#"{
      "id": "token",
      "name": "Token",
      "method": "POST",
      "path": "/token",
      "auth": { "type": "none" }
    },
    {
      "id": "search",
      "name": "Search",
      "method": "GET",
      "path": "/search",
      "auth": {
        "type": "chained",
        "source": { "endpoint": "token" },
        "extract": { "from": "body", "jsonPath": "$.access_token" },
        "ttl": { "from": "fixed", "seconds": 3600 },
        "inject": { "into": "query", "name": "access_token", "template": "{{value}}" }
      }
    }"#,
    );
    let dir = workspace(&api, serde_json::json!({}));

    let out = bin()
        .args(["run", "t", "search", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("curl "), "got: {stdout}");
    assert!(!stdout.contains(issued), "raw token leaked: {stdout}");
    assert!(
        !stdout.contains(issued_encoded),
        "percent-encoded token leaked: {stdout}"
    );
    assert!(
        stdout.contains("access_token=***"),
        "expected the injected token redacted, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// CRITICAL 3 — a credential DERIVED from a secret contains no literal secret,
// and only a header literally named `Authorization` was redacted.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn print_command_masks_a_computed_header_built_from_a_secret() {
    let server = MockServer::start().await;
    let secret = "s3cr3t-pw";
    // base64("user:" + secret) — the value that goes on the wire. It contains no
    // literal substring of the secret, so only reporting it from `apply_static`
    // can get it into the mask list.
    let expected = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(format!("user:{secret}"))
    };

    let api = api_file(
        &server.uri(),
        r#"{
      "id": "e",
      "name": "E",
      "method": "GET",
      "path": "/x",
      "auth": {
        "type": "computed",
        "name": "X-Api-Auth",
        "expression": "base64(\"user:\" + {{secret:PW}})"
      }
    }"#,
    );
    let dir = workspace(&api, serde_json::json!({ "PW": secret }));

    let out = bin()
        .args(["run", "t", "e", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("curl "), "got: {stdout}");
    assert!(
        stdout.contains("X-Api-Auth"),
        "sanity check — the computed header must be present: {stdout}"
    );
    assert!(
        !stdout.contains(&expected),
        "the computed credential was printed in full: {stdout}"
    );
    assert!(
        stdout.contains("X-Api-Auth: ***"),
        "expected the computed credential redacted, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// IMPORTANT 4 — the cache key hashed the UNRESOLVED auth definition, so
// rotating a secret reused the stale token until it expired.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rotating_a_secret_refetches_the_token_instead_of_reusing_the_cached_one() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tok",
            "expires_in": 3600
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/resource/42"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let api = api_file(&server.uri(), CHAINED_DELETE);
    let dir = workspace(&api, serde_json::json!({"PW": "old-password"}));
    let store = dir.path().join("secrets.json");

    let run = || {
        bin()
            .args(["run", "t", "destroy"])
            .env("REQCHAIN_DIR", dir.path())
            .output()
            .unwrap()
    };

    let first = run();
    assert!(
        first.status.success(),
        "run 1: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    // Control: the same credential must hit the cache, or this test would pass
    // for the wrong reason.
    let second = run();
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("auth: token -> (cached)"),
        "run 2 with an unchanged secret must reuse the cached token, got: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    // Rotate the credential. The auth DEFINITION is byte-identical — only the
    // value `{{secret:PW}}` resolves to has changed.
    std::fs::write(
        &store,
        serde_json::json!({"PW": "new-password"}).to_string(),
    )
    .unwrap();

    let third = run();
    let stderr = String::from_utf8_lossy(&third.stderr);
    assert!(third.status.success(), "run 3: {stderr}");
    assert!(
        stderr.contains("auth: token -> 200"),
        "a rotated credential must invalidate the cached token, got: {stderr}"
    );

    let requests = server.received_requests().await.unwrap();
    let token_calls = requests.iter().filter(|r| r.url.path() == "/token").count();
    assert_eq!(
        token_calls, 2,
        "expected one fetch for each distinct credential"
    );
}

// ---------------------------------------------------------------------------
// IMPORTANT 6 — exit code 2 (transport failure) had no test.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_transport_failure_exits_2() {
    // Bind and immediately drop, so the port is (almost certainly) closed but
    // was free a moment ago — a connection there is refused, not routed.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };

    let api = api_file(
        &format!("http://127.0.0.1:{port}"),
        r#"{
      "id": "e",
      "name": "E",
      "method": "GET",
      "path": "/x",
      "auth": { "type": "none" }
    }"#,
    );
    let dir = workspace(&api, serde_json::json!({}));

    let out = bin()
        .args(["run", "t", "e"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a transport failure must exit 2; stderr: {stderr}"
    );
    assert!(stderr.contains("transport error"), "got: {stderr}");
}

// ---------------------------------------------------------------------------
// Regression from the finding-3 fix (display-only): feeding EVERY static auth
// value into the GLOBAL substring mask destroyed the printed command, because
// masking is unanchored and had no minimum length. `X-Api-Version: 1` turned
// every `1` in the URL and in unrelated headers into `***`. Static auth values
// are now redacted by header NAME instead.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ordinary_header_auth_values_do_not_corrupt_the_printed_command() {
    let server = MockServer::start().await;
    let api = api_file(
        &server.uri(),
        r#"{
      "id": "items",
      "name": "Items",
      "method": "GET",
      "path": "/v1/items/1",
      "query": { "page": "1" },
      "headers": { "X-Note": "version 1 of public-client-id" },
      "auth": {
        "type": "header",
        "headers": { "X-Api-Version": "1", "X-Client": "public-client-id" }
      }
    }"#,
    );
    let dir = workspace(&api, serde_json::json!({}));

    let out = bin()
        .args(["run", "t", "items", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("curl "), "got: {stdout}");

    // The URL — host, port, path and query — must survive intact.
    assert!(
        stdout.contains(&format!("'{}/v1/items/1?page=1'", server.uri())),
        "the URL was corrupted by auth masking: {stdout}"
    );
    // An unrelated header that merely happens to contain the same characters
    // must survive intact too.
    assert!(
        stdout.contains("X-Note: version 1 of public-client-id"),
        "an unrelated header was corrupted by auth masking: {stdout}"
    );
    // Only the two auth header values are redacted.
    assert!(
        stdout.contains("X-Api-Version: ***"),
        "the auth header value must be redacted: {stdout}"
    );
    assert!(
        stdout.contains("X-Client: ***"),
        "the auth header value must be redacted: {stdout}"
    );
    assert_eq!(
        stdout.matches("***").count(),
        2,
        "exactly the two auth header values, nothing else: {stdout}"
    );
}
