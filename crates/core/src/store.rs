use crate::model::Api;
use crate::paths::Paths;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FileError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Workspace {
    pub apis: Vec<Api>,
    pub errors: Vec<FileError>,
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
                    Ok(api) => ws.apis.push(api),
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
}
