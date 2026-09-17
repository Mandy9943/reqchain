use crate::paths::Paths;
use std::collections::BTreeMap;

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

    /// A missing store is not an error: a workspace with no secrets is valid.
    pub fn load(paths: &Paths) -> Secrets {
        let Ok(text) = std::fs::read_to_string(paths.secrets_file()) else {
            return Secrets::default();
        };
        let map = serde_json::from_str::<BTreeMap<String, String>>(&text).unwrap_or_default();
        Secrets { map }
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
    /// or truncated secrets file. Unlike those two, the temp file must be
    /// unreadable by anyone but the current user from the moment it is
    /// created — the phase-1 review's finding against the token cache was
    /// exactly a `chmod` applied AFTER the write, leaving a window where the
    /// file existed world/group-readable. So the temp file is opened with
    /// mode 0600 directly via `OpenOptions`, never `set_permissions`
    /// afterwards.
    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        let path = paths.secrets_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.map)?;
        let tmp_path = path.with_extension(format!("json.tmp.{}", std::process::id()));

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp_path)?;
            f.write_all(text.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&tmp_path, &text)?;
        }

        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }
}
