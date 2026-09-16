//! End-to-end acceptance gate for phase 1 (spec §14): shells out to the real
//! `reqchain` binary against a local mock server and proves the project's two
//! headline cases actually work, plus the persistence guarantees (token reuse
//! across processes, automatic refresh once expired) that make those cases
//! meaningful in practice rather than just in a single run.

use std::process::Command;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const GATEWAY: &str = include_str!("../../../tests/fixtures/ceibal-gateway-test.json");
const ODILO: &str = include_str!("../../../tests/fixtures/odilo.json");

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_reqchain"))
}

fn workspace(files: &[(&str, String)], store: serde_json::Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let apis = dir.path().join("workspace/apis");
    std::fs::create_dir_all(&apis).unwrap();
    for (name, content) in files {
        std::fs::write(apis.join(name), content).unwrap()
    }
    std::fs::write(dir.path().join("secrets.json"), store.to_string()).unwrap();
    dir
}

/// Spec §14 case 1: pressing "run" on `consultaReparacion` works WITHOUT the
/// `token` endpoint having been run by hand — the CLI fetches the token,
/// injects it as `Authorization: Bearer <token>`, and the auth step is
/// reported on stderr.
#[tokio::test]
async fn chained_endpoint_runs_without_calling_token_first() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("authorization", "Basic YWxpY2U6aHVudGVyMg=="))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "live-token",
            "expires_in": 3600
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/consultareparacion/1.0"))
        .and(header("authorization", "Bearer live-token"))
        .and(body_string_contains("12345678"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"Reparaciones": []})),
        )
        .mount(&server)
        .await;

    let api = GATEWAY.replace("https://api-manager.test-ceibal.edu.uy", &server.uri());
    let dir = workspace(
        &[("gateway.json", api)],
        serde_json::json!({"GW_USER": "alice", "GW_PASS": "hunter2"}),
    );

    let out = bin()
        .args([
            "run",
            "ceibal-gateway-test",
            "consultaReparacion",
            "--env",
            "test",
        ])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    assert!(stdout.contains("Reparaciones"), "stdout: {stdout}");
    // An UNCACHED auth step reports its numeric status (verified in Task 10);
    // a `(cached)` step never carries a status number.
    assert!(
        stderr.contains("auth: token -> 200"),
        "the auth request must be reported: {stderr}"
    );
}

/// Spec §14 case 2: `GET /odilo/user` with `user`, `token` (from the secret
/// store) and a computed `hash: md5(now("YYYYMMDD") + documento)` header.
#[tokio::test]
async fn computed_header_endpoint_runs() {
    let server = MockServer::start().await;
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let expected_hash = format!(
        "{:x}",
        md5_simple::compute(format!("{today}12345678").as_bytes())
    );
    Mock::given(method("GET"))
        .and(path("/odilo/user"))
        .and(header("user", "12345678"))
        .and(header("hash", expected_hash.as_str()))
        .and(header("token", "api-token-value"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"user": "ok"})))
        .mount(&server)
        .await;

    let api = ODILO.replace("https://api.test-ceibal.edu.uy", &server.uri());
    let dir = workspace(
        &[("odilo.json", api)],
        serde_json::json!({"API_TOKEN": "api-token-value"}),
    );

    let out = bin()
        .args(["run", "odilo", "odilo-user", "--env", "test"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `--print-command` (named after this brief was written; the flag is not
/// `--curl`) must still mask secret literals out of the printed command.
#[tokio::test]
async fn printed_command_hides_secret_values() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "live-token",
            "expires_in": 3600
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/consultareparacion/1.0"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = GATEWAY.replace("https://api-manager.test-ceibal.edu.uy", &server.uri());
    let dir = workspace(
        &[("gateway.json", api)],
        serde_json::json!({"GW_USER": "alice", "GW_PASS": "hunter2"}),
    );

    let out = bin()
        .args([
            "run",
            "ceibal-gateway-test",
            "consultaReparacion",
            "--env",
            "test",
            "--print-command",
        ])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("curl "), "got: {stdout}");
    assert!(
        !stdout.contains("hunter2"),
        "export must not leak secret values: {stdout}"
    );
}

/// The persistent token cache is what makes chained auth cheap: two
/// consecutive CLI *processes* sharing one `REQCHAIN_DIR` must hit `/token`
/// exactly once between them, the second run reusing the cached token.
#[tokio::test]
async fn token_is_reused_across_consecutive_invocations() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "live-token",
            "expires_in": 3600
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/consultareparacion/1.0"))
        .and(header("authorization", "Bearer live-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"Reparaciones": []})),
        )
        .mount(&server)
        .await;

    let api = GATEWAY.replace("https://api-manager.test-ceibal.edu.uy", &server.uri());
    let dir = workspace(
        &[("gateway.json", api)],
        serde_json::json!({"GW_USER": "alice", "GW_PASS": "hunter2"}),
    );

    for run_number in 1..=2 {
        let out = bin()
            .args([
                "run",
                "ceibal-gateway-test",
                "consultaReparacion",
                "--env",
                "test",
            ])
            .env("REQCHAIN_DIR", dir.path())
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "run {run_number}: stderr: {stderr}");
        if run_number == 1 {
            assert!(
                stderr.contains("auth: token -> 200"),
                "run 1: expected a fresh fetch, got: {stderr}"
            );
        } else {
            assert!(
                stderr.contains("auth: token -> (cached)"),
                "run 2: expected a cache hit, got: {stderr}"
            );
        }
    }

    // wiremock's `expect(1)` is verified on drop / server shutdown; make the
    // assertion explicit and immediate too.
    let requests = server.received_requests().await.unwrap();
    let token_calls = requests.iter().filter(|r| r.url.path() == "/token").count();
    assert_eq!(
        token_calls, 1,
        "the token endpoint must be called exactly once across both runs"
    );
}

/// An expired token must be refreshed automatically: the token endpoint
/// issues a token whose `expires_in` is small enough to already be expired
/// under the 30-second clock-skew guard (see `reqchain_core::cache::SKEW_SECONDS`),
/// so two consecutive runs must fetch it twice.
#[tokio::test]
async fn expired_token_is_refreshed_automatically() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "live-token",
            // Already inside the 30s skew window at the moment it's cached,
            // so `TokenCache::get` treats it as expired immediately.
            "expires_in": 1
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/consultareparacion/1.0"))
        .and(header("authorization", "Bearer live-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"Reparaciones": []})),
        )
        .mount(&server)
        .await;

    let api = GATEWAY.replace("https://api-manager.test-ceibal.edu.uy", &server.uri());
    let dir = workspace(
        &[("gateway.json", api)],
        serde_json::json!({"GW_USER": "alice", "GW_PASS": "hunter2"}),
    );

    for run_number in 1..=2 {
        let out = bin()
            .args([
                "run",
                "ceibal-gateway-test",
                "consultaReparacion",
                "--env",
                "test",
            ])
            .env("REQCHAIN_DIR", dir.path())
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "run {run_number}: stderr: {stderr}");
        assert!(
            stderr.contains("auth: token -> 200"),
            "run {run_number}: expected a fresh fetch, got: {stderr}"
        );
    }

    let requests = server.received_requests().await.unwrap();
    let token_calls = requests.iter().filter(|r| r.url.path() == "/token").count();
    assert_eq!(
        token_calls, 2,
        "an already-expired token must be re-fetched on every run"
    );
}
