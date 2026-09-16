use reqchain_core::model::Api;

const FIXTURE: &str = include_str!("../../../tests/fixtures/example-gateway-test.json");

#[test]
fn parses_and_reserializes_byte_identically() {
    let api = Api::from_json(FIXTURE).expect("fixture must parse");
    assert_eq!(api.id, "example-gateway-test");
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

/// Final review, finding 7: `ttl.unit` was a free string, so `"minutes"`
/// deserialized fine and was then silently treated as seconds. It is now a
/// closed set, and the error names the accepted values.
#[test]
fn rejects_an_unknown_ttl_unit() {
    let bad = FIXTURE.replace("\"unit\": \"seconds\"", "\"unit\": \"minutes\"");
    assert_ne!(bad, FIXTURE, "replacement must have matched");
    let err = Api::from_json(&bad).unwrap_err().to_string();
    assert!(err.contains("minutes"), "got: {err}");
    assert!(err.contains("seconds"), "got: {err}");
    assert!(err.contains("milliseconds"), "got: {err}");
}

#[test]
fn accepts_both_supported_ttl_units() {
    for unit in ["seconds", "milliseconds"] {
        let text = FIXTURE.replace("\"unit\": \"seconds\"", &format!("\"unit\": \"{unit}\""));
        Api::from_json(&text).unwrap_or_else(|e| panic!("`{unit}` must parse: {e}"));
    }
}
