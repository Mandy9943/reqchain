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
}
