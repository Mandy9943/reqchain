use reqchain_core::model::Api;

const FIXTURE: &str = include_str!("../../../tests/fixtures/ceibal-gateway-test.json");

#[test]
fn parses_and_reserializes_byte_identically() {
    let api = Api::from_json(FIXTURE).expect("fixture must parse");
    assert_eq!(api.id, "ceibal-gateway-test");
    assert_eq!(api.endpoints.len(), 2);
    assert_eq!(api.to_json_string(), FIXTURE);
}

#[test]
fn rejects_unknown_schema_version() {
    let bad = FIXTURE.replace("\"schemaVersion\": 1", "\"schemaVersion\": 99");
    let err = Api::from_json(&bad).unwrap_err().to_string();
    assert!(err.contains("schemaVersion"), "got: {err}");
    assert!(err.contains("99"), "got: {err}");
}

#[test]
fn rejects_unknown_fields() {
    let bad = FIXTURE.replace("\"baseUrl\"", "\"base_url\"");
    let err = Api::from_json(&bad).unwrap_err().to_string();
    assert!(err.contains("base_url"), "got: {err}");
}
