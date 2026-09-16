use reqchain_core::{cache::TokenCache, chain::Executor, exec::Runner, model::Api, secrets::Secrets};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const CHAIN: &str = include_str!("../../../tests/fixtures/chain.json");

fn api(base: &str, fixture: &str) -> Api {
    Api::from_json(&fixture.replace("BASE_URL", base)).unwrap()
}

/// Issues a new token on every call so a test can tell refreshes apart.
struct TokenIssuer { calls: Arc<AtomicUsize>, expires_in: u64 }

impl Respond for TokenIssuer {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": format!("token-{n}"),
            "expires_in": self.expires_in,
        }))
    }
}

fn executor() -> Executor {
    Executor::new(
        Runner::new(),
        TokenCache::in_memory(),
        Secrets::from_map([
            ("GW_USER".into(), "alice".into()),
            ("GW_PASS".into(), "hunter2".into()),
        ]),
    )
}

#[tokio::test]
async fn fetches_the_token_without_running_the_source_endpoint_by_hand() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .and(header("authorization", "Bearer token-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let res = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(res.auth_trace.len(), 1, "the auth request must be visible");
    assert_eq!(res.auth_trace[0].endpoint_id, "token");
    assert!(res.auth_trace[0].body.contains("access_token"));
}

#[tokio::test]
async fn reuses_the_cached_token_on_a_second_run() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    ex.run(&api, "business", Some("test")).await.unwrap();
    let second = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1, "token must be reused");
    assert!(second.auth_trace[0].from_cache);
}

#[tokio::test]
async fn refreshes_a_token_whose_ttl_has_passed() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    // expires_in below the 30s skew => already expired the moment it is cached
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 1 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    ex.run(&api, "business", Some("test")).await.unwrap();
    ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2, "expired token must be refetched");
}

#[tokio::test]
async fn retries_once_with_a_fresh_token_on_401() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business")).and(header("authorization", "Bearer token-1"))
        .respond_with(ResponseTemplate::new(401)).mount(&server).await;
    Mock::given(method("POST")).and(path("/business")).and(header("authorization", "Bearer token-2"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let res = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn gives_up_after_one_retry_on_persistent_401() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .respond_with(ResponseTemplate::new(401)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let res = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(res.status, 401, "the 401 is reported, not retried forever");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "exactly one refresh");
}

#[tokio::test]
async fn extracts_from_a_response_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).insert_header("x-token", "header-token"))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/header-business"))
        .and(header("authorization", "Bearer header-token"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(ex.run(&api, "header-business", Some("test")).await.unwrap().status, 200);
}

#[tokio::test]
async fn injects_into_a_query_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "qtok"})))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/query-business"))
        .and(query_param("access_token", "qtok"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(ex.run(&api, "query-business", Some("test")).await.unwrap().status, 200);
}

#[tokio::test]
async fn detects_a_cycle_and_names_the_path() {
    let api = api("https://example.test", include_str!("../../../tests/fixtures/chain-cycle.json"));
    let mut ex = executor();
    let err = ex.run(&api, "a", Some("test")).await.unwrap_err().to_string();
    assert!(err.contains("cycle"), "got: {err}");
    assert!(err.contains('a') && err.contains('b'), "error must name the path: {err}");
}

#[tokio::test]
async fn missing_source_endpoint_is_an_actionable_error() {
    let api = api("https://example.test", include_str!("../../../tests/fixtures/chain-missing.json"));
    let mut ex = executor();
    let err = ex.run(&api, "business", Some("test")).await.unwrap_err().to_string();
    assert!(err.contains("nonexistent"), "got: {err}");
}

#[tokio::test]
async fn default_extract_path_missing_says_so_explicitly() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "x", "expiry": 1})))
        .mount(&server).await;

    let api = api(&server.uri(), include_str!("../../../tests/fixtures/chain-default-extract.json"));
    let mut ex = executor();
    let err = ex.run(&api, "business", Some("test")).await.unwrap_err().to_string();
    assert!(err.contains("$.access_token"), "got: {err}");
    assert!(err.contains("token") && err.contains("expiry"), "error must list the keys found: {err}");
    assert!(err.contains("auth.extract"), "error must say how to fix it: {err}");
}
