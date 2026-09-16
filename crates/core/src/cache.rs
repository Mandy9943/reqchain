use crate::model::Auth;
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const SKEW_SECONDS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
}

#[derive(Debug, Default)]
pub struct TokenCache {
    entries: BTreeMap<String, Entry>,
    file: Option<PathBuf>,
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TokenCache {
    pub fn in_memory() -> TokenCache {
        TokenCache::default()
    }

    pub fn persistent(paths: &Paths) -> TokenCache {
        let file = paths.cache_file();
        let entries = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        TokenCache {
            entries,
            file: Some(file),
        }
    }

    pub fn key(api_id: &str, auth: &Auth, scope_fingerprint: &str) -> String {
        use sha2::Digest;
        let definition = serde_json::to_string(auth).unwrap_or_default();
        let digest = sha2::Sha256::digest(
            format!("{api_id}\u{0}{definition}\u{0}{scope_fingerprint}").as_bytes(),
        );
        format!("{digest:x}")
    }

    pub fn get(&self, key: &str, now: u64) -> Option<String> {
        let entry = self.entries.get(key)?;
        match entry.expires_at {
            Some(exp) if now + SKEW_SECONDS >= exp => None,
            _ => Some(entry.value.clone()),
        }
    }

    pub fn put(&mut self, key: &str, value: String, expires_at: Option<u64>) {
        self.entries
            .insert(key.to_string(), Entry { value, expires_at });
    }

    pub fn invalidate(&mut self, key: &str) {
        self.entries.remove(key);
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?
        }
        let text = serde_json::to_string_pretty(&self.entries).unwrap_or_else(|_| "{}".into());
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(file)?;
            f.write_all(text.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(file, &text)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
