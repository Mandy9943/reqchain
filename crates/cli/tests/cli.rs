use std::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const GOOD: &str = include_str!("../../../tests/fixtures/ceibal-gateway-test.json");
const CHAIN: &str = include_str!("../../../tests/fixtures/chain.json");

fn bin() -> Command { Command::new(env!("CARGO_BIN_EXE_reqchain")) }

fn workspace_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let apis = dir.path().join("workspace/apis");
    std::fs::create_dir_all(&apis).unwrap();
    for (name, content) in files { std::fs::write(apis.join(name), content).unwrap() }
    dir
}

#[test]
fn list_prints_apis_and_endpoints() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let out = bin().arg("list").env("REQCHAIN_DIR", dir.path()).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("ceibal-gateway-test"));
    assert!(text.contains("consultaReparacion"));
}

#[test]
fn validate_succeeds_on_a_good_file() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let out = bin().arg("validate").env("REQCHAIN_DIR", dir.path()).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn validate_fails_with_an_actionable_message() {
    let bad = GOOD.replace("\"endpoint\": \"token\"", "\"endpoint\": \"ghost\"");
    let dir = workspace_with(&[("a.json", &bad)]);
    let out = bin().arg("validate").env("REQCHAIN_DIR", dir.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("ghost"), "got: {text}");
}

#[test]
fn validate_accepts_an_explicit_file_path() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let path = dir.path().join("workspace/apis/a.json");
    let out = bin().arg("validate").arg(&path).output().unwrap();
    assert!(out.status.success());
}

#[test]
fn unknown_endpoint_is_a_usage_error() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let out = bin()
        .args(["run", "ceibal-gateway-test", "ghost"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("ghost"), "got: {text}");
}

/// Carry-forward requirement B: the token derived by chained auth is a credential too.
/// `--print-command` output must mask it, not just the literal secrets from the store.
#[tokio::test]
async fn print_command_masks_the_derived_chain_token() {
    let server = MockServer::start().await;
    let issued_token = "issued-chain-token-abc123";
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": issued_token,
            "expires_in": 3600,
        })))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server).await;

    let dir = workspace_with(&[("a.json", &CHAIN.replace("BASE_URL", &server.uri()))]);
    std::fs::write(
        dir.path().join("secrets.json"),
        serde_json::json!({"GW_USER": "alice", "GW_PASS": "hunter2"}).to_string(),
    )
    .unwrap();

    let out = bin()
        .args(["run", "chain", "business", "--env", "test", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stdout.contains(issued_token), "derived token leaked in stdout: {stdout}");
    assert!(!stderr.contains(issued_token), "derived token leaked in stderr: {stderr}");
    assert!(stdout.contains("Bearer ***"), "expected masked bearer token, got: {stdout}");
}
