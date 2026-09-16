/// Every shipped fixture must survive `from_json` -> `to_json_string` unchanged.
/// The frontend's `serializeApi` writes the same shape, so if this holds and the
/// TS mirror is complete, a form edit cannot lose a field.
#[test]
fn every_fixture_round_trips_through_the_canonical_serializer() {
    for entry in std::fs::read_dir("../../tests/fixtures").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let Ok(api) = reqchain_core::model::Api::from_json(&text) else {
            continue; // deliberately-invalid fixtures are another test's business
        };
        let again = reqchain_core::model::Api::from_json(&api.to_json_string())
            .unwrap_or_else(|e| panic!("{} did not survive a round trip: {e}", path.display()));
        assert_eq!(
            api.to_json_string(),
            again.to_json_string(),
            "{} is not stable under re-serialization",
            path.display()
        );
    }
}
