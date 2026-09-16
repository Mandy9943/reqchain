use reqchain_core::{auth, model::Api, request, secrets::Secrets, vars::Scope};

const FIXTURE: &str = include_str!("../../../tests/fixtures/auth-static.json");

fn header_of(endpoint: &str, secrets: Secrets, name: &str) -> Option<String> {
    let api = Api::from_json(FIXTURE).unwrap();
    let ep = api.endpoint(endpoint).unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

#[test]
fn basic_auth_is_base64_of_user_colon_pass() {
    let secrets = Secrets::from_map([
        ("GW_USER".into(), "alice".into()),
        ("GW_PASS".into(), "hunter2".into()),
    ]);
    // base64("alice:hunter2")
    assert_eq!(
        header_of("basic", secrets, "authorization").as_deref(),
        Some("Basic YWxpY2U6aHVudGVyMg==")
    );
}

#[test]
fn bearer_auth_prefixes_the_token() {
    assert_eq!(
        header_of("bearer", Secrets::empty(), "authorization").as_deref(),
        Some("Bearer static-token")
    );
}

#[test]
fn custom_headers_are_added() {
    assert_eq!(
        header_of("custom", Secrets::empty(), "x-api-key").as_deref(),
        Some("abc123")
    );
    assert_eq!(
        header_of("custom", Secrets::empty(), "x-user").as_deref(),
        Some("12345678")
    );
}

#[test]
fn inherit_uses_the_api_level_auth() {
    assert_eq!(
        header_of("inheriting", Secrets::empty(), "authorization").as_deref(),
        Some("Bearer api-level")
    );
}

#[test]
fn none_overrides_inherited_auth() {
    assert_eq!(
        header_of("opted-out", Secrets::empty(), "authorization"),
        None
    );
}

#[test]
fn chained_is_left_for_the_chain_resolver() {
    assert_eq!(
        header_of("chained", Secrets::empty(), "authorization"),
        None
    );
}

#[test]
fn masked_hides_the_credential_of_an_authorization_header_but_keeps_the_scheme() {
    let api = Api::from_json(FIXTURE).unwrap();
    let secrets = Secrets::from_map([
        ("GW_USER".into(), "alice".into()),
        ("GW_PASS".into(), "hunter2".into()),
    ]);
    let ep = api.endpoint("basic").unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();

    // Not in the mask list: base64 hides the secret from a literal match.
    let masked = req.masked(&[]);
    let value = masked
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.clone());
    assert_eq!(value.as_deref(), Some("Basic ***"));

    let ep = api.endpoint("bearer").unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();
    let masked = req.masked(&[]);
    let value = masked
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.clone());
    assert_eq!(value.as_deref(), Some("Bearer ***"));
}
