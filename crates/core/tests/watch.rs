use reqchain_core::paths::Paths;
use reqchain_core::watch;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn a_burst_of_writes_fires_one_callback() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();

    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();
    let _w = watch::watch(&paths, move || {
        seen.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();

    for i in 0..10 {
        std::fs::write(paths.apis_dir().join(format!("a{i}.json")), "{}").unwrap();
    }
    std::thread::sleep(watch::DEBOUNCE * 6);

    let n = hits.load(Ordering::SeqCst);
    assert!(n >= 1, "watcher never fired");
    assert!(n <= 3, "burst was not debounced: {n} callbacks");
}

#[test]
fn dropping_the_watcher_stops_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();
    let w = watch::watch(&paths, move || {
        seen.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    drop(w);
    std::fs::write(paths.apis_dir().join("a.json"), "{}").unwrap();
    std::thread::sleep(watch::DEBOUNCE * 6);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}
