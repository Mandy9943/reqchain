use reqchain_core::validate::{validate_text, Diagnostic, Severity};

const GOOD: &str = include_str!("../../../tests/fixtures/ceibal-gateway-test.json");

fn errors(diags: &[Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn the_acceptance_fixture_is_valid() {
    let diags = validate_text(GOOD);
    assert!(
        errors(&diags).is_empty(),
        "unexpected errors: {:?}",
        errors(&diags)
    );
}

#[test]
fn reports_malformed_json_with_a_location() {
    let msgs = errors(&validate_text("{ nope"));
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].contains("line 1"), "got: {msgs:?}");
}

#[test]
fn reports_a_chained_auth_pointing_at_a_missing_endpoint() {
    let bad = GOOD.replace("\"endpoint\": \"token\"", "\"endpoint\": \"ghost\"");
    let msgs = errors(&validate_text(&bad));
    assert!(msgs.iter().any(|m| m.contains("ghost")), "got: {msgs:?}");
}

#[test]
fn reports_an_unknown_variable() {
    let bad = GOOD.replace("{{documento}}", "{{documentoo}}");
    let msgs = errors(&validate_text(&bad));
    assert!(
        msgs.iter().any(|m| m.contains("documentoo")),
        "got: {msgs:?}"
    );
}

#[test]
fn reports_an_unknown_expression_function() {
    let bad = include_str!("../../../tests/fixtures/exec.json")
        .replace("BASE_URL", "https://example.test")
        .replace("md5(now(", "sha512(now(");
    let msgs = errors(&validate_text(&bad));
    assert!(msgs.iter().any(|m| m.contains("sha512")), "got: {msgs:?}");
}

#[test]
fn reports_a_chain_cycle() {
    let text = include_str!("../../../tests/fixtures/chain-cycle.json")
        .replace("BASE_URL", "https://example.test");
    let msgs = errors(&validate_text(&text));
    assert!(msgs.iter().any(|m| m.contains("cycle")), "got: {msgs:?}");
}

#[test]
fn warns_but_does_not_fail_when_extract_relies_on_the_default() {
    let text = include_str!("../../../tests/fixtures/chain-default-extract.json")
        .replace("BASE_URL", "https://example.test");
    let diags = validate_text(&text);
    assert!(
        errors(&diags).is_empty(),
        "defaults are legal: {:?}",
        errors(&diags)
    );
    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .collect();
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("$.access_token")),
        "got: {warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.message.contains("$.expires_in")),
        "got: {warnings:?}"
    );
}

#[test]
fn every_diagnostic_carries_a_path() {
    let bad = GOOD.replace("\"endpoint\": \"token\"", "\"endpoint\": \"ghost\"");
    for d in validate_text(&bad) {
        assert!(!d.path.is_empty(), "diagnostic without a path: {d:?}");
    }
}

#[test]
fn reports_a_chain_deeper_than_the_runtime_allows() {
    let text = include_str!("../../../tests/fixtures/chain-too-deep.json")
        .replace("BASE_URL", "https://example.test");
    let msgs = errors(&validate_text(&text));
    assert!(
        msgs.iter().any(|m| m.contains("deeper than")),
        "got: {msgs:?}"
    );
}

#[test]
fn reports_xpath_extraction_as_an_error_not_a_warning() {
    let bad = GOOD.replace(
        "\"extract\": {\n          \"from\": \"body\",\n          \"jsonPath\": \"$.access_token\"\n        },",
        "\"extract\": {\n          \"from\": \"body\",\n          \"xpath\": \"/token\"\n        },",
    );
    assert_ne!(bad, GOOD, "replacement must have matched");
    let diags = validate_text(&bad);
    let errs = errors(&diags);
    assert!(
        errs.iter()
            .any(|m| m.contains("xpath extraction is not implemented yet — use jsonPath or regex")),
        "got: {errs:?}"
    );
}

#[test]
fn fixtures_satisfy_the_published_json_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../schema/reqchain-api.schema.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("schema itself must be valid");

    let fixtures_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let mut checked = 0;
    for entry in std::fs::read_dir(&fixtures_dir).expect("fixtures dir must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable fixture");
        let instance: serde_json::Value = serde_json::from_str(&text).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{} violates schema: {errors:?}",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 9,
        "expected at least 9 fixtures to be checked, found {checked}"
    );
}
