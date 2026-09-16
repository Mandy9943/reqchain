use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn from_env() -> Paths {
        let root = std::env::var_os("REQCHAIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_default();
                home.join(".config/reqchain")
            });
        Paths { root }
    }

    pub fn at(root: impl AsRef<Path>) -> Paths {
        Paths {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn workspace(&self) -> PathBuf {
        self.root.join("workspace")
    }

    pub fn apis_dir(&self) -> PathBuf {
        self.workspace().join("apis")
    }

    pub fn secrets_file(&self) -> PathBuf {
        self.root.join("secrets.json")
    }

    pub fn cache_file(&self) -> PathBuf {
        self.root.join("cache/tokens.json")
    }

    pub fn history_dir(&self) -> PathBuf {
        self.root.join("history")
    }
}
