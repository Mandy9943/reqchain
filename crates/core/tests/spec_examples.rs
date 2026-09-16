use reqchain_core::validate::{validate_text, Severity};

/// Every fenced ```json block in SPEC.md that looks like a complete API file.
fn spec_examples() -> Vec<String> {
    let spec = include_str!("../../../SPEC.md");
    let mut out = Vec::new();
    let mut rest = spec;
    while let Some(start) = rest.find("```json") {
        let after = &rest[start + 7..];
        let Some(end) = after.find("```") else { break };
        let block = after[..end].trim().to_string();
        if block.contains("\"schemaVersion\"") {
            out.push(block)
        }
        rest = &after[end + 3..];
    }
    out
}

#[test]
fn spec_contains_complete_examples() {
    assert!(
        spec_examples().len() >= 2,
        "SPEC.md must show at least the two acceptance cases"
    );
}

#[test]
fn every_spec_example_validates() {
    for (i, example) in spec_examples().iter().enumerate() {
        let errors: Vec<_> = validate_text(example)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "SPEC.md example {i} is invalid: {errors:?}"
        );
    }
}

/// Validating is not enough: the two worked examples must stay byte-identical to the
/// fixtures they claim to reproduce, or SPEC.md silently drifts from the repository.
#[test]
fn spec_examples_match_their_fixtures() {
    let fixtures = [
        (
            "tests/fixtures/ceibal-gateway-test.json",
            include_str!("../../../tests/fixtures/ceibal-gateway-test.json"),
        ),
        (
            "tests/fixtures/odilo.json",
            include_str!("../../../tests/fixtures/odilo.json"),
        ),
    ];
    let examples = spec_examples();
    assert_eq!(
        examples.len(),
        fixtures.len(),
        "SPEC.md has {} complete examples but {} fixtures are tracked here",
        examples.len(),
        fixtures.len()
    );
    for (example, (path, fixture)) in examples.iter().zip(fixtures) {
        assert_eq!(
            example.as_str(),
            fixture.trim(),
            "SPEC.md example has drifted from {path}"
        );
    }
}

#[test]
fn the_odilo_fixture_validates() {
    let errors: Vec<_> = validate_text(include_str!("../../../tests/fixtures/odilo.json"))
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}
