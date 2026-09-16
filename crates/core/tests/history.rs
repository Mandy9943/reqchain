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
    let kept = reqchain_core::history::entry_from(&result, &req, true);
    assert_eq!(kept.response_body.as_deref(), Some("body"));
    let dropped = reqchain_core::history::entry_from(&result, &req, false);
    assert_eq!(dropped.status, 200);
    assert_eq!(dropped.size_bytes, 4);
    assert!(dropped.request_body.is_none());
    assert!(dropped.response_body.is_none());
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
