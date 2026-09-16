// apps/desktop/src-tauri/tests/run.rs — against wiremock, no network
use reqchain_core::paths::Paths;
use reqchain_desktop::{commands, state::AppState};
use wiremock::matchers::{header, method, path};
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

/// Workspace with a `token` endpoint and a `bizq` endpoint chained onto it
/// via a QUERY-injected token (instead of `biz`'s header injection) — used to
/// reproduce reqwest's transport-error message, which appends `for url
/// (...)`, embedding a query-injected chain token in full.
fn state_for_query_inject(base_url: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    let text = format!(
        r#"{{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"{base_url}",
        "endpoints":[
          {{"id":"token","name":"Token","method":"POST","path":"/token"}},
          {{"id":"bizq","name":"BizQ","method":"GET","path":"/bizq",
            "auth":{{"type":"chained","source":{{"endpoint":"token"}},
              "inject":{{"into":"query","name":"access_token"}}}}}}]}}"#
    );
    std::fs::write(paths.apis_dir().join("demo.json"), text).unwrap();
    (dir, AppState::new(paths))
}

/// Workspace with a single endpoint secured by STATIC bearer auth (a fixed
/// literal token, not a secret or a chain) — used to prove a static auth
/// credential echoed back in a response is redacted too, without folding it
/// into the global REQUEST-display mask (which would corrupt unrelated
/// request output).
fn state_with_bearer(base_url: &str, token: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    let text = format!(
        r#"{{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"{base_url}",
        "endpoints":[
          {{"id":"staticauth","name":"StaticAuth","method":"GET","path":"/staticauth",
            "auth":{{"type":"bearer","token":"{token}"}}}}]}}"#
    );
    std::fs::write(paths.apis_dir().join("demo.json"), text).unwrap();
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

/// The frontend previews the selected endpoint on its own — on selection and on
/// environment change — so a preview that resolved the chain would POST to a
/// production token endpoint merely because the user clicked around the sidebar,
/// and that request would appear nowhere in the UI. A preview must therefore
/// touch the network ZERO times: not the business endpoint, not the auth one.
#[tokio::test]
async fn preview_endpoint_sends_no_request_at_all_not_even_the_auth_one() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .expect(0)
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

    // The injected header is still SHOWN — the whole point of the product is that
    // the chain is visible — but as a placeholder naming its source endpoint,
    // never as a token that was silently fetched.
    let authorization = preview
        .headers
        .iter()
        .find(|[k, _]| k.eq_ignore_ascii_case("authorization"))
        .map(|[_, v]| v.clone())
        .expect("the chained header must still be rendered");
    assert_eq!(authorization, "Bearer <chained token from `token`>");
    assert!(
        !authorization.contains(TOKEN),
        "a preview must never carry a real token: {authorization}"
    );

    // `expect(0)` is only checked when the mock server is dropped; assert here
    // too so the failure names the actual count.
    assert_eq!(
        mock.received_requests().await.unwrap().len(),
        0,
        "a preview must perform no I/O whatsoever"
    );
}

/// The counterpart: `copy as curl` is an explicit user action, so it keeps
/// resolving the chain for real and emits a runnable (masked) request.
#[tokio::test]
async fn curl_command_still_resolves_the_chain_for_real() {
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
    let curl = commands::curl_command_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    assert!(curl.contains("/biz"), "{curl}");
    assert!(
        !curl.contains("<chained token from"),
        "curl must resolve the chain, not render the preview placeholder: {curl}"
    );
    // Resolved, and then masked before display (spec §5.3).
    assert!(!curl.contains(TOKEN), "{curl}");
    assert!(curl.contains("***"), "{curl}");
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

/// Finding 1 (CRITICAL): `exec::ExecError::Transport`'s message is built from
/// reqwest's `Display`, which appends `" for url (...)"`. With a
/// query-injected chain token, that URL carries the live token in full — so
/// a transport failure on the business leg must not surface it raw.
///
/// The token is cached by a first run against a live mock, then the mock
/// server is dropped so the *business* leg's connection is refused on the
/// second run — the token step is a cache hit (no network needed for it), so
/// the failure is purely transport, and the URL it names still carries the
/// cached token via query injection.
#[tokio::test]
async fn a_transport_failure_on_a_query_injected_token_does_not_leak_it_in_the_error() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 })),
        )
        .mount(&mock)
        .await;

    let (_d, state) = state_for_query_inject(&mock.uri());

    // First run: caches the token (the business leg's own result is
    // irrelevant — /bizq is deliberately left unmocked here, so it 404s,
    // which is fine; the token cache key does not depend on `baseUrl`).
    let _ = commands::run_endpoint_inner(&state, "demo", "bizq", None).await;

    // Point the API at an unroutable host and reload. `reload` only
    // refreshes `workspace`/`secrets` — the executor's `TokenCache` is
    // untouched — so the token step below is served entirely from cache
    // (no network), and only the business leg's connection is refused.
    let text = r#"{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"http://127.0.0.1:1",
        "endpoints":[
          {"id":"token","name":"Token","method":"POST","path":"/token"},
          {"id":"bizq","name":"BizQ","method":"GET","path":"/bizq",
            "auth":{"type":"chained","source":{"endpoint":"token"},
              "inject":{"into":"query","name":"access_token"}}}]}"#;
    std::fs::write(state.paths.apis_dir().join("demo.json"), text).unwrap();
    state.reload().await;

    let err = commands::run_endpoint_inner(&state, "demo", "bizq", None)
        .await
        .unwrap_err();
    assert!(
        !err.contains(TOKEN),
        "token leaked in transport error: {err}"
    );
}

/// Finding 1 (CRITICAL): `chain::RunError::Chain` can embed up to 200 raw
/// characters of the auth endpoint's response body — which routinely echoes
/// back a rejected credential. Here the `/token` endpoint rejects the
/// request and echoes the secret used to authenticate to it; the resulting
/// error string must not carry that secret.
#[tokio::test]
async fn a_chain_error_excerpt_does_not_leak_the_credential_the_auth_endpoint_echoed_back() {
    const SECRET: &str = "supersecretvalue1234567890";
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string(format!("rejected credential: {SECRET}")),
        )
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    // Any value in the secret store belongs in every mask, whether or not
    // this particular endpoint references it by name — the same way a
    // secret used by a DIFFERENT endpoint must never leak through this one's
    // errors. Writing it to the store (rather than wiring it into the
    // fixture's own request) isolates exactly what this test is about: the
    // error-string redaction itself, not request interpolation.
    std::fs::write(
        state.paths.secrets_file(),
        serde_json::json!({ "IRRELEVANT": SECRET }).to_string(),
    )
    .unwrap();
    state.reload().await;

    let err = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap_err();
    assert!(
        !err.contains(SECRET),
        "secret echoed by the auth endpoint leaked in the error: {err}"
    );
}

/// Finding 3 (HIGH): the default `/biz` mock never echoes the token, so
/// response-body/header redaction had no test that would fail if it were
/// deleted. Here the business response echoes the token in BOTH its body
/// and a response header; the whole serialized `RunDto` must be clean.
#[tokio::test]
async fn the_business_response_echoing_the_token_is_redacted_everywhere() {
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
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!("echo: {TOKEN}"))
                .insert_header("x-echo", TOKEN),
        )
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    let run = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    let serialized = serde_json::to_string(&run).unwrap();
    assert!(
        !serialized.contains(TOKEN),
        "token leaked in echoed response: {serialized}"
    );
    assert!(run.body.contains("***"));
    assert!(run
        .headers
        .iter()
        .any(|[k, v]| k.eq_ignore_ascii_case("x-echo") && v.contains("***")));
}

/// Finding 4 (HIGH): nothing asserted the token is absent from the on-disk
/// history file on the default `storeBodies: true` path — only the
/// `storeBodies: false` test touched the raw file. Here the business
/// response echoes the token in its body, which is exactly what history
/// stores by default; the raw `.jsonl` must not carry it.
#[tokio::test]
async fn the_token_never_reaches_the_on_disk_history_file() {
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
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("echo: {TOKEN}")))
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    let raw =
        std::fs::read_to_string(state.paths.history_dir().join("demo").join("biz.jsonl")).unwrap();
    assert!(
        !raw.contains(TOKEN),
        "token leaked into history file: {raw}"
    );
}

/// Finding 5 (MEDIUM): `redact` alone does not expand percent-encoded forms.
/// A response header echoing a secret percent-encoded (as a real proxy or
/// `Location` redirect might) must still be caught.
#[tokio::test]
async fn a_percent_encoded_secret_in_a_response_header_is_still_redacted() {
    const SECRET: &str = "p@ss/w+rd=1234";
    const ENCODED: &str = "p%40ss%2Fw%2Brd%3D1234"; // percent-encoding of SECRET

    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/secure"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .insert_header("location", ENCODED),
        )
        .mount(&mock)
        .await;

    let (_d, state) = state_with_secret_header(&mock.uri(), SECRET);
    let run = commands::run_endpoint_inner(&state, "demo", "secure", None)
        .await
        .unwrap();

    let serialized = serde_json::to_string(&run).unwrap();
    assert!(
        !serialized.contains(ENCODED),
        "percent-encoded secret leaked: {serialized}"
    );
}

/// Finding 6 (MEDIUM): a static auth credential VALUE (here, a fixed bearer
/// token) never joined any mask list before this fix — only its header NAME
/// did. A server echoing it back in a response body leaked it verbatim. The
/// request side must stay untouched: the executor's own `Authorization`
/// header keeps its usual by-name `Bearer ***` masking, proving the fix did
/// NOT fold the static value into the global request-display mask.
#[tokio::test]
async fn a_static_auth_credential_echoed_in_the_response_is_redacted() {
    const STATIC_TOKEN: &str = "static-bearer-credential-999999";
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/staticauth"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("leaked: {STATIC_TOKEN}")))
        .mount(&mock)
        .await;

    let (_d, state) = state_with_bearer(&mock.uri(), STATIC_TOKEN);
    let run = commands::run_endpoint_inner(&state, "demo", "staticauth", None)
        .await
        .unwrap();

    assert!(
        !run.body.contains(STATIC_TOKEN),
        "static auth credential leaked in response body: {}",
        run.body
    );
    let [_, auth_header_value] = run
        .effective
        .headers
        .iter()
        .find(|[k, _]| k.eq_ignore_ascii_case("authorization"))
        .unwrap();
    assert_eq!(
        auth_header_value, "Bearer ***",
        "the static value must still be masked BY NAME on the request side, \
         not folded into a global substring mask"
    );
}

/// Finding 2 (HIGH): the `Executor`'s frozen `Secrets` snapshot. After a
/// rotation, `AppState::reload()` must push the new secret into the
/// long-lived `Executor`, not just into `state.secrets` — otherwise the
/// request keeps using the STALE value (sent on the wire, unmasked, since
/// only the NEW value is in anyone's mask) while claiming to be masked.
/// Proven two ways here: the mock only matches the NEW value (so a stale
/// request would 404), and the stale value never appears anywhere in the
/// serialized output.
#[tokio::test]
async fn a_rotated_secret_is_sent_not_the_stale_one_and_the_stale_one_never_leaks() {
    const OLD: &str = "old-secret-value-123456";
    const NEW: &str = "new-secret-value-abcdef";
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/secure"))
        .and(header("x-secret", NEW))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_with_secret_header(&mock.uri(), OLD);

    // Rotate the secret on disk, then reload — this must propagate into the
    // Executor's own (previously frozen) copy, not just `state.secrets`.
    std::fs::write(
        state.paths.secrets_file(),
        serde_json::json!({ "PASS": NEW }).to_string(),
    )
    .unwrap();
    state.reload().await;

    let run = commands::run_endpoint_inner(&state, "demo", "secure", None)
        .await
        .unwrap();
    assert_eq!(run.status, 200); // proves the NEW value, not the stale one, was sent

    let serialized = serde_json::to_string(&run).unwrap();
    assert!(
        !serialized.contains(OLD),
        "stale secret leaked: {serialized}"
    );
    assert!(
        !serialized.contains(NEW),
        "rotated secret leaked unmasked: {serialized}"
    );
}

/// Masking on the CACHE-HIT path had nothing pinning it. It is safe today only
/// because the GUI's cache lives in memory and is always filled by the very
/// `Executor` that derived the token, so the token is in `derived_values()` and
/// therefore in the mask. One persistence change — or one shared executor —
/// turns that coincidence into a live leak: a cache hit injects a real token
/// into the request, the auth trace and history with nothing masking it.
///
/// So: fill the cache with a token this executor never derived, run, and assert
/// the serialized DTO is clean.
#[tokio::test]
async fn a_cache_hit_masks_a_token_this_executor_never_derived() {
    const FOREIGN: &str = "foreign-token-not-derived-here";
    let mock = MockServer::start().await;
    // A cache hit must not call the auth endpoint at all.
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "access_token": "should-never-be-used", "expires_in": 3600 }),
        ))
        .expect(0)
        .mount(&mock)
        .await;
    // The business endpoint echoes the token back, exercising the response-side
    // mask as well as the request-side one.
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "echoed": format!("Bearer {FOREIGN}") })),
        )
        .mount(&mock)
        .await;

    let (dir, state) = state_for(&mock.uri(), "");
    // Populate the workspace so the executor can compute the key for `biz`.
    let _ = commands::load_workspace_inner(&state).await;
    {
        let api = state
            .workspace
            .lock()
            .unwrap()
            .api("demo")
            .cloned()
            .expect("api loaded");
        let mut executor = state.executor.lock().await;
        let key = executor
            .cache_key(&api, "biz", None)
            .expect("biz has a chained auth");
        let far_future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;
        executor
            .cache_mut()
            .put(&key, FOREIGN.to_string(), Some(far_future));
    }

    let dto = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .expect("the run succeeds off the cached token");

    assert!(
        dto.auth_trace.iter().any(|s| s.from_cache),
        "the test must actually exercise a cache hit: {:?}",
        dto.auth_trace
    );

    let serialized = serde_json::to_string(&dto).unwrap();
    assert!(
        !serialized.contains(FOREIGN),
        "a cached token the executor did not derive leaked into the DTO: {serialized}"
    );
    assert!(serialized.contains("***"), "{serialized}");

    // And it must not reach the history file on disk either.
    let history =
        std::fs::read_to_string(dir.path().join("history").join("demo").join("biz.jsonl")).unwrap();
    assert!(
        !history.contains(FOREIGN),
        "the cached token reached the history file: {history}"
    );
}
