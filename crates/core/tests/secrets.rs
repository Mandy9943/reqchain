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

    let back = Secrets::load(&paths).unwrap();
    assert_eq!(back.get("GW_PASS"), Some("hunter2"));
}

/// Renamed from `saving_never_leaves_a_readable_temp_file_behind` — the
/// assertion below is that no temp file remains at all, not that a
/// remaining one would be unreadable. The name should say what it checks.
#[test]
fn saving_never_leaves_a_temp_file_behind() {
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

/// The two tests above (and any test that only inspects the FINAL mode of
/// `secrets_file()`) cannot distinguish "opened with mode 0600 already set"
/// from "created at a default mode, then `chmod`ed to 0600 afterwards" — both
/// implementations produce an identical final mode, yet the second has a
/// real TOCTOU window where the file briefly exists group/world readable.
/// That is the exact phase-1 token-cache bug the task brief calls out by
/// name. Runtime mode inspection cannot pin "at open time, not via a later
/// chmod" as a property, so this pins it at the source level instead: the
/// write path must open with an explicit mode, and must never call
/// `set_permissions` on the file it just wrote.
#[test]
fn save_sets_the_mode_at_open_time_not_via_a_later_chmod() {
    let src = include_str!("../src/secrets.rs");
    assert!(
        src.contains(".mode(0o600)"),
        "save() must open the temp file with an explicit 0600 mode, not rely on the umask"
    );
    assert!(
        !src.contains("set_permissions"),
        "save() must not tighten permissions after writing — that TOCTOU window is exactly \
         the phase-1 token-cache bug this task calls out"
    );
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

    let mut s2 = Secrets::load(&paths).unwrap();
    s2.set("A", "two".into());
    s2.remove("B"); // no-op, must not error
    s2.save(&paths).unwrap();

    let back = Secrets::load(&paths).unwrap();
    assert_eq!(back.get("A"), Some("two"));
}

/// A missing store is not an error — a fresh workspace has none yet.
#[test]
fn load_of_a_missing_file_is_an_empty_ok_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let loaded = Secrets::load(&paths).unwrap();
    assert!(loaded.names().is_empty());
}

/// An unparseable store IS an error — distinct from "missing" — so a caller
/// can refuse to save over it (which would silently destroy whatever it
/// still holds) instead of treating it as an empty, freely-overwritable
/// store.
#[test]
fn load_of_unparseable_json_is_an_error_not_a_silent_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::write(paths.secrets_file(), "{ not json").unwrap();
    let result = Secrets::load(&paths);
    assert!(
        result.is_err(),
        "corrupt JSON must be reported, not swallowed"
    );
}

/// A symlink planted at the exact temp-file path a naive `pid`-only scheme
/// would use must not let `save` silently write through it (or silently
/// drop the requested mode, which `.create(true)` would do for a
/// pre-existing path). `save` uses `create_new`, so it must refuse outright
/// when something is already sitting at ITS OWN chosen temp path — proven
/// here by planting a decoy at the specific name the old pid-only scheme
/// would have produced and confirming `save` still succeeds and still
/// produces a correctly-owned, 0600 file at the real target (i.e. it did not
/// get fooled into using the planted path).
#[cfg(unix)]
#[test]
fn a_symlink_at_the_old_predictable_temp_path_does_not_capture_the_write() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let decoy_target = dir.path().join("decoy-capture-target");

    let old_style_tmp_path = paths
        .secrets_file()
        .with_extension(format!("json.tmp.{}", std::process::id()));
    symlink(&decoy_target, &old_style_tmp_path).unwrap();

    let mut s = Secrets::empty();
    s.set("A", "value".into());
    s.save(&paths).unwrap();

    assert!(
        !decoy_target.exists(),
        "save must not have written through the planted symlink"
    );
    let mode = std::fs::metadata(paths.secrets_file())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let back = Secrets::load(&paths).unwrap();
    assert_eq!(back.get("A"), Some("value"));
}
