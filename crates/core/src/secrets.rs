use crate::paths::Paths;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
pub struct Secrets {
    map: BTreeMap<String, String>,
}

impl Secrets {
    pub fn empty() -> Secrets {
        Secrets::default()
    }

    pub fn from_map(entries: impl IntoIterator<Item = (String, String)>) -> Secrets {
        Secrets {
            map: entries.into_iter().collect(),
        }
    }

    /// A MISSING store is not an error — a fresh workspace has no secrets —
    /// but an UNPARSEABLE one is, and the two must not be conflated: `save`
    /// writes the WHOLE map, so a caller that silently read a corrupt file
    /// as "empty" and then let the user add one secret would overwrite (and
    /// so destroy) every entry the corrupted file still physically held.
    /// Callers must surface an `Err` to the user rather than proceeding as
    /// if the store were empty.
    pub fn load(paths: &Paths) -> Result<Secrets, String> {
        let file = paths.secrets_file();
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Secrets::default());
            }
            Err(e) => return Err(format!("reading {}: {e}", file.display())),
        };
        let map = serde_json::from_str::<BTreeMap<String, String>>(&text)
            .map_err(|e| format!("{} is not valid JSON: {e}", file.display()))?;
        Ok(Secrets { map })
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(|s| s.as_str())
    }
    pub fn values(&self) -> impl Iterator<Item = &String> {
        self.map.values()
    }

    /// Sorted (the map is a `BTreeMap`, so iteration order is already
    /// lexicographic) — never returns a value, only names.
    pub fn names(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }

    pub fn set(&mut self, name: &str, value: String) {
        self.map.insert(name.to_string(), value);
    }

    /// Reports whether anything was actually removed.
    pub fn remove(&mut self, name: &str) -> bool {
        self.map.remove(name).is_some()
    }

    /// Writes the store to `paths.secrets_file()`. Mirrors `history::append`
    /// and `commands::write_atomically`: a temp file in the same directory,
    /// then renamed over the target, so a crash never leaves a half-written
    /// or truncated secrets file — and `sync_all`s the temp file before the
    /// rename, so the atomicity claim holds against a power loss too, not
    /// just a process crash (a rename with no preceding fsync can still
    /// leave a zero-length file behind on some filesystems/orderings).
    ///
    /// Unlike those two, this file must be unreadable by anyone but the
    /// current user from the moment it exists, and its path must not be
    /// plantable by an attacker who controls `REQCHAIN_DIR` (which is
    /// user-overridable to any directory):
    ///  - the mode is set via `OpenOptions::mode(0o600)` at open time, never
    ///    tightened by a permissions call made AFTER the write — the phase-1
    ///    review's finding against the token cache was exactly that kind of
    ///    after-the-fact chmod, leaving a window where the file existed
    ///    group/world readable;
    ///  - the temp path is opened with `create_new(true)`, not `create(true)`
    ///    — `create(true)` silently follows (and, for a regular file,
    ///    ignores `.mode()` on) a pre-existing path, including one planted
    ///    as a symlink pointing wherever an attacker likes. `create_new`
    ///    refuses outright if anything is already there;
    ///  - the temp filename includes a per-process counter and a
    ///    nanosecond timestamp, not just the pid, so it is not a predictable
    ///    name an attacker could pre-plant a symlink at between runs.
    ///
    /// On any failure the half-written temp file is removed before the
    /// error is returned, rather than left behind for the next save (or an
    /// attacker) to find.
    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        let path = paths.secrets_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.map)?;
        let tmp_path = unique_temp_path(&path);

        if let Err(e) = write_temp_file(&tmp_path, &text) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
        if let Err(e) = std::fs::rename(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
        Ok(())
    }
}

/// A temp path that is both unique (a concurrent save from another process,
/// or a retried save in this one, never collides) and unpredictable (an
/// attacker who wants to pre-plant a symlink at this path cannot guess it in
/// advance) — the pid alone, as a previous version of this function used,
/// is guessable and reused across process lifetimes.
fn unique_temp_path(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("json.tmp.{}.{nanos}.{counter}", std::process::id()))
}

#[cfg(unix)]
fn write_temp_file(tmp_path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(tmp_path)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_temp_file(tmp_path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp_path)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(())
}
