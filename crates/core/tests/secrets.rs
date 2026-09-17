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
    // Scoped to `write_temp_file` itself (to the end of the file, where it's
    // defined — both the unix and non-unix variants), not the whole module:
    // a broader scan could pass by accident on unrelated code mentioning
    // these same substrings (a doc comment, an unrelated helper) without
    // actually pinning the property of the function that matters.
    let start = src
        .find("fn write_temp_file")
        .expect("write_temp_file must exist in secrets.rs");
    let write_temp_file_src = &src[start..];

    assert!(
        write_temp_file_src.contains(".mode(0o600)"),
        "write_temp_file must open with an explicit 0600 mode, not rely on the umask"
    );
    // The symlink-planting property (finding 5) has the same
    // runtime-unobservability problem as the mode-at-open-time one: a
    // symlink attack can only be exercised against a PREDICTABLE temp
    // path, and the current naming scheme (nanoseconds + a counter) is
    // deliberately not predictable, so a runtime test can only ever cover
    // the OLD pid-only scheme (see
    // `a_symlink_at_the_old_predictable_temp_path_does_not_capture_the_write`
    // below) rather than today's actual attack surface. Pin the structural
    // property directly instead: `create(true)` silently follows (and, for
    // an existing regular file, ignores the requested mode on) a
    // pre-existing path, including a planted symlink — only `create_new`
    // refuses outright.
    assert!(
        write_temp_file_src.contains(".create_new(true)"),
        "write_temp_file must use create_new(true), not create(true) — create(true) would \
         silently follow or overwrite a pre-existing path, including a planted symlink"
    );
    assert!(
        !write_temp_file_src.contains("set_permissions"),
        "write_temp_file must not tighten permissions after writing — that TOCTOU window is \
         exactly the phase-1 token-cache bug this task calls out"
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

/// Review finding 3: every save produces a NEVER-REUSED temp filename
/// (nanoseconds + a counter), unlike the old pid-only scheme a later save
/// from the same pid would eventually overwrite — so a leftover from a
/// crash between the write and the rename (a hard crash or `SIGKILL`,
/// which the error-path cleanup in `save` cannot run against) would
/// otherwise sit there forever, holding a complete plaintext copy of the
/// store. `save` sweeps stale `<file>.json.tmp.*` siblings at the start of
/// every call, so the leftover is cleaned up on the very next save.
#[test]
fn a_stale_temp_file_from_a_previous_crash_is_swept_on_the_next_save() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(dir.path()).unwrap();
    let stale = dir.path().join("secrets.json.tmp.99999.123456789.7");
    std::fs::write(&stale, "leftover plaintext from a crashed save").unwrap();

    let mut s = Secrets::empty();
    s.set("A", "value".into());
    s.save(&paths).unwrap();

    assert!(
        !stale.exists(),
        "a stale temp file from a previous crash must be swept on the next save"
    );
    let back = Secrets::load(&paths).unwrap();
    assert_eq!(back.get("A"), Some("value"));
}

/// The sweep must be scoped to THIS store's own temp files — never delete
/// an unrelated file that merely happens to sit in the same directory.
#[test]
fn sweeping_stale_temp_files_does_not_touch_unrelated_files() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(dir.path()).unwrap();
    let unrelated = dir.path().join("cache").join("tokens.json");
    std::fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
    std::fs::write(&unrelated, "{}").unwrap();
    let sibling_file = dir.path().join("secrets.json.bak"); // not a `.tmp.` name
    std::fs::write(&sibling_file, "keep me").unwrap();

    let mut s = Secrets::empty();
    s.set("A", "value".into());
    s.save(&paths).unwrap();

    assert!(unrelated.exists());
    assert!(sibling_file.exists());
}
