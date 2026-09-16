use reqchain_core::model::Api;
use reqchain_core::secrets::Secrets;
use reqchain_core::vars::Scope;

fn api() -> Api {
    Api::from_json(include_str!("../../../tests/fixtures/precedence.json")).unwrap()
}

#[test]
fn endpoint_beats_environment_beats_api() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    // who = defined at all three levels
    assert_eq!(scope.interpolate("{{who}}").unwrap(), "endpoint");
    // where = defined at environment and api
    assert_eq!(scope.interpolate("{{where}}").unwrap(), "environment");
    // what = defined only at api
    assert_eq!(scope.interpolate("{{what}}").unwrap(), "api");
}

#[test]
fn environment_selection_changes_resolution() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, Some("prod"), &secrets);
    assert_eq!(scope.interpolate("{{where}}").unwrap(), "prod-environment");
}

#[test]
fn interpolates_multiple_occurrences_and_surrounding_text() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    assert_eq!(
        scope.interpolate("a/{{what}}/b/{{what}}").unwrap(),
        "a/api/b/api"
    );
}

#[test]
fn unknown_variable_is_an_actionable_error() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let err = scope.interpolate("x{{nope}}y").unwrap_err().to_string();
    assert!(err.contains("nope"), "got: {err}");
}

#[test]
fn resolves_secrets_and_masks_them() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let secrets = Secrets::from_map([("GW_PASS".to_string(), "hunter2".to_string())]);
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    assert_eq!(scope.interpolate("{{secret:GW_PASS}}").unwrap(), "hunter2");
    assert_eq!(scope.interpolate_masked("{{secret:GW_PASS}}"), "***");
}

#[test]
fn missing_secret_names_the_store() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let err = scope
        .interpolate("{{secret:ABSENT}}")
        .unwrap_err()
        .to_string();
    assert!(err.contains("ABSENT"), "got: {err}");
    assert!(err.contains("secret"), "got: {err}");
}
