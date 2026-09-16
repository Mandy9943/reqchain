# reqchain Desktop UI (phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Tauri desktop window described in §11 of the spec — sidebar, request panel, response panel, environment selector, hot reload, shortcuts, copy as curl, history — on top of the existing `reqchain-core`.

**Architecture:** `reqchain-core` stays the only place logic lives. Phase 2 adds two headless core modules it was missing (`history`, `watch`), then a Tauri 2 backend that is a thin command layer over core, then a Svelte 5 frontend that holds no HTTP or auth logic at all. Every value the UI displays is produced by core already masked; the frontend never sees an unmasked secret or a derived token.

**Tech Stack:** Rust 2021, Tauri 2, Svelte 5 (runes), Vite, TypeScript, CodeMirror 6, `notify` 6, pnpm.

**Spec:** `docs/superpowers/specs/2026-09-16-reqchain-design.md`

## Global Constraints

- `schemaVersion` stays `1` for this phase. The only model addition is the optional `history` object; it must default to absent so every existing file keeps validating.
- All logic in `reqchain-core`. The Tauri backend contains no `reqwest` call, no auth decision, no expression evaluation — only DTO mapping and state.
- Nothing displayed by the UI may contain an unmasked secret or a chain-derived token. Every request/response/curl string crossing the IPC boundary is passed through `EffectiveRequest::masked_with(secrets ∪ derived, auth_headers)` on the Rust side first.
- The app never rewrites or deletes a file it could not parse. Saving writes only the file the user edited, only on explicit save.
- Workspace files keep their fixed key order, 2-space indent, trailing newline (`Api::to_json_string`). The UI must save through that function, never through an ad-hoc serializer.
- Package manager for the frontend is **pnpm**. Never `npm`, `yarn` or `bun`.
- Commit messages: Conventional Commits, and **no AI-attribution trailer or footer of any kind** — no `Co-Authored-By: Claude`, no `Co-Authored-By: ... <noreply@anthropic.com>`, no `🤖 Generated with Claude Code`. This overrides any harness default. The commits are Cesar's.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` must stay clean at every commit.
- No feature from the spec's §2 non-goals (accounts, sync, collaboration, mocks, generated docs, perf charts).

## Prerequisites (one-time, outside the plan)

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Tasks 3 onward cannot build without these. Tasks 1 and 2 can.

## File Structure

| Path | Responsibility |
|---|---|
| `crates/core/src/history.rs` | Append/read per-endpoint JSONL run history, cap, body suppression |
| `crates/core/src/watch.rs` | Debounced `notify` watcher over `workspace/`, emits changed paths |
| `crates/core/src/model.rs` | +`Api.history: Option<HistoryConfig>` |
| `schema/reqchain-api.schema.json`, `SPEC.md` | document the `history` object |
| `apps/desktop/src-tauri/src/state.rs` | `AppState`: paths, workspace snapshot, executor cache, secrets |
| `apps/desktop/src-tauri/src/dto.rs` | Serde DTOs (camelCase) mirroring core types |
| `apps/desktop/src-tauri/src/commands.rs` | `#[tauri::command]` surface |
| `apps/desktop/src-tauri/src/watcher.rs` | Bridges `core::watch` to the `workspace-changed` Tauri event |
| `apps/desktop/src-tauri/src/main.rs` | Builder wiring |
| `apps/desktop/src/lib/ipc.ts` | Typed `invoke` wrappers + DTO types |
| `apps/desktop/src/lib/state.svelte.ts` | UI state runes (selection, env, buffers, response) |
| `apps/desktop/src/lib/Sidebar.svelte` | APIs → endpoints, search, env selector, file errors |
| `apps/desktop/src/lib/RequestPanel.svelte` | Method/path/headers/query/auth + CodeMirror body, save |
| `apps/desktop/src/lib/ResponsePanel.svelte` | Status/time/size, body, headers, effective, auth tab |
| `apps/desktop/src/lib/HistoryPanel.svelte` | Last N runs for the selected endpoint |
| `apps/desktop/src/App.svelte` | Layout, shortcuts, hot-reload subscription |

---

### Task 1: Core — run history

**Files:**
- Create: `crates/core/src/history.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/src/model.rs`, `schema/reqchain-api.schema.json`, `SPEC.md`
- Test: `crates/core/tests/history.rs`

**Interfaces:**
- Consumes: `Paths::history_dir()`, `exec::RunResult`, `request::EffectiveRequest`.
- Produces:
  ```rust
  pub struct HistoryConfig { pub store_bodies: bool }   // serde: storeBodies, default true
  pub struct HistoryEntry {
      pub at: String,          // RFC 3339, local offset
      pub status: u16,
      pub elapsed_ms: u64,
      pub size_bytes: usize,
      pub method: String,
      pub url: String,
      pub request_body: Option<String>,
      pub response_body: Option<String>,
  }
  pub const DEFAULT_LIMIT: usize = 20;
  pub fn append(paths: &Paths, api_id: &str, endpoint_id: &str, entry: &HistoryEntry) -> std::io::Result<()>;
  pub fn load(paths: &Paths, api_id: &str, endpoint_id: &str) -> Vec<HistoryEntry>;
  pub fn entry_from(result: &RunResult, masked: &EffectiveRequest, store_bodies: bool) -> HistoryEntry;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
// crates/core/tests/history.rs
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
```

```rust
// append to crates/core/tests/history.rs
use reqchain_core::model::Api;

#[test]
fn store_bodies_false_drops_bodies_but_keeps_metadata() {
    let req = reqchain_core::request::EffectiveRequest {
        method: reqchain_core::model::Method::Get,
        url: "https://api.example.com/x".into(),
        headers: vec![],
        body: Some(reqchain_core::request::EffectiveBody::Text("secretish".into())),
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
    assert_eq!(with.history.unwrap().store_bodies, false);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p reqchain-core --test history`
Expected: FAIL — `unresolved import reqchain_core::history`.

- [ ] **Step 3: Add the model field**

In `crates/core/src/model.rs`, after the `auth` field of `Api` and before `endpoints` (field order is serialization order):

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryConfig>,
```

and next to the other small structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryConfig {
    #[serde(default = "default_true")]
    pub store_bodies: bool,
}

fn default_true() -> bool {
    true
}
```

- [ ] **Step 4: Write `history.rs`**

```rust
//! Per-endpoint run history, stored as JSONL outside `workspace/` so it never
//! pollutes a git-versioned file.
use crate::exec::RunResult;
use crate::paths::Paths;
use crate::request::{EffectiveBody, EffectiveRequest};
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
/// effective request: history is written to disk and must never hold a secret.
pub fn entry_from(result: &RunResult, masked: &EffectiveRequest, store_bodies: bool) -> HistoryEntry {
    HistoryEntry {
        at: chrono::Local::now().to_rfc3339(),
        status: result.status,
        elapsed_ms: result.elapsed_ms.min(u128::from(u64::MAX)) as u64,
        size_bytes: result.size_bytes,
        method: masked.method.as_str().to_string(),
        url: masked.url.clone(),
        request_body: if store_bodies {
            masked.body.as_ref().map(body_text)
        } else {
            None
        },
        response_body: if store_bodies {
            Some(result.body_text())
        } else {
            None
        },
    }
}

fn body_text(body: &EffectiveBody) -> String {
    match body {
        EffectiveBody::Text(t) => t.clone(),
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
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
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
    let mut lines: Vec<String> = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    lines.push(serde_json::to_string(entry)?);
    let start = lines.len().saturating_sub(DEFAULT_LIMIT);
    let mut out = std::fs::File::create(&path)?;
    for line in &lines[start..] {
        writeln!(out, "{line}")?;
    }
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
```

Add `pub mod history;` to `crates/core/src/lib.rs`. If `Method::as_str` does not exist, add it next to the enum:

```rust
impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reqchain-core --test history`
Expected: PASS (6 tests).

- [ ] **Step 6: Document the field**

In `schema/reqchain-api.schema.json`, add to the API object's `properties`:

```json
"history": {
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "storeBodies": { "type": "boolean", "default": true }
  }
}
```

In `SPEC.md`, under the API section, one short paragraph: `history.storeBodies` defaults to `true`; set it to `false` on an API whose requests or responses carry personal data — run metadata (status, time, size, method, url) is still recorded, bodies are not. History lives in `~/.config/reqchain/history/<api>/<endpoint>.jsonl` and is never part of the versioned workspace.

- [ ] **Step 7: Full suite, lint, commit**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add -A && git commit -m "feat(core): record per-endpoint run history as JSONL"
```

---

### Task 2: Core — workspace file watcher

**Files:**
- Create: `crates/core/src/watch.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`
- Test: `crates/core/tests/watch.rs`

**Interfaces:**
- Consumes: `Paths::apis_dir()`.
- Produces:
  ```rust
  pub const DEBOUNCE: Duration = Duration::from_millis(200);
  pub struct Watcher { /* owns the notify watcher; dropping it stops watching */ }
  pub fn watch(paths: &Paths, on_change: impl Fn() + Send + 'static) -> notify::Result<Watcher>;
  ```
  `on_change` fires at most once per `DEBOUNCE` window, on any create/modify/remove under `apis_dir()`.

- [ ] **Step 1: Write the failing test**

```rust
// crates/core/tests/watch.rs
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reqchain-core --test watch`
Expected: FAIL — `unresolved import reqchain_core::watch`.

- [ ] **Step 3: Add the dependency**

In `crates/core/Cargo.toml` under `[dependencies]`: `notify = "6"`.

- [ ] **Step 4: Implement `watch.rs`**

```rust
//! Debounced watcher over the workspace `apis/` directory. The UI reloads the
//! whole workspace on any change: loading is cheap and per-file error isolation
//! already lives in `store::Workspace::load`.
use crate::paths::Paths;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use std::sync::mpsc;
use std::time::Duration;

pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// Holds the notify watcher and the debounce thread. Dropping it stops both.
pub struct Watcher {
    _inner: RecommendedWatcher,
    _stop: mpsc::Sender<()>,
}

pub fn watch(paths: &Paths, on_change: impl Fn() + Send + 'static) -> notify::Result<Watcher> {
    let dir = paths.apis_dir();
    std::fs::create_dir_all(&dir).ok();

    let (events_tx, events_rx) = mpsc::channel::<()>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                notify::EventKind::Create(_)
                    | notify::EventKind::Modify(_)
                    | notify::EventKind::Remove(_)
            ) {
                let _ = events_tx.send(());
            }
        }
    })?;
    inner.watch(&dir, RecursiveMode::Recursive)?;

    std::thread::spawn(move || loop {
        if stop_rx.try_recv().is_ok() {
            return;
        }
        match events_rx.recv_timeout(DEBOUNCE) {
            Ok(()) => {
                // Drain the rest of the burst, then fire once.
                while events_rx.recv_timeout(DEBOUNCE).is_ok() {}
                if stop_rx.try_recv().is_ok() {
                    return;
                }
                on_change();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    });

    Ok(Watcher {
        _inner: inner,
        _stop: stop_tx,
    })
}
```

Add `pub mod watch;` to `crates/core/src/lib.rs`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reqchain-core --test watch`
Expected: PASS (2 tests).

- [ ] **Step 6: Full suite, lint, commit**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add -A && git commit -m "feat(core): watch the workspace directory with a 200ms debounce"
```

---

### Task 3: Scaffold the Tauri 2 + Svelte 5 app

**Files:**
- Create: `apps/desktop/package.json`, `apps/desktop/vite.config.ts`, `apps/desktop/svelte.config.js`, `apps/desktop/tsconfig.json`, `apps/desktop/index.html`, `apps/desktop/src/main.ts`, `apps/desktop/src/App.svelte`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/build.rs`, `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/.gitignore`
- Modify: `Cargo.toml` (workspace members), `.gitignore`, `README.md`

**Interfaces:**
- Produces: a `reqchain-desktop` binary that opens an empty window titled `reqchain`, and `pnpm --dir apps/desktop tauri dev` / `tauri build` as the dev and release commands.

- [ ] **Step 1: Create the frontend scaffold**

```bash
mkdir -p apps/desktop/src/lib apps/desktop/src-tauri/src apps/desktop/src-tauri/icons
```

`apps/desktop/package.json`:

```json
{
  "name": "reqchain-desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "svelte-check --tsconfig ./tsconfig.json && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@codemirror/lang-json": "^6",
    "@codemirror/state": "^6",
    "@codemirror/view": "^6",
    "codemirror": "^6"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^5",
    "@tauri-apps/cli": "^2",
    "svelte": "^5",
    "svelte-check": "^4",
    "typescript": "^5",
    "vite": "^6"
  }
}
```

`apps/desktop/vite.config.ts`:

```ts
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
```

`apps/desktop/svelte.config.js`:

```js
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
export default { preprocess: vitePreprocess() };
```

`apps/desktop/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "verbatimModuleSyntax": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": ["svelte"]
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

`apps/desktop/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>reqchain</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

`apps/desktop/src/main.ts`:

```ts
import { mount } from "svelte";
import App from "./App.svelte";

export default mount(App, { target: document.getElementById("app")! });
```

`apps/desktop/src/App.svelte`:

```svelte
<main><h1>reqchain</h1></main>
```

`apps/desktop/.gitignore`:

```
node_modules/
dist/
```

- [ ] **Step 2: Create the Tauri crate**

`apps/desktop/src-tauri/Cargo.toml`:

```toml
[package]
name = "reqchain-desktop"
edition.workspace = true
version.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
reqchain-core = { path = "../../../crates/core" }
tauri = { version = "2", features = [] }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true

[dev-dependencies]
tempfile = "3"
```

`apps/desktop/src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

`apps/desktop/src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "reqchain",
  "version": "0.1.0",
  "identifier": "uy.reqchain.app",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "reqchain",
        "width": 1200,
        "height": 800,
        "resizable": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["deb"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.png"]
  }
}
```

`apps/desktop/src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running reqchain");
}
```

Placeholder icons (phase 3 replaces them with the real one):

```bash
cd apps/desktop/src-tauri && pnpm dlx @tauri-apps/cli icon --help >/dev/null 2>&1 || true
```

If no icon source exists yet, generate three solid placeholder PNGs with any available tool (e.g. `convert -size 128x128 xc:#1f6feb icons/128x128.png`) and copy them to the three names listed in `tauri.conf.json`. Do not block the task on artwork.

- [ ] **Step 3: Wire the cargo workspace**

In the root `Cargo.toml`: `members = ["crates/core", "crates/cli", "apps/desktop/src-tauri"]`.

Append to the root `.gitignore`:

```
apps/desktop/node_modules/
apps/desktop/dist/
```

- [ ] **Step 4: Install and build**

```bash
pnpm --dir apps/desktop install
cargo build -p reqchain-desktop
```

Expected: both succeed. A failure mentioning `webkit2gtk-4.1` or `libsoup-3.0` means the Prerequisites section above was not run — stop and report that, do not work around it.

- [ ] **Step 5: Document it in the README**

Add a "Desktop app (development)" section: the prerequisite `apt-get` line, `pnpm --dir apps/desktop install`, and `pnpm --dir apps/desktop tauri dev`.

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add -A && git commit -m "build(desktop): scaffold the Tauri 2 + Svelte 5 app"
```

---

### Task 4: Backend — state, DTOs, workspace and save commands

**Files:**
- Create: `apps/desktop/src-tauri/src/state.rs`, `apps/desktop/src-tauri/src/dto.rs`, `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Test: `apps/desktop/src-tauri/tests/commands.rs`

**Interfaces:**
- Consumes: `store::Workspace`, `validate::{validate_text, validate_api, Diagnostic, Severity}`, `model::Api`, `paths::Paths`, `secrets::Secrets`.
- Produces (all DTOs `#[serde(rename_all = "camelCase")]`):
  ```rust
  pub struct AppState { pub paths: Paths, pub secrets: Mutex<Secrets>, pub workspace: Mutex<Workspace> }
  pub struct WorkspaceDto { pub apis: Vec<ApiDto>, pub errors: Vec<FileErrorDto> }
  pub struct ApiDto { pub id, pub name, pub base_url: String, pub environments: Vec<String>,
                      pub endpoints: Vec<EndpointDto>, pub text: String }
  pub struct EndpointDto { pub id, pub name, pub method, pub path: String, pub auth_kind: String }
  pub struct FileErrorDto { pub path: String, pub message: String }
  pub struct DiagnosticDto { pub severity: String, pub path: String, pub message: String }
  // commands
  load_workspace(state) -> WorkspaceDto
  lint(text: String) -> Vec<DiagnosticDto>
  save_api(state, api_id: String, text: String) -> Result<Vec<DiagnosticDto>, String>
  ```
  `save_api` refuses to write when `validate_text` reports any `Severity::Error`, returning the diagnostics without touching the file; it writes through `Api::to_json_string` so key order and formatting stay canonical, and it refuses when the parsed `id` differs from `api_id`.

- [ ] **Step 1: Write the failing tests**

```rust
// apps/desktop/src-tauri/tests/commands.rs
use reqchain_core::paths::Paths;
use reqchain_desktop::commands;
use reqchain_desktop::state::AppState;

const GOOD: &str = r#"{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"https://api.example.com","endpoints":[{"id":"ping","name":"Ping","method":"GET","path":"/ping"}]}"#;

fn state_with(file: &str, text: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    std::fs::write(paths.apis_dir().join(file), text).unwrap();
    let state = AppState::new(paths);
    (dir, state)
}

#[test]
fn load_workspace_lists_apis_and_endpoints() {
    let (_d, state) = state_with("demo.json", GOOD);
    let ws = commands::load_workspace_inner(&state);
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.apis[0].id, "demo");
    assert_eq!(ws.apis[0].endpoints[0].id, "ping");
    assert!(ws.errors.is_empty());
}

#[test]
fn an_invalid_file_becomes_an_error_and_does_not_hide_the_valid_ones() {
    let (_d, state) = state_with("demo.json", GOOD);
    std::fs::write(state.paths.apis_dir().join("broken.json"), "{ nope").unwrap();
    let ws = commands::load_workspace_inner(&state);
    assert_eq!(ws.apis.len(), 1);
    assert_eq!(ws.errors.len(), 1);
    assert!(ws.errors[0].path.ends_with("broken.json"));
}

#[test]
fn save_api_rejects_an_invalid_document_and_leaves_the_file_untouched() {
    let (_d, state) = state_with("demo.json", GOOD);
    let before = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    let diags = commands::save_api_inner(&state, "demo", "{ nope").unwrap();
    assert!(diags.iter().any(|d| d.severity == "error"));
    let after = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn save_api_rejects_a_document_whose_id_does_not_match_the_file() {
    let (_d, state) = state_with("demo.json", GOOD);
    let renamed = GOOD.replace(r#""id":"demo""#, r#""id":"other""#);
    let err = commands::save_api_inner(&state, "demo", &renamed).unwrap_err();
    assert!(err.contains("id"), "unhelpful error: {err}");
}

#[test]
fn save_api_writes_canonical_json() {
    let (_d, state) = state_with("demo.json", GOOD);
    let diags = commands::save_api_inner(&state, "demo", GOOD).unwrap();
    assert!(diags.iter().all(|d| d.severity != "error"));
    let written = std::fs::read_to_string(state.paths.apis_dir().join("demo.json")).unwrap();
    assert!(written.ends_with('\n'));
    assert!(written.contains("\n  \"id\": \"demo\""));
}

#[test]
fn lint_reports_the_chained_default_warning() {
    let text = r#"{"schemaVersion":1,"id":"d","name":"D","baseUrl":"https://x","endpoints":[
      {"id":"token","name":"T","method":"POST","path":"/token"},
      {"id":"biz","name":"B","method":"GET","path":"/b","auth":{"type":"chained","source":{"endpoint":"token"},
       "inject":{"into":"header","name":"Authorization","template":"Bearer {{value}}"}}}]}"#;
    let diags = commands::lint_inner(text);
    assert!(diags.iter().any(|d| d.severity == "warning"));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p reqchain-desktop`
Expected: FAIL — crate has no library target.

- [ ] **Step 3: Give the crate a library target**

Add to `apps/desktop/src-tauri/Cargo.toml`:

```toml
[lib]
name = "reqchain_desktop"
path = "src/lib.rs"

[[bin]]
name = "reqchain-desktop"
path = "src/main.rs"
```

`apps/desktop/src-tauri/src/lib.rs`:

```rust
pub mod commands;
pub mod dto;
pub mod state;
```

- [ ] **Step 4: Implement state and DTOs**

`state.rs`:

```rust
use reqchain_core::paths::Paths;
use reqchain_core::secrets::Secrets;
use reqchain_core::store::Workspace;
use std::sync::Mutex;

pub struct AppState {
    pub paths: Paths,
    pub secrets: Mutex<Secrets>,
    pub workspace: Mutex<Workspace>,
}

impl AppState {
    pub fn new(paths: Paths) -> AppState {
        let secrets = Secrets::load(&paths);
        let workspace = Workspace::load(&paths);
        AppState {
            paths,
            secrets: Mutex::new(secrets),
            workspace: Mutex::new(workspace),
        }
    }

    /// Re-reads secrets and workspace from disk. Called on startup and on every
    /// watcher event.
    pub fn reload(&self) {
        *self.workspace.lock().unwrap() = Workspace::load(&self.paths);
        *self.secrets.lock().unwrap() = Secrets::load(&self.paths);
    }
}
```

`dto.rs` — the structs listed under **Interfaces**, plus:

```rust
impl From<&reqchain_core::validate::Diagnostic> for DiagnosticDto {
    fn from(d: &reqchain_core::validate::Diagnostic) -> DiagnosticDto {
        DiagnosticDto {
            severity: match d.severity {
                reqchain_core::validate::Severity::Error => "error",
                reqchain_core::validate::Severity::Warning => "warning",
            }
            .into(),
            path: d.path.clone(),
            message: d.message.clone(),
        }
    }
}
```

`ApiDto::text` carries the API's file as written on disk so the editor opens the real bytes, not a re-serialization.

`EndpointDto::auth_kind` is `"inherit" | "none" | "basic" | "bearer" | "header" | "computed" | "chained"`, so the sidebar can badge chained endpoints without shipping credentials to the frontend.

- [ ] **Step 5: Implement the commands**

Each `#[tauri::command]` is a one-line wrapper over a plain `*_inner` function that takes `&AppState` — the inner functions are what the tests call.

```rust
pub fn load_workspace_inner(state: &AppState) -> WorkspaceDto { /* map Workspace → DTO */ }

pub fn lint_inner(text: &str) -> Vec<DiagnosticDto> {
    reqchain_core::validate::validate_text(text).iter().map(Into::into).collect()
}

pub fn save_api_inner(state: &AppState, api_id: &str, text: &str) -> Result<Vec<DiagnosticDto>, String> {
    let diags: Vec<DiagnosticDto> = lint_inner(text);
    if diags.iter().any(|d| d.severity == "error") {
        return Ok(diags);          // reported, nothing written
    }
    let api = reqchain_core::model::Api::from_json(text).map_err(|e| e.to_string())?;
    if api.id != api_id {
        return Err(format!(
            "this file holds API `{api_id}`; the edited document has id `{}`. Rename via the file, not the editor.",
            api.id
        ));
    }
    let path = state.paths.apis_dir().join(format!("{api_id}.json"));
    std::fs::write(&path, api.to_json_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    state.reload();
    Ok(diags)
}
```

Register them in `main.rs` with `.manage(AppState::new(Paths::from_env()))` and `.invoke_handler(tauri::generate_handler![load_workspace, lint, save_api])`.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p reqchain-desktop`
Expected: PASS (6 tests).

- [ ] **Step 7: Lint and commit**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add -A && git commit -m "feat(desktop): expose workspace loading, linting and saving over IPC"
```

---

### Task 5: Backend — run, preview, curl and history commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/src/dto.rs`, `apps/desktop/src-tauri/src/main.rs`
- Test: `apps/desktop/src-tauri/tests/run.rs`

**Interfaces:**
- Consumes: `chain::{Executor, RunError}`, `exec::{Runner, RunResult, AuthStep}`, `cache::TokenCache`, `shell::to_shell_command`, `history`.
- Produces:
  ```rust
  pub struct RunDto { pub status: u16, pub elapsed_ms: u64, pub size_bytes: usize,
                      pub headers: Vec<[String; 2]>, pub body: String, pub body_is_binary: bool,
                      pub effective: EffectiveDto, pub auth_trace: Vec<AuthStepDto> }
  pub struct EffectiveDto { pub method: String, pub url: String,
                            pub headers: Vec<[String; 2]>, pub body: Option<String> }
  pub struct AuthStepDto { pub endpoint_id: String, pub request: Option<EffectiveDto>,
                           pub status: u16, pub body: String, pub from_cache: bool }
  run_endpoint(state, api_id, endpoint_id, env: Option<String>) -> Result<RunDto, String>
  preview_endpoint(state, api_id, endpoint_id, env) -> Result<EffectiveDto, String>
  curl_command(state, api_id, endpoint_id, env) -> Result<String, String>
  history(state, api_id, endpoint_id) -> Vec<HistoryEntry>
  ```
  The `Executor` lives in `AppState` behind a `tokio::sync::Mutex` so its in-memory `TokenCache` survives between runs — the UI must not re-fetch a token per click.

**Masking contract (the reason this task exists):** before any `EffectiveRequest`, auth-trace request, auth-trace body or curl string leaves Rust, it goes through

```rust
let mut mask = executor.derived_values();
mask.extend(secrets.values().cloned());
let shown = req.masked_with(&mask, &executor.auth_headers());
```

and every auth-step `body` and the response `body` are additionally passed through a string-level redaction of the same `mask` list. An auth response body contains the token verbatim; showing it raw would defeat every other mask in the app.

- [ ] **Step 1: Write the failing tests**

The first test is written out in full; the remaining seven follow its shape — same
`state_for(&mock)` helper, same direct call into the `*_inner` functions.

```rust
// apps/desktop/src-tauri/tests/run.rs — against wiremock, no network
use reqchain_core::paths::Paths;
use reqchain_desktop::{commands, state::AppState};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tok-abcdef-1234567890";

/// Workspace with a `token` endpoint and a `biz` endpoint chained onto it.
fn state_for(base_url: &str, extra: &str) -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    std::fs::create_dir_all(paths.apis_dir()).unwrap();
    let text = format!(
        r#"{{"schemaVersion":1,"id":"demo","name":"Demo","baseUrl":"{base_url}"{extra},
        "endpoints":[
          {{"id":"token","name":"Token","method":"POST","path":"/token"}},
          {{"id":"biz","name":"Biz","method":"GET","path":"/biz",
            "auth":{{"type":"chained","source":{{"endpoint":"token"}},
              "inject":{{"into":"header","name":"Authorization","template":"Bearer {{{{value}}}}"}}}}}}]}}"#
    );
    std::fs::write(paths.apis_dir().join("demo.json"), text).unwrap();
    (dir, AppState::new(paths))
}

#[tokio::test]
async fn a_chained_run_returns_the_business_response_and_an_auth_trace() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "access_token": TOKEN, "expires_in": 3600 }),
        ))
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/biz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&mock)
        .await;

    let (_d, state) = state_for(&mock.uri(), "");
    let run = commands::run_endpoint_inner(&state, "demo", "biz", None)
        .await
        .unwrap();

    assert_eq!(run.status, 200);
    assert_eq!(run.body, "ok");
    assert_eq!(run.auth_trace.len(), 1);
    assert_eq!(run.auth_trace[0].endpoint_id, "token");
    assert!(!run.auth_trace[0].from_cache);
}

#[tokio::test]
async fn the_token_never_appears_unmasked_anywhere_in_the_run_dto() {
    // assert the literal token string is absent from: effective headers, auth_trace[0].body,
    // auth_trace[0].request headers, and the serialized RunDto as a whole.
}

#[tokio::test]
async fn a_secret_never_appears_unmasked_in_the_curl_command() { /* {{secret:PASS}} in a header */ }

#[tokio::test]
async fn preview_endpoint_does_not_send_the_business_request() {
    // wiremock: /biz expects 0 calls; /token expects 1
}

#[tokio::test]
async fn a_second_run_reuses_the_cached_token() {
    // wiremock: /token expects exactly 1 call across two run_endpoint calls
}

#[tokio::test]
async fn a_run_appends_one_history_entry() { /* history(...).len() == 1 after one run */ }

#[tokio::test]
async fn store_bodies_false_keeps_bodies_out_of_the_history_file() {
    // api with "history":{"storeBodies":false}; assert the jsonl has no body field
}

#[tokio::test]
async fn a_transport_failure_becomes_a_readable_error_not_a_panic() { /* unroutable port */ }
```

Each test builds an `AppState` over a `tempfile` root with a workspace file pointing at the wiremock `base_url`, then calls the `*_inner` function directly.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p reqchain-desktop --test run`
Expected: FAIL — the functions do not exist.

- [ ] **Step 3: Implement**

Add to `AppState`:

```rust
pub executor: tokio::sync::Mutex<Executor>,
```

initialised as `Executor::new(Runner::new(), TokenCache::in_memory(), secrets.clone())`. If `TokenCache` has no in-memory constructor, add one to core (`TokenCache::in_memory()` — same type, no file path, `save` is a no-op) rather than persisting the GUI's tokens to the CLI's cache file; the spec says the UI holds the cache in memory.

`run_endpoint_inner` then: look up the API (error if missing), `executor.run(api, endpoint_id, env)`, map `RunError` to a readable `String`, build the masked `RunDto`, append a history entry using `api.history.map(|h| h.store_bodies).unwrap_or(true)`, and return. A failed history append is a non-fatal warning logged to stderr, never an error shown instead of the response.

`body_is_binary` is true when the response body is not valid UTF-8; `body` is then the empty string and the frontend shows type and size only (spec §8).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p reqchain-desktop --test run`
Expected: PASS (8 tests).

- [ ] **Step 5: Lint and commit**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add -A && git commit -m "feat(desktop): run endpoints over IPC with masked effective requests and history"
```

---

### Task 6: Backend — hot reload event

**Files:**
- Create: `apps/desktop/src-tauri/src/watcher.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `core::watch::watch`, `AppState::reload`.
- Produces: the Tauri event `workspace-changed` (no payload — the frontend re-invokes `load_workspace`), and a `Watcher` kept alive for the app's lifetime in managed state.

- [ ] **Step 1: Implement**

```rust
use reqchain_core::watch::{self, Watcher};
use tauri::{AppHandle, Emitter, Manager};

pub fn start(app: &AppHandle) -> Option<Watcher> {
    let handle = app.clone();
    let paths = app.state::<crate::state::AppState>().paths.clone();
    match watch::watch(&paths, move || {
        handle.state::<crate::state::AppState>().reload();
        let _ = handle.emit("workspace-changed", ());
    }) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("file watching disabled: {e}");
            None
        }
    }
}
```

Call it from `.setup(|app| { let w = watcher::start(app.handle()); app.manage(w); Ok(()) })`. A watcher that fails to start degrades to a manual-reload app; it never prevents startup.

- [ ] **Step 2: Verify by hand**

Run `pnpm --dir apps/desktop tauri dev`, then in another shell `touch ~/.config/reqchain/workspace/apis/*.json`. Expected: the dev console shows one `workspace-changed` per touch, debounced.

- [ ] **Step 3: Lint and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add -A && git commit -m "feat(desktop): emit workspace-changed when a file on disk changes"
```

---

### Task 7: Frontend — shell, state and sidebar

**Files:**
- Create: `apps/desktop/src/lib/ipc.ts`, `apps/desktop/src/lib/state.svelte.ts`, `apps/desktop/src/lib/Sidebar.svelte`, `apps/desktop/src/app.css`
- Modify: `apps/desktop/src/App.svelte`, `apps/desktop/src/main.ts`

**Interfaces:**
- Consumes: `load_workspace`, `lint`, `save_api` from Task 4.
- Produces:
  ```ts
  // state.svelte.ts
  export const ui = $state({
    workspace: { apis: [], errors: [] } as WorkspaceDto,
    selected: null as { apiId: string; endpointId: string } | null,
    env: {} as Record<string, string | null>,   // apiId → environment name
    buffers: {} as Record<string, string>,      // apiId → unsaved editor text
    search: "",
    response: null as RunDto | null,
    running: false,
    error: null as string | null,
  });
  export async function reload(): Promise<void>;
  export function selectedApi(): ApiDto | undefined;
  export function selectedEndpoint(): EndpointDto | undefined;
  ```

- [ ] **Step 1: Type the IPC layer**

`ipc.ts` mirrors the Rust DTOs exactly (camelCase) and exports one thin async function per command. No logic beyond `invoke`.

- [ ] **Step 2: Build the sidebar**

Contents, top to bottom: a search input bound to `ui.search`; per API, a header row with the API name and a `<select>` of its environments (bound to `ui.env[apiId]`, default the first environment or none); under it, the endpoints whose id, name or path matches the search, each showing method badge, name, and a `chain` marker when `authKind === "chained"`; at the bottom, a red block listing `ui.workspace.errors` as `basename — message`.

Selecting an endpoint sets `ui.selected` and clears `ui.response`.

- [ ] **Step 3: Lay out the window**

`App.svelte`: a three-column CSS grid — sidebar (280px, resizable is out of scope), request panel (1fr), response panel (1fr). Calls `reload()` on mount. Dark-on-light system palette, system font stack, no CSS framework.

- [ ] **Step 4: Verify in the dev window**

Run `pnpm --dir apps/desktop tauri dev` with at least two API files in the workspace, one of them deliberately broken. Expected: both valid APIs listed with their endpoints, the broken one listed under errors with its message, search filters endpoints as you type, the environment selector lists the file's environments.

- [ ] **Step 5: Commit**

```bash
pnpm --dir apps/desktop build
git add -A && git commit -m "feat(desktop): add the app shell and the API/endpoint sidebar"
```

---

### Task 8: Frontend — request panel with editor and save

**Files:**
- Create: `apps/desktop/src/lib/RequestPanel.svelte`, `apps/desktop/src/lib/Editor.svelte`
- Modify: `apps/desktop/src/App.svelte`

**Interfaces:**
- Consumes: `ui.buffers`, `save_api`, `lint`, `preview_endpoint`.
- Produces: an editing surface for the selected API's file.

**Design ruling (recorded, since the spec is terse here):** the request panel edits **the API's JSON file**, not a form-per-field. One CodeMirror 6 JSON editor over the whole file, plus a read-only summary header for the selected endpoint (method, resolved URL from `preview_endpoint`, auth kind). Rationale: the file is the model, agents write it, and a form would need a second serializer that could disagree with `Api::to_json_string`. The summary header is what makes it usable; the editor is what makes it honest.

- [ ] **Step 1: Wrap CodeMirror**

`Editor.svelte`: props `value: string`, `onChange: (v: string) => void`, `diagnostics: DiagnosticDto[]`. Sets up `EditorView` with `json()`, line numbers, and a `$effect` that replaces the document when `value` changes from outside (hot reload) but not while the user is typing.

- [ ] **Step 2: Build the panel**

Header: method badge, the effective URL from `preview_endpoint` (debounced 300 ms, errors shown inline in grey, never thrown), auth kind, and a `Save` button enabled only when `ui.buffers[apiId] !== api.text`. Below: the editor. Below that: the diagnostics strip from `lint` on every change (debounced 300 ms) — errors red, warnings amber, each `path — message`.

`Save` calls `save_api`; if it returns error-severity diagnostics, nothing was written and the strip says so; if it throws, the message is shown as-is.

- [ ] **Step 3: Verify in the dev window**

Edit an endpoint's path, watch the header URL update, save, confirm the file on disk changed and the sidebar reflects it. Introduce a syntax error, confirm Save refuses and the file is unchanged.

- [ ] **Step 4: Commit**

```bash
pnpm --dir apps/desktop build
git add -A && git commit -m "feat(desktop): edit and save API files from the request panel"
```

---

### Task 9: Frontend — response panel

**Files:**
- Create: `apps/desktop/src/lib/ResponsePanel.svelte`
- Modify: `apps/desktop/src/App.svelte`

**Interfaces:**
- Consumes: `run_endpoint`, `curl_command`, `ui.response`.

- [ ] **Step 1: Build it**

Top bar: `Send` button (disabled while `ui.running`), status code (green 2xx, amber 3xx, red 4xx/5xx), elapsed ms, size in bytes/KB, and a `Copy as curl` button that invokes `curl_command` and writes to the clipboard.

Tabs: **Body** (pretty-printed when the body parses as JSON, raw otherwise, with a toggle and a case-insensitive find box that highlights matches), **Headers** (name/value table), **Effective** (method, url, headers, body of `response.effective` — already masked by Rust), **Auth** (one collapsible block per `authTrace` step: endpoint id, `from cache` badge, status, its effective request, its response body).

A binary response (`bodyIsBinary`) renders `content-type` and size with the note that binary bodies are not displayed.

An error from `run_endpoint` replaces the panel with the message and nothing else — no partial response.

- [ ] **Step 2: Verify against the acceptance case**

With a live or mocked gateway, select the chained endpoint on a cold cache and press Send. Expected: a 2xx, and an Auth tab showing the token request with `from cache: false`. Press Send again: the Auth tab shows `from cache: true` and no second token call.

- [ ] **Step 3: Commit**

```bash
pnpm --dir apps/desktop build
git add -A && git commit -m "feat(desktop): show status, body, headers, effective request and auth trace"
```

---

### Task 10: Frontend — hot reload, shortcuts, history

**Files:**
- Create: `apps/desktop/src/lib/HistoryPanel.svelte`
- Modify: `apps/desktop/src/App.svelte`, `apps/desktop/src/lib/state.svelte.ts`

**Interfaces:**
- Consumes: the `workspace-changed` event, `history`.

- [ ] **Step 1: Hot reload that preserves state**

Subscribe with `listen("workspace-changed", ...)` and call `reload()`. Rules, all of them spec §9:
- `ui.selected` survives; if the endpoint no longer exists, keep showing it with a `removed` marker instead of clearing the panel.
- `ui.buffers[apiId]` survives — an unsaved buffer is never overwritten by a reload. If the on-disk text changed while a buffer is dirty, show a `changed on disk` badge with a `Discard mine` action.
- `ui.response` survives.

- [ ] **Step 2: Shortcuts**

A single `keydown` listener on `window`: `Ctrl+Enter` sends the selected endpoint, `Ctrl+K` focuses the sidebar search and selects its text, `Ctrl+S` saves the current buffer. Each calls `preventDefault` only when it acts.

- [ ] **Step 3: History**

A collapsed strip under the response panel listing the last runs for the selected endpoint: time, status, elapsed, size. Clicking one expands it into the response bodies it stored, or a `bodies not stored` note when `storeBodies` is false. Refreshed after every run.

- [ ] **Step 4: Verify the three behaviours by hand**

Edit a file in an external editor while the app is open with a dirty buffer in another API; confirm the sidebar updates and the buffer survives. Press each shortcut. Run an endpoint three times and confirm three history rows.

- [ ] **Step 5: Final checks and commit**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
pnpm --dir apps/desktop build
git add -A && git commit -m "feat(desktop): hot reload, keyboard shortcuts and run history"
```

---

## Deliberately not in phase 2

- `.deb`, `.desktop`, the real icon, the install script — phase 3.
- GNOME keyring for secrets; `secrets.json` remains the only source.
- A form-based request editor (see the ruling in Task 8).
- An MCP server (spec §10 explains why it is not built at all).
- Editing `secrets.json` from the UI: secrets stay a file the user owns.
- Resizable panes, themes, tabs for multiple simultaneous responses.

## Phase-1 debt to fold in while here

These were recorded during phase 1 and become visible once a GUI shows an auth trace. Fix them in the task that first touches the surface, and say so in the commit:

- `chain::RunError` excerpts and `AuthStep` fields carry unmasked material; Task 5's masking contract must cover them at the boundary, and the core-side fix belongs with it.
- A cache hit reports `status: 0` as a sentinel — Task 5's DTO should map it to `null`/`from cache` rather than showing `0` in the UI.
