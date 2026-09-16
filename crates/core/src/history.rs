//! Per-endpoint run history, stored as JSONL outside `workspace/` so it never
//! pollutes a git-versioned file.
use crate::exec::RunResult;
use crate::paths::Paths;
use crate::request::{self, EffectiveBody, EffectiveRequest};
use serde::{Deserialize, Serialize};
use std::io::Write;

pub const DEFAULT_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub at: String,
    pub status: u16,
    pub elapsed_ms: u64,
    pub size_bytes: usize,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
}

/// Builds an entry from a finished run. `masked` must be the already-masked
/// effective request; `mask` is the list of literal values — secrets and
/// chain-derived tokens — that must never reach disk. Even a masked request can
/// carry a live credential in its own RESPONSE body (a chained-auth source
/// endpoint's response holds the token), so both bodies are redacted here with
/// the same substring mask `request::masked_with` uses, not just copied through.
pub fn entry_from(
    result: &RunResult,
    masked: &EffectiveRequest,
    mask: &[String],
    store_bodies: bool,
) -> HistoryEntry {
    HistoryEntry {
        at: chrono::Local::now().to_rfc3339(),
        status: result.status,
        elapsed_ms: result.elapsed_ms.min(u128::from(u64::MAX)) as u64,
        size_bytes: result.size_bytes,
        method: masked.method.as_str().to_string(),
        url: masked.url.clone(),
        request_body: if store_bodies {
            masked
                .body
                .as_ref()
                .map(body_text)
                .map(|b| request::redact(&b, mask))
        } else {
            None
        },
        response_body: if store_bodies {
            Some(request::redact(&result.body_text(), mask))
        } else {
            None
        },
    }
}

fn body_text(body: &EffectiveBody) -> String {
    match body {
        EffectiveBody::Text { content, .. } => content.clone(),
        other => format!("{other:?}"),
    }
}

fn file(paths: &Paths, api_id: &str, endpoint_id: &str) -> std::path::PathBuf {
    paths
        .history_dir()
        .join(sanitize(api_id))
        .join(format!("{}.jsonl", sanitize(endpoint_id)))
}

/// Ids come from files an agent may have written; keep them from escaping the
/// history directory.
fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn append(
    paths: &Paths,
    api_id: &str,
    endpoint_id: &str,
    entry: &HistoryEntry,
) -> std::io::Result<()> {
    let path = file(paths, api_id, endpoint_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Only treat "not found" as empty (file doesn't exist yet). Any other error
    // (permission denied, not a regular file, etc.) must be reported, not read
    // as an empty history — the next step would then truncate real entries away.
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    lines.push(serde_json::to_string(entry)?);
    let start = lines.len().saturating_sub(DEFAULT_LIMIT);

    // Write to a temp file in the same directory and rename it over the target,
    // so a crash or a concurrent writer never observes a half-written or
    // truncated history file.
    let tmp_path = path.with_extension(format!("jsonl.tmp.{}", std::process::id()));
    {
        let mut out = std::fs::File::create(&tmp_path)?;
        for line in &lines[start..] {
            writeln!(out, "{line}")?;
        }
    }
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Newest first. A line that no longer parses is skipped, never fatal.
pub fn load(paths: &Paths, api_id: &str, endpoint_id: &str) -> Vec<HistoryEntry> {
    let text = std::fs::read_to_string(file(paths, api_id, endpoint_id)).unwrap_or_default();
    let mut entries: Vec<HistoryEntry> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    entries
}
