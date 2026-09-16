use crate::model::Api;
use crate::paths::Paths;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Workspace {
    pub apis: Vec<Api>,
    pub errors: Vec<FileError>,
    /// Maps an API's `id` to the file it was loaded from. Two files sharing an
    /// `id` keep only the first-loaded one — both here and in `apis` — and the
    /// copy is reported as a `FileError`. An id addresses exactly one API
    /// everywhere else (`api()`, `path_of()`, the desktop DTO list and its
    /// keyed rendering), so admitting a second entry under the same key would
    /// only break those consumers.
    sources: BTreeMap<String, PathBuf>,
}

impl Workspace {
    pub fn load(paths: &Paths) -> Workspace {
        let mut ws = Workspace::default();
        let dir = paths.apis_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                // Only treat "not found" as empty (directory doesn't exist yet).
                // Any other error (permission denied, not a directory, etc.) must be reported.
                if e.kind() != std::io::ErrorKind::NotFound {
                    ws.errors.push(FileError {
                        path: dir,
                        message: e.to_string(),
                    });
                }
                return ws;
            }
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for path in files {
            match std::fs::read_to_string(&path) {
                Err(e) => ws.errors.push(FileError {
                    path,
                    message: e.to_string(),
                }),
                Ok(text) => match Api::from_json(&text) {
                    Ok(api) => {
                        if let Some(existing) = ws.sources.get(&api.id) {
                            ws.errors.push(FileError {
                                path: path.clone(),
                                message: format!(
                                    "duplicate id `{}`: already loaded from {}, ignoring the copy in {}",
                                    api.id,
                                    existing.display(),
                                    path.display()
                                ),
                            });
                        } else {
                            ws.sources.insert(api.id.clone(), path.clone());
                            ws.apis.push(api);
                        }
                    }
                    Err(e) => ws.errors.push(FileError {
                        path,
                        message: e.to_string(),
                    }),
                },
            }
        }
        ws
    }

    pub fn api(&self, id: &str) -> Option<&Api> {
        self.apis.iter().find(|a| a.id == id)
    }

    /// The file the API with this `id` was loaded from, if any. When two files
    /// share an `id`, this points at the first one loaded (directory order).
    pub fn path_of(&self, api_id: &str) -> Option<&Path> {
        self.sources.get(api_id).map(PathBuf::as_path)
    }
}
