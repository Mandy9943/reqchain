use reqchain_core::history::{self, HistoryEntry};
use reqchain_core::paths::Paths;

fn entry(status: u16) -> HistoryEntry {
    HistoryEntry {
        at: "2026-09-16T10:00:00-03:00".into(),
        status,
        elapsed_ms: 12,
        size_bytes: 3,
        method: "GET".into(),
        url: "https://api.example.com/x".into(),
        request_body: Some("req".into()),
        response_body: Some("res".into()),
    }
}

#[test]
fn appends_and_reads_back_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    for s in [200u16, 201, 202] {
        history::append(&paths, "api", "ep", &entry(s)).unwrap();
    }
    let got = history::load(&paths, "api", "ep");
    assert_eq!(
        got.iter().map(|e| e.status).collect::<Vec<_>>(),
        vec![202, 201, 200]
    );
}

#[test]
fn caps_the_file_at_the_default_limit() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    for i in 0..history::DEFAULT_LIMIT + 5 {
        history::append(&paths, "api", "ep", &entry(200 + i as u16)).unwrap();
    }
    let got = history::load(&paths, "api", "ep");
    assert_eq!(got.len(), history::DEFAULT_LIMIT);
    assert_eq!(got[0].status, 200 + (history::DEFAULT_LIMIT + 4) as u16);
}

#[test]
fn a_corrupt_line_is_skipped_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    history::append(&paths, "api", "ep", &entry(200)).unwrap();
    let file = paths.history_dir().join("api").join("ep.jsonl");
    let mut text = std::fs::read_to_string(&file).unwrap();
    text.push_str("{ not json\n");
    std::fs::write(&file, text).unwrap();
    assert_eq!(history::load(&paths, "api", "ep").len(), 1);
}

#[test]
fn history_for_an_endpoint_never_run_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(history::load(&Paths::at(dir.path()), "api", "ep").is_empty());
}

use reqchain_core::model::Api;

#[test]
fn store_bodies_false_drops_bodies_but_keeps_metadata() {
    let req = reqchain_core::request::EffectiveRequest {
        method: reqchain_core::model::Method::Get,
        url: "https://api.example.com/x".into(),
        headers: vec![],
        body: Some(reqchain_core::request::EffectiveBody::Text {
            content_type: "text/plain".into(),
            content: "secretish".into(),
        }),
    };
    let result = reqchain_core::exec::RunResult {
        status: 200,
        elapsed_ms: 9,
        size_bytes: 4,
        headers: vec![],
        body: b"body".to_vec(),
        effective: req.clone(),
        auth_trace: vec![],
    };
    let kept = reqchain_core::history::entry_from(&result, &req, &[], true);
    assert_eq!(kept.response_body.as_deref(), Some("body"));
    let dropped = reqchain_core::history::entry_from(&result, &req, &[], false);
    assert_eq!(dropped.status, 200);
    assert_eq!(dropped.size_bytes, 4);
    assert!(dropped.request_body.is_none());
    assert!(dropped.response_body.is_none());
}

/// HIGH finding from review: a chained-auth source endpoint's own response body
/// holds the token, so `entry_from` must redact every value in `mask` (secrets
/// and chain-derived tokens) from both bodies before the entry is even built —
/// and this must hold end to end, through `append` and back out through `load`,
/// not just inside `entry_from` itself.
#[test]
fn a_token_in_the_response_body_never_reaches_disk() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let token = "super-secret-chained-token";
    let req = reqchain_core::request::EffectiveRequest {
        method: reqchain_core::model::Method::Get,
        url: "https://api.example.com/x".into(),
        headers: vec![],
        body: Some(reqchain_core::request::EffectiveBody::Text {
            content_type: "application/json".into(),
            content: format!("{{\"authorize\":\"{token}\"}}"),
        }),
    };
    let result = reqchain_core::exec::RunResult {
        status: 200,
        elapsed_ms: 5,
        size_bytes: token.len(),
        headers: vec![],
        body: format!("{{\"access_token\":\"{token}\"}}").into_bytes(),
        effective: req.clone(),
        auth_trace: vec![],
    };
    let mask = vec![token.to_string()];
    let e = reqchain_core::history::entry_from(&result, &req, &mask, true);
    assert!(!e.response_body.as_ref().unwrap().contains(token));
    assert!(!e.request_body.as_ref().unwrap().contains(token));

    history::append(&paths, "api", "ep", &e).unwrap();

    // The token must not be sitting in the file on disk in any form.
    let raw = std::fs::read_to_string(paths.history_dir().join("api").join("ep.jsonl")).unwrap();
    assert!(
        !raw.contains(token),
        "raw history file must never hold the token: {raw}"
    );

    let got = history::load(&paths, "api", "ep");
    assert_eq!(got.len(), 1);
    assert!(!got[0].response_body.as_ref().unwrap().contains(token));
    assert!(got[0].response_body.as_ref().unwrap().contains("***"));
}

/// MEDIUM finding from review: `append` must not treat a real read error (e.g.
/// the target existing as something other than a plain file) as "no history
/// yet" — doing so would then truncate real entries away on the next write.
/// Portable without root: making the target path a directory forces
/// `read_to_string` to fail with something other than `NotFound`, the same
/// trick `store.rs`'s `an_unreadable_apis_directory_is_reported_not_swallowed`
/// test uses.
#[test]
fn a_real_read_error_is_propagated_not_treated_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let file = paths.history_dir().join("api").join("ep.jsonl");
    std::fs::create_dir_all(&file).unwrap();
    let result = history::append(&paths, "api", "ep", &entry(200));
    assert!(
        result.is_err(),
        "a real I/O error reading the history file must not be swallowed"
    );
}

/// MEDIUM finding from review: `append` must write through a temp file and
/// rename it into place rather than truncating in place, so a crash never
/// loses entries. We can't simulate a crash mid-write in a unit test, but we
/// can assert on the visible outcome: after a successful append, the target
/// directory holds only the final `.jsonl` file — no leftover temp file.
#[test]
fn append_leaves_no_temp_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    history::append(&paths, "api", "ep", &entry(200)).unwrap();
    let ep_dir = paths.history_dir().join("api");
    let names: Vec<String> = std::fs::read_dir(&ep_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["ep.jsonl".to_string()]);
}

#[test]
fn history_config_defaults_to_storing_bodies_and_round_trips() {
    let api: Api = Api::from_json(
        r#"{"schemaVersion":1,"id":"a","name":"A","baseUrl":"https://x","endpoints":[]}"#,
    )
    .unwrap();
    assert!(api.history.is_none());
    let with = Api::from_json(
        r#"{"schemaVersion":1,"id":"a","name":"A","baseUrl":"https://x","history":{"storeBodies":false},"endpoints":[]}"#,
    )
    .unwrap();
    assert!(!with.history.unwrap().store_bodies);
}
