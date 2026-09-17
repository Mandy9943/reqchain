use reqchain_core::paths::Paths;
use reqchain_core::secrets::Secrets;

#[test]
fn set_and_save_writes_an_0600_file_that_loads_back() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let mut s = Secrets::empty();
    s.set("GW_PASS", "hunter2".into());
    s.save(&paths).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(paths.secrets_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "secrets file must not be group/world readable"
        );
    }

    let back = Secrets::load(&paths);
    assert_eq!(back.get("GW_PASS"), Some("hunter2"));
}

#[test]
fn saving_never_leaves_a_readable_temp_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let mut s = Secrets::empty();
    s.set("A", "b".into());
    s.save(&paths).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

#[test]
fn remove_reports_whether_anything_was_removed() {
    let mut s = Secrets::from_map([("A".to_string(), "1".to_string())]);
    assert!(s.remove("A"));
    assert!(!s.remove("A"));
    assert!(s.names().is_empty());
}

#[test]
fn names_are_sorted_and_never_carry_a_value() {
    let s = Secrets::from_map([
        ("ZEBRA".to_string(), "z-value".to_string()),
        ("ALPHA".to_string(), "a-value".to_string()),
    ]);
    assert_eq!(s.names(), vec!["ALPHA".to_string(), "ZEBRA".to_string()]);
}

#[test]
fn save_is_atomic_and_overwrites_a_previous_version() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let mut s = Secrets::empty();
    s.set("A", "one".into());
    s.save(&paths).unwrap();

    let mut s2 = Secrets::load(&paths);
    s2.set("A", "two".into());
    s2.remove("B"); // no-op, must not error
    s2.save(&paths).unwrap();

    let back = Secrets::load(&paths);
    assert_eq!(back.get("A"), Some("two"));
}
