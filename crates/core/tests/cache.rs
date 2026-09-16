use reqchain_core::cache::{TokenCache, SKEW_SECONDS};
use reqchain_core::model::Auth;
use reqchain_core::paths::Paths;

fn auth_a() -> Auth {
    Auth::Bearer { token: "a".into() }
}
fn auth_b() -> Auth {
    Auth::Bearer { token: "b".into() }
}

#[test]
fn returns_a_live_token() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), Some(1_000 + 600));
    assert_eq!(c.get(&k, 1_000).as_deref(), Some("tok"));
}

#[test]
fn treats_a_token_as_expired_skew_seconds_early() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), Some(1_000));
    assert!(
        c.get(&k, 1_000 - SKEW_SECONDS + 1).is_none(),
        "must expire {SKEW_SECONDS}s early"
    );
    assert!(c.get(&k, 1_000 - SKEW_SECONDS - 5).is_some());
}

#[test]
fn a_token_without_ttl_never_expires() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), None);
    assert_eq!(c.get(&k, u64::MAX / 2).as_deref(), Some("tok"));
}

#[test]
fn changing_the_auth_definition_changes_the_key() {
    assert_ne!(
        TokenCache::key("api", &auth_a(), "test"),
        TokenCache::key("api", &auth_b(), "test")
    );
}

#[test]
fn changing_the_environment_changes_the_key() {
    assert_ne!(
        TokenCache::key("api", &auth_a(), "test"),
        TokenCache::key("api", &auth_a(), "prod")
    );
}

#[test]
fn invalidate_removes_the_entry() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), Some(9_999_999_999));
    c.invalidate(&k);
    assert!(c.get(&k, 1_000).is_none());
}

#[test]
fn persists_across_instances_with_owner_only_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let k = TokenCache::key("api", &auth_a(), "test");

    // Test new file creation with secure permissions
    {
        let mut c = TokenCache::persistent(&paths);
        c.put(&k, "tok".into(), Some(9_999_999_999));
        c.save().unwrap();
    }
    let reloaded = TokenCache::persistent(&paths);
    assert_eq!(reloaded.get(&k, 1_000).as_deref(), Some("tok"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(paths.cache_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "token cache must be owner-only");

        // Test repair of pre-existing file with wider permissions
        std::fs::set_permissions(paths.cache_file(), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        let mode_before = std::fs::metadata(paths.cache_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode_before & 0o777,
            0o644,
            "pre-existing file is set to 0o644"
        );

        {
            let mut c = TokenCache::persistent(&paths);
            c.put(&k, "updated".into(), Some(9_999_999_999));
            c.save().unwrap();
        }

        let mode_after = std::fs::metadata(paths.cache_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode_after & 0o777,
            0o600,
            "save() must tighten pre-existing file to 0o600"
        );
    }
}
