use reqchain_core::{
    cache::TokenCache,
    chain::Executor,
    exec::Runner,
    model::Api,
    secrets::Secrets,
    validate::{validate_api, Severity},
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const CHAIN: &str = include_str!("../../../tests/fixtures/chain.json");

fn api(base: &str, fixture: &str) -> Api {
    Api::from_json(&fixture.replace("BASE_URL", base)).unwrap()
}

/// Issues a new token on every call so a test can tell refreshes apart.
struct TokenIssuer {
    calls: Arc<AtomicUsize>,
    expires_in: u64,
}

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
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(TokenIssuer {
            calls: calls.clone(),
            expires_in: 3600,
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/business"))
        .and(header("authorization", "Bearer token-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

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
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(TokenIssuer {
            calls: calls.clone(),
            expires_in: 3600,
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/business"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

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
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(TokenIssuer {
            calls: calls.clone(),
            expires_in: 1,
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/business"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    ex.run(&api, "business", Some("test")).await.unwrap();
    ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "expired token must be refetched"
    );
}

#[tokio::test]
async fn retries_once_with_a_fresh_token_on_401() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(TokenIssuer {
            calls: calls.clone(),
            expires_in: 3600,
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/business"))
        .and(header("authorization", "Bearer token-1"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/business"))
        .and(header("authorization", "Bearer token-2"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

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
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(TokenIssuer {
            calls: calls.clone(),
            expires_in: 3600,
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/business"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let res = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(res.status, 401, "the 401 is reported, not retried forever");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "exactly one refresh");
}

#[tokio::test]
async fn extracts_from_a_response_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).insert_header("x-token", "header-token"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/header-business"))
        .and(header("authorization", "Bearer header-token"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(
        ex.run(&api, "header-business", Some("test"))
            .await
            .unwrap()
            .status,
        200
    );
}

#[tokio::test]
async fn injects_into_a_query_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "qtok"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/query-business"))
        .and(query_param("access_token", "qtok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(
        ex.run(&api, "query-business", Some("test"))
            .await
            .unwrap()
            .status,
        200
    );
}

#[tokio::test]
async fn detects_a_cycle_and_names_the_path() {
    let api = api(
        "https://example.test",
        include_str!("../../../tests/fixtures/chain-cycle.json"),
    );
    let mut ex = executor();
    let err = ex
        .run(&api, "a", Some("test"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("cycle"), "got: {err}");
    assert!(
        err.contains('a') && err.contains('b'),
        "error must name the path: {err}"
    );
}

#[tokio::test]
async fn missing_source_endpoint_is_an_actionable_error() {
    let api = api(
        "https://example.test",
        include_str!("../../../tests/fixtures/chain-missing.json"),
    );
    let mut ex = executor();
    let err = ex
        .run(&api, "business", Some("test"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("nonexistent"), "got: {err}");
}

#[tokio::test]
async fn default_extract_path_missing_says_so_explicitly() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"token": "x", "expiry": 1})),
        )
        .mount(&server)
        .await;

    let api = api(
        &server.uri(),
        include_str!("../../../tests/fixtures/chain-default-extract.json"),
    );
    let mut ex = executor();
    let err = ex
        .run(&api, "business", Some("test"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("$.access_token"), "got: {err}");
    assert!(
        err.contains("token") && err.contains("expiry"),
        "error must list the keys found: {err}"
    );
    assert!(
        err.contains("auth.extract"),
        "error must say how to fix it: {err}"
    );
}

#[tokio::test]
async fn url_encodes_the_token_when_injecting_into_a_query_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "a+b/c=&d"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/query-business"))
        .and(query_param("access_token", "a+b/c=&d"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(
        ex.run(&api, "query-business", Some("test"))
            .await
            .unwrap()
            .status,
        200
    );
}

#[tokio::test]
async fn a_two_level_chain_keeps_every_auth_step_visible() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/a"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "token-a", "expires_in": 3600,
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/b"))
        .and(header("authorization", "Bearer token-a"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "token-b", "expires_in": 3600,
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/c"))
        .and(header("authorization", "Bearer token-b"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(
        &server.uri(),
        include_str!("../../../tests/fixtures/chain-deep.json"),
    );
    let mut ex = executor();
    let res = ex.run(&api, "c", Some("test")).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(
        res.auth_trace.len(),
        2,
        "both hops must be visible: {:?}",
        res.auth_trace
    );
    assert_eq!(res.auth_trace[0].endpoint_id, "a", "deepest step first");
    assert_eq!(res.auth_trace[1].endpoint_id, "b");
}

#[tokio::test]
async fn injects_into_a_json_body_pointer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "btok"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/body-business"))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"doc": "12345678", "token": "btok"}),
        ))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(
        ex.run(&api, "body-business", Some("test"))
            .await
            .unwrap()
            .status,
        200
    );
}

#[tokio::test]
async fn a_body_pointer_that_resolves_to_nothing_is_an_error_not_a_silent_skip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "btok"})),
        )
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let err = ex
        .run(&api, "bad-pointer-business", Some("test"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("/nope/deep"), "got: {err}");
    assert!(err.contains("doc"), "error must list the body keys: {err}");
    assert!(err.contains("unauthenticated"), "got: {err}");
}

#[tokio::test]
async fn a_token_with_no_expiry_is_not_cached() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/token-noexp"))
        .respond_with(TokenIssuerWithoutExpiry {
            calls: calls.clone(),
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/noexp-business"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    ex.run(&api, "noexp-business", Some("test")).await.unwrap();
    ex.run(&api, "noexp-business", Some("test")).await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a token we cannot age out must not be cached"
    );
}

/// Same as `TokenIssuer` but omits `expires_in`, so no TTL can be computed.
struct TokenIssuerWithoutExpiry {
    calls: Arc<AtomicUsize>,
}

impl Respond for TokenIssuerWithoutExpiry {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({ "access_token": format!("token-{n}") }))
    }
}

#[tokio::test]
async fn a_failing_auth_endpoint_is_reported_as_such() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token-fail"))
        .respond_with(ResponseTemplate::new(500).set_body_string("gateway exploded"))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let err = ex
        .run(&api, "fail-business", Some("test"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("500"), "error must name the status: {err}");
    assert!(
        err.contains("token-fail"),
        "error must name the auth endpoint: {err}"
    );
    assert!(
        err.contains("gateway exploded"),
        "error must show the body: {err}"
    );
}

#[tokio::test]
async fn xpath_extraction_says_it_is_not_implemented() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "x"})),
        )
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let err = ex
        .run(&api, "xpath-business", Some("test"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("xpath"), "got: {err}");
    assert!(err.contains("not implemented"), "got: {err}");
}

#[tokio::test]
async fn derived_tokens_are_exposed_so_the_caller_can_mask_them() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(TokenIssuer {
            calls: calls.clone(),
            expires_in: 3600,
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/business"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    ex.run(&api, "business", Some("test")).await.unwrap();
    let derived = ex.derived_values();
    assert!(
        derived.contains(&"token-1".to_string()),
        "the chain token must be exposed: {derived:?}"
    );
    // Final review, finding 3: a credential DERIVED from secrets — here the
    // base64 of the `/token` endpoint's Basic auth — contains no literal secret
    // substring, so `apply_static` now reports it and it joins the mask list too.
    assert!(
        derived.contains(&"YWxpY2U6aHVudGVyMg==".to_string()),
        "a static auth credential must be exposed for masking: {derived:?}"
    );
    assert_eq!(derived.len(), 2, "nothing else: {derived:?}");
}

/// Cross-checks `validate::validate_api`'s depth guard against the runtime's:
/// the two must agree exactly on where a chain becomes too deep. Uses
/// `tests/fixtures/chain-too-deep.json`, a linear chain `a -> b -> c -> d -> e -> f`
/// (6 endpoints). Starting from `b` walks only `b -> c -> d -> e -> f` (5
/// endpoints, at the limit) and must be accepted by both. Starting from `a`
/// walks all 6 and must be rejected by both.
#[tokio::test]
async fn a_five_level_chain_is_accepted_by_both_the_runtime_and_the_validator() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/f"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tf", "expires_in": 3600,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/e"))
        .and(header("authorization", "Bearer tf"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "te", "expires_in": 3600,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/d"))
        .and(header("authorization", "Bearer te"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "td", "expires_in": 3600,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/c"))
        .and(header("authorization", "Bearer td"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tc", "expires_in": 3600,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/b"))
        .and(header("authorization", "Bearer tc"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api(
        &server.uri(),
        include_str!("../../../tests/fixtures/chain-too-deep.json"),
    );

    let mut ex = executor();
    let res = ex.run(&api, "b", Some("test")).await;
    assert!(
        res.is_ok(),
        "the runtime must accept a 5-endpoint chain: {res:?}"
    );

    // The validator sees the whole file, including `a` (which makes the chain
    // 6 deep). Drop `a` to isolate the exact same b..f sub-chain the runtime
    // just walked, and confirm the validator raises no depth error for it.
    let mut reduced = api.clone();
    reduced.endpoints.retain(|e| e.id != "a");
    let diags = validate_api(&reduced);
    let depth_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error && d.message.contains("deeper than"))
        .collect();
    assert!(
        depth_errors.is_empty(),
        "validator must not flag a chain the runtime accepts: {depth_errors:?}"
    );
}

#[tokio::test]
async fn a_six_level_chain_is_rejected_by_both_the_runtime_and_the_validator() {
    // No mocks are mounted: the runtime must fail on the depth guard before
    // making any network call — the deepest hop (`f`) is never reached — so an
    // empty mock server is the correct fixture; any accidental HTTP call would
    // itself fail loudly and surface as a different, unexpected error.
    let server = MockServer::start().await;
    let api = api(
        &server.uri(),
        include_str!("../../../tests/fixtures/chain-too-deep.json"),
    );

    let mut ex = executor();
    let err = ex
        .run(&api, "a", Some("test"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("deeper than 5 levels"), "got: {err}");

    let diags = validate_api(&api);
    let depth_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error && d.message.contains("deeper than"))
        .collect();
    assert!(
        !depth_errors.is_empty(),
        "validator must flag a chain the runtime rejects: {diags:?}"
    );
}
