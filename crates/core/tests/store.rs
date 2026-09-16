use reqchain_core::{paths::Paths, store::Workspace};
use std::fs;

fn workspace_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Paths) {
    let dir = tempfile::tempdir().unwrap();
    let apis = dir.path().join("workspace/apis");
    fs::create_dir_all(&apis).unwrap();
    for (name, content) in files {
        fs::write(apis.join(name), content).unwrap();
    }
    let paths = Paths::at(dir.path());
    (dir, paths)
}

const GOOD: &str = include_str!("../../../tests/fixtures/example-gateway-test.json");

#[test]
fn loads_valid_apis() {
    let (_d, paths) = workspace_with(&[("a.json", GOOD)]);
    let ws = Workspace::load(&paths);
    assert_eq!(ws.apis.len(), 1);
    assert!(ws.errors.is_empty());
}

#[test]
fn one_broken_file_does_not_hide_the_others() {
    let (_d, paths) = workspace_with(&[("good.json", GOOD), ("broken.json", "{ nope")]);
    let ws = Workspace::load(&paths);
    assert_eq!(ws.apis.len(), 1, "the valid API must still load");
    assert_eq!(ws.errors.len(), 1);
    let err = &ws.errors[0];
    assert!(err.path.ends_with("broken.json"));
    assert!(
        err.message.contains("line 1"),
        "error must locate the problem: {}",
        err.message
    );
}

#[test]
fn broken_file_is_left_untouched_on_disk() {
    let (_d, paths) = workspace_with(&[("broken.json", "{ nope")]);
    let _ = Workspace::load(&paths);
    let still = std::fs::read_to_string(paths.apis_dir().join("broken.json")).unwrap();
    assert_eq!(still, "{ nope", "loader must never rewrite a file");
}

#[test]
fn missing_workspace_is_empty_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::load(&Paths::at(dir.path()));
    assert!(ws.apis.is_empty());
    assert!(ws.errors.is_empty());
}

#[test]
fn path_of_finds_the_file_an_api_was_loaded_from_even_with_a_mismatched_basename() {
    let (_d, paths) = workspace_with(&[("gateway.json", GOOD)]);
    let ws = Workspace::load(&paths);
    let path = ws
        .path_of("example-gateway-test")
        .expect("path must be recorded");
    assert!(path.ends_with("gateway.json"));
}

#[test]
fn path_of_is_none_for_an_unknown_id() {
    let (_d, paths) = workspace_with(&[("gateway.json", GOOD)]);
    let ws = Workspace::load(&paths);
    assert!(ws.path_of("nope").is_none());
}

#[test]
fn duplicate_ids_keep_the_first_file_and_report_an_error_naming_both_paths() {
    let (_d, paths) = workspace_with(&[("a.json", GOOD), ("b.json", GOOD)]);
    let ws = Workspace::load(&paths);
    // Both files parse fine and share the same id. Only the FIRST is kept in
    // `apis`: an id is the key every consumer addresses an API by (the source
    // map, `api()`, `path_of()`, and the UI's keyed lists), so returning two
    // entries under one key is never usable — it just breaks whoever assumes
    // the id identifies one API. The copy is reported as a `FileError` and
    // the rest of the workspace keeps working (design spec §9).
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.errors.len(), 1);
    let err = &ws.errors[0];
    assert!(
        err.message.contains("example-gateway-test"),
        "{}",
        err.message
    );
    assert!(err.message.contains("a.json"), "{}", err.message);
    assert!(err.message.contains("b.json"), "{}", err.message);
    let path = ws.path_of("example-gateway-test").unwrap();
    assert!(
        path.ends_with("a.json"),
        "must keep the first-loaded path: {path:?}"
    );
}

#[test]
fn an_unreadable_apis_directory_is_reported_not_swallowed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("workspace")).unwrap();
    // `apis` exists but is a file, so read_dir fails with something other than NotFound
    std::fs::write(dir.path().join("workspace/apis"), "not a directory").unwrap();
    let ws = Workspace::load(&Paths::at(dir.path()));
    assert!(ws.apis.is_empty());
    assert_eq!(
        ws.errors.len(),
        1,
        "the failure must be reported, not swallowed"
    );
    assert!(ws.errors[0].path.ends_with("apis"));
}
