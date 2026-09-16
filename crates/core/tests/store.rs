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

const GOOD: &str = include_str!("../../../tests/fixtures/ceibal-gateway-test.json");

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
    assert!(err.message.contains("line 1"), "error must locate the problem: {}", err.message);
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
