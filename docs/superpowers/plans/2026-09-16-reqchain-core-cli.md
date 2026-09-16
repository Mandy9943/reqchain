# reqchain Core + CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `reqchain-core` and the `reqchain` CLI so a chained-auth HTTP request defined in a JSON file runs end to end — token fetched, cached, injected, refreshed on 401 — with a published schema, a `SPEC.md` an agent can write files from, and a validator.

**Architecture:** One Rust workspace with two crates. `reqchain-core` owns every rule (model, loading, variables, expressions, auth, execution, cache, validation) and has no UI or CLI dependency; `reqchain-cli` is a thin `clap` shell over it. The Tauri app in a later plan is a second thin shell over the same crate. Every behaviour is tested headless against a `wiremock` server — no network, no real credentials.

**Tech Stack:** Rust 2021, tokio, reqwest, serde/serde_json, serde_json_path, regex, chrono, md-5/sha1/sha2, base64, jsonschema, clap, thiserror, wiremock.

**Spec:** `docs/superpowers/specs/2026-09-16-reqchain-design.md`

## Global Constraints

- Rust edition 2021, toolchain stable (rustup). Target: Ubuntu 24.04 x86_64.
- Crate versions are floors, use the latest compatible: `tokio 1`, `reqwest 0.12` (features `json`, `multipart`, `rustls-tls`, default-features off), `serde 1` (derive), `serde_json 1` (feature `preserve_order` NOT enabled — struct field order is the canonical order), `serde_json_path 0.7`, `regex 1`, `chrono 0.4`, `md-5 0.10`, `sha1 0.10`, `sha2 0.10`, `base64 0.22`, `jsonschema 0.26`, `clap 4` (derive), `thiserror 2`, `wiremock 0.6`, `tempfile 3`.
- `schemaVersion` is `1` everywhere. A file with any other value is a load error, never a silent migration.
- Workspace files are written with 2-space indent, struct-declaration key order, and a trailing newline. No timestamps, ids, counters or run metadata are ever written into `workspace/`.
- Secrets live only in the secrets file (`$REQCHAIN_DIR/secrets.json`, mode 0600). They are never written into `workspace/`, never into history, and are masked as `***` in any effective-request or shell-command export unless explicitly revealed.
- All paths are rooted at `REQCHAIN_DIR` if set, else `~/.config/reqchain`. No test ever touches the real directory — every test uses `tempfile::TempDir`.
- No `eval`, no scripting engine. The expression language of spec §6 is the only computation.
- Every error type carries an actionable message: what was found, what was expected, and where (file, JSON path, variable name).

---

### Task 1: Workspace scaffold and data model

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`
- Create: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/core/src/model.rs`
- Test: `crates/core/tests/model_roundtrip.rs`
- Create: `tests/fixtures/example-gateway-test.json`

**Interfaces:**
- Consumes: nothing.
- Produces: `reqchain_core::model::{Api, Environment, Endpoint, Method, Body, Auth, AuthExtract, AuthTtl, AuthInject, ChainSource, SCHEMA_VERSION, LoadError}`. `Api::from_json(&str) -> Result<Api, LoadError>`, `Api::to_json_string(&self) -> String` (2-space indent, trailing newline), `Api::endpoint(&self, id: &str) -> Option<&Endpoint>`.

- [ ] **Step 1: Create the workspace files**

`Cargo.toml`:

```toml
[workspace]
members = ["crates/core", "crates/cli"]
resolver = "2"

[workspace.package]
edition = "2021"
version = "0.1.0"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "fs"] }
```

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
```

`.gitignore`:

```
/target
```

`crates/core/Cargo.toml`:

```toml
[package]
name = "reqchain-core"
edition.workspace = true
version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write the failing test**

`crates/core/tests/model_roundtrip.rs`:

```rust
use reqchain_core::model::Api;

const FIXTURE: &str = include_str!("../../../tests/fixtures/example-gateway-test.json");

#[test]
fn parses_and_reserializes_byte_identically() {
    let api = Api::from_json(FIXTURE).expect("fixture must parse");
    assert_eq!(api.id, "example-gateway-test");
    assert_eq!(api.endpoints.len(), 2);
    assert_eq!(api.to_json_string(), FIXTURE);
}

#[test]
fn rejects_unknown_schema_version() {
    let bad = FIXTURE.replace("\"schemaVersion\": 1", "\"schemaVersion\": 99");
    let err = Api::from_json(&bad).unwrap_err().to_string();
    assert!(err.contains("schemaVersion"), "got: {err}");
    assert!(err.contains("99"), "got: {err}");
}

#[test]
fn rejects_unknown_fields() {
    let bad = FIXTURE.replace("\"baseUrl\"", "\"base_url\"");
    let err = Api::from_json(&bad).unwrap_err().to_string();
    assert!(err.contains("base_url"), "got: {err}");
}
```

Create `tests/fixtures/example-gateway-test.json` with exactly this content (2-space indent, trailing newline — this fixture is also the spec §14 acceptance file):

```json
{
  "schemaVersion": 1,
  "id": "example-gateway-test",
  "name": "Example Gateway (test)",
  "baseUrl": "https://api-manager.example.com",
  "variables": {},
  "environments": [
    {
      "name": "test",
      "variables": {
        "person_id": "12345678"
      }
    }
  ],
  "auth": {
    "type": "none"
  },
  "endpoints": [
    {
      "id": "token",
      "name": "Token",
      "method": "POST",
      "path": "/token",
      "headers": {},
      "query": {},
      "variables": {},
      "auth": {
        "type": "basic",
        "username": "{{secret:GW_USER}}",
        "password": "{{secret:GW_PASS}}"
      },
      "body": {
        "type": "form",
        "fields": {
          "grant_type": "client_credentials"
        }
      }
    },
    {
      "id": "repairQuery",
      "name": "Repair Query",
      "method": "POST",
      "path": "/repairs/1.0",
      "headers": {},
      "query": {},
      "variables": {},
      "auth": {
        "type": "chained",
        "source": {
          "endpoint": "token"
        },
        "extract": {
          "from": "body",
          "jsonPath": "$.access_token"
        },
        "ttl": {
          "from": "body",
          "jsonPath": "$.expires_in",
          "unit": "seconds"
        },
        "inject": {
          "into": "header",
          "name": "Authorization",
          "template": "Bearer {{value}}"
        },
        "retryOn": [401, 403]
      },
      "body": {
        "type": "json",
        "content": {
          "RepairQuery_In": {
            "PersonId": "{{person_id}}"
          }
        }
      }
    }
  ]
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test model_roundtrip`
Expected: FAIL — `unresolved import reqchain_core::model`.

- [ ] **Step 4: Implement the model**

`crates/core/src/lib.rs`:

```rust
pub mod model;
```

`crates/core/src/model.rs` — field declaration order IS the serialization order, so keep it exactly as written:

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

pub type Vars = BTreeMap<String, String>;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("invalid JSON at line {line}, column {column}: {message}")]
    Json { line: usize, column: usize, message: String },
    #[error("unsupported schemaVersion {found}, this build understands {expected}")]
    SchemaVersion { found: u32, expected: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Api {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(default)]
    pub variables: Vars,
    #[serde(default)]
    pub environments: Vec<Environment>,
    #[serde(default = "Auth::none")]
    pub auth: Auth,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub variables: Vars,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub method: Method,
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default)]
    pub variables: Vars,
    #[serde(default = "Auth::inherit")]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method { Get, Post, Put, Patch, Delete, Head, Options }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Body {
    Json { content: serde_json::Value },
    Form { fields: BTreeMap<String, String> },
    Multipart { fields: BTreeMap<String, String>, #[serde(default)] files: BTreeMap<String, String> },
    Text { content: String },
    Xml { content: String },
    Binary { path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Auth {
    Inherit,
    None,
    Basic { username: String, password: String },
    Bearer { token: String },
    Header { headers: BTreeMap<String, String> },
    Computed { name: String, expression: String },
    Chained {
        source: ChainSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extract: Option<AuthExtract>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ttl: Option<AuthTtl>,
        inject: AuthInject,
        #[serde(default = "default_retry_on", rename = "retryOn")]
        retry_on: Vec<u16>,
    },
}

impl Auth {
    pub fn none() -> Self { Auth::None }
    pub fn inherit() -> Self { Auth::Inherit }
}

fn default_retry_on() -> Vec<u16> { vec![401, 403] }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainSource { pub endpoint: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "lowercase", deny_unknown_fields)]
pub enum AuthExtract {
    Body {
        #[serde(default, rename = "jsonPath", skip_serializing_if = "Option::is_none")] json_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")] xpath: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")] regex: Option<String>,
    },
    Header {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")] regex: Option<String>,
    },
    Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "lowercase", deny_unknown_fields)]
pub enum AuthTtl {
    Body {
        #[serde(rename = "jsonPath")] json_path: String,
        #[serde(default = "seconds_unit")] unit: String,
    },
    Fixed { seconds: u64 },
    Absolute {
        #[serde(rename = "jsonPath")] json_path: String,
    },
}

fn seconds_unit() -> String { "seconds".to_string() }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "into", rename_all = "lowercase", deny_unknown_fields)]
pub enum AuthInject {
    Header { name: String, template: String },
    Query { name: String, #[serde(default = "value_template")] template: String },
    Body { pointer: String, #[serde(default = "value_template")] template: String },
}

fn value_template() -> String { "{{value}}".to_string() }

impl Api {
    pub fn from_json(text: &str) -> Result<Api, LoadError> {
        let api: Api = serde_json::from_str(text).map_err(|e| LoadError::Json {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        })?;
        if api.schema_version != SCHEMA_VERSION {
            return Err(LoadError::SchemaVersion { found: api.schema_version, expected: SCHEMA_VERSION });
        }
        Ok(api)
    }

    pub fn to_json_string(&self) -> String {
        let mut out = serde_json::to_string_pretty(self).expect("model is serializable");
        out.push('\n');
        out
    }

    pub fn endpoint(&self, id: &str) -> Option<&Endpoint> {
        self.endpoints.iter().find(|e| e.id == id)
    }
}
```

Note: `serde_json::to_string_pretty` uses 2-space indent, which is the required format.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test model_roundtrip`
Expected: 3 passed. If the round-trip test fails, diff the output against the fixture and fix the **fixture** to match struct order — never add ordering hacks to the model.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore crates tests && git commit -m "feat(core): add API/endpoint data model with stable JSON round-trip"
```

---

### Task 2: Workspace loader with per-file error isolation

**Files:**
- Create: `crates/core/src/paths.rs`, `crates/core/src/store.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/store.rs`

**Interfaces:**
- Consumes: `model::{Api, LoadError}`.
- Produces: `paths::Paths` with `from_env()`, `at(path)`, `workspace()`, `apis_dir()`, `secrets_file()`, `cache_file()`, `history_dir()`; `store::Workspace::load(&Paths) -> Workspace` with fields `apis: Vec<Api>`, `errors: Vec<FileError>`, method `api(&self, id: &str) -> Option<&Api>`; `store::FileError { path: PathBuf, message: String }`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/store.rs`:

```rust
use reqchain_core::{paths::Paths, store::Workspace};
use std::fs;

fn workspace_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Paths) {
    let dir = tempfile::tempdir().unwrap();
    let apis = dir.path().join("workspace/apis");
    fs::create_dir_all(&apis).unwrap();
    for (name, content) in files {
        fs::write(apis.join(name), content).unwrap();
    }
    let paths = Paths::at(dir.path());
    (dir, paths)
}

const GOOD: &str = include_str!("../../../tests/fixtures/example-gateway-test.json");

#[test]
fn loads_valid_apis() {
    let (_d, paths) = workspace_with(&[("a.json", GOOD)]);
    let ws = Workspace::load(&paths);
    assert_eq!(ws.apis.len(), 1);
    assert!(ws.errors.is_empty());
}

#[test]
fn one_broken_file_does_not_hide_the_others() {
    let (_d, paths) = workspace_with(&[("good.json", GOOD), ("broken.json", "{ nope")]);
    let ws = Workspace::load(&paths);
    assert_eq!(ws.apis.len(), 1, "the valid API must still load");
    assert_eq!(ws.errors.len(), 1);
    let err = &ws.errors[0];
    assert!(err.path.ends_with("broken.json"));
    assert!(err.message.contains("line 1"), "error must locate the problem: {}", err.message);
}

#[test]
fn broken_file_is_left_untouched_on_disk() {
    let (_d, paths) = workspace_with(&[("broken.json", "{ nope")]);
    let _ = Workspace::load(&paths);
    let still = std::fs::read_to_string(paths.apis_dir().join("broken.json")).unwrap();
    assert_eq!(still, "{ nope", "loader must never rewrite a file");
}

#[test]
fn missing_workspace_is_empty_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::load(&Paths::at(dir.path()));
    assert!(ws.apis.is_empty());
    assert!(ws.errors.is_empty());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test store`
Expected: FAIL — `unresolved import reqchain_core::paths`.

- [ ] **Step 3: Implement paths and the loader**

`crates/core/src/paths.rs`:

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths { pub root: PathBuf }

impl Paths {
    pub fn from_env() -> Paths {
        let root = std::env::var_os("REQCHAIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
                home.join(".config/reqchain")
            });
        Paths { root }
    }
    pub fn at(root: impl AsRef<Path>) -> Paths { Paths { root: root.as_ref().to_path_buf() } }
    pub fn workspace(&self) -> PathBuf { self.root.join("workspace") }
    pub fn apis_dir(&self) -> PathBuf { self.workspace().join("apis") }
    pub fn secrets_file(&self) -> PathBuf { self.root.join("secrets.json") }
    pub fn cache_file(&self) -> PathBuf { self.root.join("cache/tokens.json") }
    pub fn history_dir(&self) -> PathBuf { self.root.join("history") }
}
```

`crates/core/src/store.rs`:

```rust
use crate::model::Api;
use crate::paths::Paths;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FileError { pub path: PathBuf, pub message: String }

#[derive(Debug, Default)]
pub struct Workspace { pub apis: Vec<Api>, pub errors: Vec<FileError> }

impl Workspace {
    pub fn load(paths: &Paths) -> Workspace {
        let mut ws = Workspace::default();
        let dir = paths.apis_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else { return ws };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for path in files {
            match std::fs::read_to_string(&path) {
                Err(e) => ws.errors.push(FileError { path, message: e.to_string() }),
                Ok(text) => match Api::from_json(&text) {
                    Ok(api) => ws.apis.push(api),
                    Err(e) => ws.errors.push(FileError { path, message: e.to_string() }),
                },
            }
        }
        ws
    }

    pub fn api(&self, id: &str) -> Option<&Api> { self.apis.iter().find(|a| a.id == id) }
}
```

Add `pub mod paths;` and `pub mod store;` to `crates/core/src/lib.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test store`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core && git commit -m "feat(core): load workspace APIs with per-file error isolation"
```

---

### Task 3: Variable and secret resolution

**Files:**
- Create: `crates/core/src/vars.rs`, `crates/core/src/secrets.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/vars.rs`
- Create: `tests/fixtures/precedence.json`

**Interfaces:**
- Consumes: `model::{Api, Endpoint, Vars}`, `paths::Paths`.
- Produces: `secrets::Secrets` with `empty()`, `from_map(iter)`, `load(&Paths)`, `get(&str) -> Option<&str>`, `values() -> impl Iterator<Item = &String>`; `vars::Scope::new(&Api, &Endpoint, Option<&str>, &Secrets) -> Scope`, `Scope::lookup`, `Scope::interpolate(&str) -> Result<String, VarError>`, `Scope::interpolate_masked(&str) -> String`, `Scope::secret_values(&self) -> Vec<String>`; `vars::VarError::{Unknown { name }, UnknownSecret { name }}`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/vars.rs`:

```rust
use reqchain_core::model::Api;
use reqchain_core::secrets::Secrets;
use reqchain_core::vars::Scope;

fn api() -> Api {
    Api::from_json(include_str!("../../../tests/fixtures/precedence.json")).unwrap()
}

#[test]
fn endpoint_beats_environment_beats_api() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &Secrets::empty());
    // who = defined at all three levels
    assert_eq!(scope.interpolate("{{who}}").unwrap(), "endpoint");
    // where = defined at environment and api
    assert_eq!(scope.interpolate("{{where}}").unwrap(), "environment");
    // what = defined only at api
    assert_eq!(scope.interpolate("{{what}}").unwrap(), "api");
}

#[test]
fn environment_selection_changes_resolution() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let scope = Scope::new(&api, ep, Some("prod"), &Secrets::empty());
    assert_eq!(scope.interpolate("{{where}}").unwrap(), "prod-environment");
}

#[test]
fn interpolates_multiple_occurrences_and_surrounding_text() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &Secrets::empty());
    assert_eq!(scope.interpolate("a/{{what}}/b/{{what}}").unwrap(), "a/api/b/api");
}

#[test]
fn unknown_variable_is_an_actionable_error() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &Secrets::empty());
    let err = scope.interpolate("x{{nope}}y").unwrap_err().to_string();
    assert!(err.contains("nope"), "got: {err}");
}

#[test]
fn resolves_secrets_and_masks_them() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let secrets = Secrets::from_map([("GW_PASS".to_string(), "hunter2".to_string())]);
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    assert_eq!(scope.interpolate("{{secret:GW_PASS}}").unwrap(), "hunter2");
    assert_eq!(scope.interpolate_masked("{{secret:GW_PASS}}"), "***");
}

#[test]
fn missing_secret_names_the_store() {
    let api = api();
    let ep = api.endpoint("probe").unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &Secrets::empty());
    let err = scope.interpolate("{{secret:ABSENT}}").unwrap_err().to_string();
    assert!(err.contains("ABSENT"), "got: {err}");
    assert!(err.contains("secret"), "got: {err}");
}
```

Create `tests/fixtures/precedence.json`:

```json
{
  "schemaVersion": 1,
  "id": "precedence",
  "name": "Precedence",
  "baseUrl": "https://example.test",
  "variables": {
    "what": "api",
    "where": "api",
    "who": "api"
  },
  "environments": [
    {
      "name": "test",
      "variables": {
        "where": "environment",
        "who": "environment"
      }
    },
    {
      "name": "prod",
      "variables": {
        "where": "prod-environment"
      }
    }
  ],
  "auth": {
    "type": "none"
  },
  "endpoints": [
    {
      "id": "probe",
      "name": "Probe",
      "method": "GET",
      "path": "/probe",
      "headers": {},
      "query": {},
      "variables": {
        "who": "endpoint"
      },
      "auth": {
        "type": "inherit"
      }
    }
  ]
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test vars`
Expected: FAIL — `unresolved import reqchain_core::vars`.

- [ ] **Step 3: Implement the secret store**

`crates/core/src/secrets.rs`:

```rust
use crate::paths::Paths;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct Secrets { map: BTreeMap<String, String> }

impl Secrets {
    pub fn empty() -> Secrets { Secrets::default() }

    pub fn from_map(entries: impl IntoIterator<Item = (String, String)>) -> Secrets {
        Secrets { map: entries.into_iter().collect() }
    }

    /// A missing store is not an error: a workspace with no secrets is valid.
    pub fn load(paths: &Paths) -> Secrets {
        let Ok(text) = std::fs::read_to_string(paths.secrets_file()) else { return Secrets::default() };
        let map = serde_json::from_str::<BTreeMap<String, String>>(&text).unwrap_or_default();
        Secrets { map }
    }

    pub fn get(&self, name: &str) -> Option<&str> { self.map.get(name).map(|s| s.as_str()) }
    pub fn values(&self) -> impl Iterator<Item = &String> { self.map.values() }
}
```

- [ ] **Step 4: Implement the scope**

`crates/core/src/vars.rs`:

```rust
use crate::model::{Api, Endpoint, Vars};
use crate::secrets::Secrets;

#[derive(Debug, thiserror::Error)]
pub enum VarError {
    #[error("unknown variable `{name}` — define it on the endpoint, the active environment, or the API")]
    Unknown { name: String },
    #[error("unknown secret `{name}` — add it to the secret store")]
    UnknownSecret { name: String },
}

pub struct Scope<'a> {
    endpoint_vars: &'a Vars,
    env_vars: Option<&'a Vars>,
    api_vars: &'a Vars,
    secrets: &'a Secrets,
}

impl<'a> Scope<'a> {
    pub fn new(api: &'a Api, endpoint: &'a Endpoint, env: Option<&str>, secrets: &'a Secrets) -> Scope<'a> {
        let env_vars = env
            .and_then(|name| api.environments.iter().find(|e| e.name == name))
            .map(|e| &e.variables);
        Scope { endpoint_vars: &endpoint.variables, env_vars, api_vars: &api.variables, secrets }
    }

    /// Precedence, most specific first: endpoint -> active environment -> API.
    pub fn lookup(&self, name: &str) -> Option<&str> {
        self.endpoint_vars.get(name)
            .or_else(|| self.env_vars.and_then(|v| v.get(name)))
            .or_else(|| self.api_vars.get(name))
            .map(|s| s.as_str())
    }

    pub fn interpolate(&self, input: &str) -> Result<String, VarError> { self.expand(input, false) }

    pub fn interpolate_masked(&self, input: &str) -> String {
        self.expand(input, true).unwrap_or_else(|_| input.to_string())
    }

    pub fn secret_values(&self) -> Vec<String> { self.secrets.values().cloned().collect() }

    fn expand(&self, input: &str, mask: bool) -> Result<String, VarError> {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                out.push_str(&rest[start..]);
                return Ok(out);
            };
            let name = after[..end].trim();
            if let Some(secret) = name.strip_prefix("secret:") {
                let secret = secret.trim();
                let value = self.secrets.get(secret)
                    .ok_or_else(|| VarError::UnknownSecret { name: secret.to_string() })?;
                out.push_str(if mask { "***" } else { value });
            } else {
                let value = self.lookup(name)
                    .ok_or_else(|| VarError::Unknown { name: name.to_string() })?;
                out.push_str(value);
            }
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }
}
```

Add `pub mod secrets;` and `pub mod vars;` to `lib.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test vars`
Expected: 6 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/core tests/fixtures && git commit -m "feat(core): resolve variables endpoint-first with masked secrets"
```

---

### Task 4: Expression language

**Files:**
- Create: `crates/core/src/expr.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`
- Test: `crates/core/tests/expr.rs`

**Interfaces:**
- Consumes: `vars::Scope`.
- Produces: `expr::eval(input: &str, scope: &Scope) -> Result<String, ExprError>`; `expr::SUPPORTED: &[&str]`; `ExprError::{UnknownFunction { name, supported }, Syntax { message }, Arity { name, expected, found }, Var(VarError)}`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/expr.rs`:

```rust
use reqchain_core::expr;
use reqchain_core::model::Api;
use reqchain_core::secrets::Secrets;
use reqchain_core::vars::Scope;

macro_rules! eval {
    ($src:expr) => {{
        let api = Api::from_json(include_str!("../../../tests/fixtures/precedence.json")).unwrap();
        let secrets = Secrets::empty();
        let ep = api.endpoint("probe").unwrap();
        let scope = Scope::new(&api, ep, Some("test"), &secrets);
        expr::eval($src, &scope)
    }};
}

#[test]
fn hashes_and_encodes() {
    assert_eq!(eval!(r#"md5("abc")"#).unwrap(), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(eval!(r#"sha1("abc")"#).unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(eval!(r#"sha256("abc")"#).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(eval!(r#"base64("abc")"#).unwrap(), "YWJj");
}

#[test]
fn concatenates_strings_and_variables() {
    assert_eq!(eval!(r#""a" + "b" + {{what}}"#).unwrap(), "abapi");
}

#[test]
fn now_formats_current_local_date() {
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    assert_eq!(eval!(r#"now("YYYYMMDD")"#).unwrap(), today);
}

#[test]
fn nests_calls_over_concatenation() {
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let expected = format!("{:x}", md5::compute(format!("{today}api").as_bytes()));
    assert_eq!(eval!(r#"md5(now("YYYYMMDD") + {{what}})"#).unwrap(), expected);
}

#[test]
fn unknown_function_lists_the_supported_set() {
    let err = eval!(r#"sha512("abc")"#).unwrap_err().to_string();
    assert!(err.contains("sha512"), "got: {err}");
    assert!(err.contains("sha256"), "error must list supported functions: {err}");
}

#[test]
fn unbalanced_parens_are_a_syntax_error() {
    assert!(eval!(r#"md5("abc""#).is_err());
}

#[test]
fn wrong_arity_is_reported() {
    let err = eval!(r#"md5("a", "b")"#).unwrap_err().to_string();
    assert!(err.contains("md5"), "got: {err}");
}
```

Add to `crates/core/Cargo.toml`:

```toml
[dependencies]
md-5 = "0.10"
sha1 = "0.10"
sha2 = "0.10"
base64 = "0.22"
chrono = "0.4"

[dev-dependencies]
md5 = "0.7"
chrono = "0.4"
```

(`md-5` is the RustCrypto implementation used by the library; `md5` is a tiny one-call crate used only to compute expected values in tests.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test expr`
Expected: FAIL — `unresolved import reqchain_core::expr`.

- [ ] **Step 3: Implement the parser and evaluator**

`crates/core/src/expr.rs` — recursive descent over the §6 grammar, no eval:

```rust
use crate::vars::{Scope, VarError};
use base64::Engine;

pub const SUPPORTED: &[&str] = &["md5", "sha1", "sha256", "base64", "now"];

#[derive(Debug, thiserror::Error)]
pub enum ExprError {
    #[error("unknown function `{name}` — supported: {supported}")]
    UnknownFunction { name: String, supported: String },
    #[error("expression syntax error: {message}")]
    Syntax { message: String },
    #[error("`{name}` takes {expected} argument(s), found {found}")]
    Arity { name: String, expected: usize, found: usize },
    #[error(transparent)]
    Var(#[from] VarError),
}

pub fn eval(input: &str, scope: &Scope) -> Result<String, ExprError> {
    let mut p = Parser { s: input.as_bytes(), i: 0, scope };
    let value = p.expr()?;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(ExprError::Syntax { message: format!("unexpected trailing input at byte {}", p.i) });
    }
    Ok(value)
}

struct Parser<'a, 'b> { s: &'a [u8], i: usize, scope: &'a Scope<'b> }

impl<'a, 'b> Parser<'a, 'b> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() { self.i += 1 }
    }

    fn expr(&mut self) -> Result<String, ExprError> {
        let mut acc = self.term()?;
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b'+' {
                self.i += 1;
                acc.push_str(&self.term()?);
            } else {
                return Ok(acc);
            }
        }
    }

    fn term(&mut self) -> Result<String, ExprError> {
        self.skip_ws();
        if self.i >= self.s.len() {
            return Err(ExprError::Syntax { message: "unexpected end of expression".into() });
        }
        match self.s[self.i] {
            b'"' => self.string(),
            b'{' => self.varref(),
            _ => self.funcall(),
        }
    }

    fn string(&mut self) -> Result<String, ExprError> {
        self.i += 1; // opening quote
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i] != b'"' { self.i += 1 }
        if self.i >= self.s.len() {
            return Err(ExprError::Syntax { message: "unterminated string literal".into() });
        }
        let out = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        self.i += 1; // closing quote
        Ok(out)
    }

    fn varref(&mut self) -> Result<String, ExprError> {
        let rest = std::str::from_utf8(&self.s[self.i..]).unwrap_or_default();
        if !rest.starts_with("{{") {
            return Err(ExprError::Syntax { message: "expected a variable reference".into() });
        }
        let end = rest.find("}}").ok_or(ExprError::Syntax { message: "unterminated variable reference".into() })?;
        let token = &rest[..end + 2];
        self.i += token.len();
        Ok(self.scope.interpolate(token)?)
    }

    fn funcall(&mut self) -> Result<String, ExprError> {
        let start = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
            self.i += 1
        }
        let name = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        if name.is_empty() {
            return Err(ExprError::Syntax { message: format!("expected a function name at byte {start}") });
        }
        self.skip_ws();
        if self.i >= self.s.len() || self.s[self.i] != b'(' {
            return Err(ExprError::Syntax { message: format!("expected `(` after `{name}`") });
        }
        self.i += 1;
        let mut args = Vec::new();
        loop {
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b')' { self.i += 1; break }
            args.push(self.expr()?);
            self.skip_ws();
            if self.i < self.s.len() && self.s[self.i] == b',' { self.i += 1; continue }
            if self.i < self.s.len() && self.s[self.i] == b')' { self.i += 1; break }
            return Err(ExprError::Syntax { message: format!("unterminated argument list for `{name}`") });
        }
        apply(&name, &args)
    }
}

fn one<'a>(name: &str, args: &'a [String]) -> Result<&'a str, ExprError> {
    match args {
        [a] => Ok(a.as_str()),
        _ => Err(ExprError::Arity { name: name.to_string(), expected: 1, found: args.len() }),
    }
}

fn apply(name: &str, args: &[String]) -> Result<String, ExprError> {
    match name {
        "md5" => {
            use md5::Digest;
            Ok(format!("{:x}", md5::Md5::digest(one(name, args)?.as_bytes())))
        }
        "sha1" => {
            use sha1::Digest;
            Ok(format!("{:x}", sha1::Sha1::digest(one(name, args)?.as_bytes())))
        }
        "sha256" => {
            use sha2::Digest;
            Ok(format!("{:x}", sha2::Sha256::digest(one(name, args)?.as_bytes())))
        }
        "base64" => Ok(base64::engine::general_purpose::STANDARD.encode(one(name, args)?.as_bytes())),
        "now" => Ok(chrono::Local::now().format(&to_chrono_format(one(name, args)?)).to_string()),
        other => Err(ExprError::UnknownFunction {
            name: other.to_string(),
            supported: SUPPORTED.join(", "),
        }),
    }
}

/// Maps the documented, user-facing format tokens to chrono's, longest token first.
fn to_chrono_format(fmt: &str) -> String {
    const MAP: &[(&str, &str)] = &[
        ("YYYY", "%Y"), ("MM", "%m"), ("DD", "%d"),
        ("HH", "%H"), ("mm", "%M"), ("ss", "%S"),
    ];
    let mut out = String::with_capacity(fmt.len());
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for (from, to) in MAP {
            if rest.starts_with(from) {
                out.push_str(to);
                rest = &rest[from.len()..];
                continue 'outer;
            }
        }
        let ch = rest.chars().next().unwrap();
        if ch == '%' { out.push('%') } // escape a literal percent for chrono
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}
```

The RustCrypto crate `md-5` exposes the module path `md5::Md5`, which is why the `use md5::Digest;` line above refers to the library dependency, not the dev-dependency of the same name. If the two clash during the build, rename the dev-dependency in `Cargo.toml` as `md5_simple = { package = "md5", version = "0.7" }` and update the test imports.

Add `pub mod expr;` to `lib.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test expr`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core && git commit -m "feat(core): add closed expression language for computed headers"
```

---

### Task 5: Request building and execution

**Files:**
- Create: `crates/core/src/request.rs`, `crates/core/src/exec.rs`, `crates/core/src/shell.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`
- Test: `crates/core/tests/exec.rs`
- Create: `tests/fixtures/exec.json`

**Interfaces:**
- Consumes: `model`, `vars::Scope`, `expr`.
- Produces: `request::{EffectiveRequest { method, url, headers, body }, EffectiveBody, BuildError}`, `EffectiveRequest::masked(&self, secrets: &[String]) -> EffectiveRequest`, `request::build(&Api, &Endpoint, &Scope) -> Result<EffectiveRequest, BuildError>` (auth NOT applied — Task 6 applies it); `exec::{Runner, RunResult, AuthStep, ExecError}`, `Runner::new()`, `Runner::send(&self, &EffectiveRequest) -> Result<RunResult, ExecError>`, `RunResult::body_text()`; `shell::to_shell_command(&EffectiveRequest) -> String` (the "copy as curl" export).

- [ ] **Step 1: Write the failing test**

`crates/core/tests/exec.rs`:

```rust
use reqchain_core::exec::Runner;
use reqchain_core::model::Api;
use reqchain_core::secrets::Secrets;
use reqchain_core::shell;
use reqchain_core::vars::Scope;
use reqchain_core::{auth, request};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn api_with_base(base: &str) -> Api {
    let text = include_str!("../../../tests/fixtures/exec.json").replace("BASE_URL", base);
    Api::from_json(&text).unwrap()
}

#[tokio::test]
async fn sends_interpolated_json_body_and_reports_metrics() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo/1.0"))
        .and(header("x-doc", "12345678"))
        .and(body_string_contains("12345678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let api = api_with_base(&server.uri());
    let ep = api.endpoint("echo").unwrap();
    let secrets = Secrets::empty();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let req = request::build(&api, ep, &scope).unwrap();
    assert_eq!(req.url, format!("{}/echo/1.0?trace=on", server.uri()));

    let res = Runner::new().send(&req).await.unwrap();
    assert_eq!(res.status, 200);
    assert!(res.size_bytes > 0);
    assert!(res.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")));
}

#[tokio::test]
async fn computed_header_is_evaluated() {
    let server = MockServer::start().await;
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let expected = format!("{:x}", md5::compute(format!("{today}12345678").as_bytes()));
    Mock::given(method("GET"))
        .and(path("/library/user"))
        .and(header("hash", expected.as_str()))
        .and(header("user", "12345678"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let api = api_with_base(&server.uri());
    let ep = api.endpoint("library").unwrap();
    let secrets = Secrets::from_map([("API_TOKEN".into(), "t0k".into())]);
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();
    assert_eq!(Runner::new().send(&req).await.unwrap().status, 200);
}

#[test]
fn shell_export_masks_secret_values() {
    let api = api_with_base("https://example.test");
    let ep = api.endpoint("library").unwrap();
    let secrets = Secrets::from_map([("API_TOKEN".into(), "sup3rsecret".into())]);
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();
    let out = shell::to_shell_command(&req.masked(&scope.secret_values()));
    assert!(out.starts_with("curl "));
    assert!(!out.contains("sup3rsecret"), "export must not leak secret values: {out}");
    assert!(out.contains("***"));
}
```

Create `tests/fixtures/exec.json`:

```json
{
  "schemaVersion": 1,
  "id": "exec",
  "name": "Exec",
  "baseUrl": "BASE_URL",
  "variables": {},
  "environments": [
    {
      "name": "test",
      "variables": {
        "person_id": "12345678"
      }
    }
  ],
  "auth": {
    "type": "none"
  },
  "endpoints": [
    {
      "id": "echo",
      "name": "Echo",
      "method": "POST",
      "path": "/echo/1.0",
      "headers": {
        "x-doc": "{{person_id}}"
      },
      "query": {
        "trace": "on"
      },
      "variables": {},
      "auth": {
        "type": "none"
      },
      "body": {
        "type": "json",
        "content": {
          "doc": "{{person_id}}"
        }
      }
    },
    {
      "id": "library",
      "name": "Library user",
      "method": "GET",
      "path": "/library/user",
      "headers": {
        "token": "{{secret:API_TOKEN}}",
        "user": "{{person_id}}"
      },
      "query": {},
      "variables": {},
      "auth": {
        "type": "computed",
        "name": "hash",
        "expression": "md5(now(\"YYYYMMDD\") + {{person_id}})"
      }
    }
  ]
}
```

Add to `crates/core/Cargo.toml`:

```toml
[dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "multipart", "rustls-tls"] }
tokio.workspace = true

[dev-dependencies]
wiremock = "0.6"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test exec`
Expected: FAIL — `unresolved import reqchain_core::request`.

- [ ] **Step 3: Implement request building**

`crates/core/src/request.rs`:

```rust
use crate::model::{Api, Body, Endpoint, Method};
use crate::vars::{Scope, VarError};

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Var(#[from] VarError),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectiveBody {
    Text { content_type: String, content: String },
    Form { fields: Vec<(String, String)> },
    Multipart { fields: Vec<(String, String)>, files: Vec<(String, String)> },
    Binary { path: String },
}

#[derive(Debug, Clone)]
pub struct EffectiveRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<EffectiveBody>,
}

impl EffectiveRequest {
    /// Replaces every secret value with `***`, for display and shell export.
    pub fn masked(&self, secrets: &[String]) -> EffectiveRequest {
        let mask = |s: &String| {
            let mut out = s.clone();
            for secret in secrets {
                if !secret.is_empty() { out = out.replace(secret.as_str(), "***") }
            }
            out
        };
        EffectiveRequest {
            method: self.method,
            url: mask(&self.url),
            headers: self.headers.iter().map(|(k, v)| (k.clone(), mask(v))).collect(),
            body: self.body.as_ref().map(|b| match b {
                EffectiveBody::Text { content_type, content } =>
                    EffectiveBody::Text { content_type: content_type.clone(), content: mask(content) },
                EffectiveBody::Form { fields } =>
                    EffectiveBody::Form { fields: fields.iter().map(|(k, v)| (k.clone(), mask(v))).collect() },
                other => other.clone(),
            }),
        }
    }
}

pub fn build(api: &Api, endpoint: &Endpoint, scope: &Scope) -> Result<EffectiveRequest, BuildError> {
    let base = scope.interpolate(&api.base_url)?;
    let path = scope.interpolate(&endpoint.path)?;
    let mut url = format!("{}{}", base.trim_end_matches('/'), path);

    if !endpoint.query.is_empty() {
        let mut pairs = Vec::new();
        for (k, v) in &endpoint.query {
            pairs.push(format!("{}={}", urlencode(k), urlencode(&scope.interpolate(v)?)));
        }
        url.push('?');
        url.push_str(&pairs.join("&"));
    }

    let mut headers = Vec::new();
    for (k, v) in &endpoint.headers {
        headers.push((k.clone(), scope.interpolate(v)?));
    }

    let body = match &endpoint.body {
        None => None,
        Some(Body::Json { content }) => Some(EffectiveBody::Text {
            content_type: "application/json".into(),
            content: scope.interpolate(&content.to_string())?,
        }),
        Some(Body::Text { content }) => Some(EffectiveBody::Text {
            content_type: "text/plain".into(),
            content: scope.interpolate(content)?,
        }),
        Some(Body::Xml { content }) => Some(EffectiveBody::Text {
            content_type: "application/xml".into(),
            content: scope.interpolate(content)?,
        }),
        Some(Body::Form { fields }) => {
            let mut out = Vec::new();
            for (k, v) in fields { out.push((k.clone(), scope.interpolate(v)?)) }
            Some(EffectiveBody::Form { fields: out })
        }
        Some(Body::Multipart { fields, files }) => {
            let mut f = Vec::new();
            for (k, v) in fields { f.push((k.clone(), scope.interpolate(v)?)) }
            let mut fl = Vec::new();
            for (k, v) in files { fl.push((k.clone(), scope.interpolate(v)?)) }
            Some(EffectiveBody::Multipart { fields: f, files: fl })
        }
        Some(Body::Binary { path }) => Some(EffectiveBody::Binary { path: scope.interpolate(path)? }),
    };

    Ok(EffectiveRequest { method: endpoint.method, url, headers, body })
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
```

- [ ] **Step 4: Implement the runner**

`crates/core/src/exec.rs`:

```rust
use crate::model::Method;
use crate::request::{EffectiveBody, EffectiveRequest};
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("cannot read file `{path}`: {message}")]
    File { path: String, message: String },
}

#[derive(Debug, Clone)]
pub struct AuthStep {
    pub endpoint_id: String,
    pub request: Option<EffectiveRequest>,
    pub status: u16,
    pub body: String,
    pub from_cache: bool,
}

#[derive(Debug)]
pub struct RunResult {
    pub status: u16,
    pub elapsed_ms: u128,
    pub size_bytes: usize,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub effective: EffectiveRequest,
    pub auth_trace: Vec<AuthStep>,
}

impl RunResult {
    pub fn body_text(&self) -> String { String::from_utf8_lossy(&self.body).into_owned() }
}

#[derive(Clone)]
pub struct Runner { client: reqwest::Client }

impl Default for Runner { fn default() -> Self { Runner::new() } }

impl Runner {
    pub fn new() -> Runner {
        Runner { client: reqwest::Client::builder().build().expect("client builds") }
    }

    pub async fn send(&self, req: &EffectiveRequest) -> Result<RunResult, ExecError> {
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
            Method::Head => reqwest::Method::HEAD,
            Method::Options => reqwest::Method::OPTIONS,
        };
        let mut rb = self.client.request(method, &req.url);
        for (k, v) in &req.headers { rb = rb.header(k, v) }
        rb = match &req.body {
            None => rb,
            Some(EffectiveBody::Text { content_type, content }) =>
                rb.header("content-type", content_type).body(content.clone()),
            Some(EffectiveBody::Form { fields }) => rb.form(&fields.clone()),
            Some(EffectiveBody::Multipart { fields, files }) => {
                let mut form = reqwest::multipart::Form::new();
                for (k, v) in fields { form = form.text(k.clone(), v.clone()) }
                for (k, path) in files {
                    let bytes = std::fs::read(path)
                        .map_err(|e| ExecError::File { path: path.clone(), message: e.to_string() })?;
                    form = form.part(k.clone(), reqwest::multipart::Part::bytes(bytes).file_name(path.clone()));
                }
                rb.multipart(form)
            }
            Some(EffectiveBody::Binary { path }) => {
                let bytes = std::fs::read(path)
                    .map_err(|e| ExecError::File { path: path.clone(), message: e.to_string() })?;
                rb.body(bytes)
            }
        };

        let started = Instant::now();
        let res = rb.send().await.map_err(|e| ExecError::Transport(e.to_string()))?;
        let status = res.status().as_u16();
        let headers = res.headers().iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
            .collect();
        let body = res.bytes().await.map_err(|e| ExecError::Transport(e.to_string()))?.to_vec();
        Ok(RunResult {
            status,
            elapsed_ms: started.elapsed().as_millis(),
            size_bytes: body.len(),
            headers,
            body,
            effective: req.clone(),
            auth_trace: Vec::new(),
        })
    }
}
```

`crates/core/src/shell.rs` — renders the effective request as a shell command for the clipboard. It only ever builds a string; it never executes anything:

```rust
use crate::request::{EffectiveBody, EffectiveRequest};

pub fn to_shell_command(req: &EffectiveRequest) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
    let mut parts = vec![
        "curl".to_string(),
        "-X".into(),
        format!("{:?}", req.method).to_uppercase(),
        q(&req.url),
    ];
    for (k, v) in &req.headers {
        parts.push("-H".into());
        parts.push(q(&format!("{k}: {v}")));
    }
    match &req.body {
        None => {}
        Some(EffectiveBody::Text { content_type, content }) => {
            parts.push("-H".into());
            parts.push(q(&format!("content-type: {content_type}")));
            parts.push("--data".into());
            parts.push(q(content));
        }
        Some(EffectiveBody::Form { fields }) => {
            for (k, v) in fields {
                parts.push("--data-urlencode".into());
                parts.push(q(&format!("{k}={v}")));
            }
        }
        Some(EffectiveBody::Multipart { fields, files }) => {
            for (k, v) in fields { parts.push("-F".into()); parts.push(q(&format!("{k}={v}"))) }
            for (k, p) in files { parts.push("-F".into()); parts.push(q(&format!("{k}=@{p}"))) }
        }
        Some(EffectiveBody::Binary { path }) => {
            parts.push("--data-binary".into());
            parts.push(q(&format!("@{path}")));
        }
    }
    parts.join(" ")
}
```

Add `pub mod request;`, `pub mod exec;`, `pub mod shell;` to `lib.rs`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reqchain-core --test exec`
Expected: `sends_interpolated_json_body_and_reports_metrics` PASSES; the other two FAIL because `auth::apply_static` does not exist yet — that is Task 6.

- [ ] **Step 6: Commit**

```bash
git add crates/core tests/fixtures && git commit -m "feat(core): build and send effective requests with shell export"
```

---

### Task 6: Static auth types

**Files:**
- Create: `crates/core/src/auth.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/auth_static.rs`
- Create: `tests/fixtures/auth-static.json`

**Interfaces:**
- Consumes: `model::Auth`, `request::EffectiveRequest`, `expr`, `vars::Scope`.
- Produces: `auth::resolve(&Api, &Endpoint) -> &Auth` (applies `inherit`); `auth::apply_static(&Api, &Endpoint, &Scope, &mut EffectiveRequest) -> Result<(), AuthError>` — handles `none`/`basic`/`bearer`/`header`/`computed`, and is a **no-op for `chained`** (Task 8 owns it); `auth::AuthError::{Var(VarError), Expr(ExprError)}`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/auth_static.rs`:

```rust
use reqchain_core::{auth, model::Api, request, secrets::Secrets, vars::Scope};

const FIXTURE: &str = include_str!("../../../tests/fixtures/auth-static.json");

fn header_of(endpoint: &str, secrets: Secrets, name: &str) -> Option<String> {
    let api = Api::from_json(FIXTURE).unwrap();
    let ep = api.endpoint(endpoint).unwrap();
    let scope = Scope::new(&api, ep, Some("test"), &secrets);
    let mut req = request::build(&api, ep, &scope).unwrap();
    auth::apply_static(&api, ep, &scope, &mut req).unwrap();
    req.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
}

#[test]
fn basic_auth_is_base64_of_user_colon_pass() {
    let secrets = Secrets::from_map([
        ("GW_USER".into(), "alice".into()),
        ("GW_PASS".into(), "hunter2".into()),
    ]);
    // base64("alice:hunter2")
    assert_eq!(header_of("basic", secrets, "authorization").as_deref(), Some("Basic YWxpY2U6aHVudGVyMg=="));
}

#[test]
fn bearer_auth_prefixes_the_token() {
    assert_eq!(header_of("bearer", Secrets::empty(), "authorization").as_deref(), Some("Bearer static-token"));
}

#[test]
fn custom_headers_are_added() {
    assert_eq!(header_of("custom", Secrets::empty(), "x-api-key").as_deref(), Some("abc123"));
    assert_eq!(header_of("custom", Secrets::empty(), "x-user").as_deref(), Some("12345678"));
}

#[test]
fn inherit_uses_the_api_level_auth() {
    assert_eq!(header_of("inheriting", Secrets::empty(), "authorization").as_deref(), Some("Bearer api-level"));
}

#[test]
fn none_overrides_inherited_auth() {
    assert_eq!(header_of("opted-out", Secrets::empty(), "authorization"), None);
}

#[test]
fn chained_is_left_for_the_chain_resolver() {
    assert_eq!(header_of("chained", Secrets::empty(), "authorization"), None);
}
```

Create `tests/fixtures/auth-static.json`: `id` `auth-static`, `baseUrl` `https://example.test`,
API-level `auth` `{"type":"bearer","token":"api-level"}`, one environment `test` with
`person_id: "12345678"`, and six endpoints — all `GET`, `path` `/x`, empty
`headers`/`query`/`variables`, no `body`:

| id | auth |
|---|---|
| `basic` | `{"type":"basic","username":"{{secret:GW_USER}}","password":"{{secret:GW_PASS}}"}` |
| `bearer` | `{"type":"bearer","token":"static-token"}` |
| `custom` | `{"type":"header","headers":{"x-api-key":"abc123","x-user":"{{person_id}}"}}` |
| `inheriting` | `{"type":"inherit"}` |
| `opted-out` | `{"type":"none"}` |
| `chained` | `{"type":"chained","source":{"endpoint":"bearer"},"inject":{"into":"header","name":"Authorization","template":"Bearer {{value}}"}}` |

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test auth_static`
Expected: FAIL — `unresolved import reqchain_core::auth`.

- [ ] **Step 3: Implement static auth**

`crates/core/src/auth.rs`:

```rust
use crate::expr::{self, ExprError};
use crate::model::{Api, Auth, Endpoint};
use crate::request::EffectiveRequest;
use crate::vars::{Scope, VarError};
use base64::Engine;

const NONE: Auth = Auth::None;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(transparent)]
    Var(#[from] VarError),
    #[error(transparent)]
    Expr(#[from] ExprError),
}

/// Resolves `inherit` to the API-level auth. An API whose own auth is `inherit`
/// is treated as `none` — there is nothing above it to inherit from.
pub fn resolve<'a>(api: &'a Api, endpoint: &'a Endpoint) -> &'a Auth {
    match &endpoint.auth {
        Auth::Inherit => match &api.auth {
            Auth::Inherit => &NONE,
            other => other,
        },
        other => other,
    }
}

fn set_header(req: &mut EffectiveRequest, name: &str, value: String) {
    req.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    req.headers.push((name.to_string(), value));
}

pub fn apply_static(
    api: &Api,
    endpoint: &Endpoint,
    scope: &Scope,
    req: &mut EffectiveRequest,
) -> Result<(), AuthError> {
    match resolve(api, endpoint) {
        Auth::Inherit | Auth::None => {}
        Auth::Basic { username, password } => {
            let user = scope.interpolate(username)?;
            let pass = scope.interpolate(password)?;
            let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            set_header(req, "Authorization", format!("Basic {encoded}"));
        }
        Auth::Bearer { token } => {
            set_header(req, "Authorization", format!("Bearer {}", scope.interpolate(token)?));
        }
        Auth::Header { headers } => {
            for (k, v) in headers {
                let value = scope.interpolate(v)?;
                set_header(req, k, value);
            }
        }
        Auth::Computed { name, expression } => {
            let value = expr::eval(expression, scope)?;
            set_header(req, name, value);
        }
        // Chained auth needs I/O; `chain::Executor` owns it.
        Auth::Chained { .. } => {}
    }
    Ok(())
}
```

Add `pub mod auth;` to `lib.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test auth_static --test exec`
Expected: all 9 pass, including the two `exec` tests that were blocked on `apply_static`.

- [ ] **Step 5: Commit**

```bash
git add crates/core tests/fixtures && git commit -m "feat(core): apply basic, bearer, custom-header and computed auth"
```

---

### Task 7: Token cache with TTL and skew

**Files:**
- Create: `crates/core/src/cache.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/cache.rs`

**Interfaces:**
- Consumes: `model::Auth`, `paths::Paths`.
- Produces: `cache::TokenCache` with `in_memory()`, `persistent(&Paths)`, `key(api_id: &str, auth: &Auth, scope_fingerprint: &str) -> String`, `get(&self, key: &str, now: u64) -> Option<String>`, `put(&mut self, key: &str, value: String, expires_at: Option<u64>)`, `invalidate(&mut self, key: &str)`, `save(&self) -> std::io::Result<()>`; `cache::SKEW_SECONDS: u64 = 30`; `cache::now_unix() -> u64`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/cache.rs`:

```rust
use reqchain_core::cache::{TokenCache, SKEW_SECONDS};
use reqchain_core::model::Auth;
use reqchain_core::paths::Paths;

fn auth_a() -> Auth { Auth::Bearer { token: "a".into() } }
fn auth_b() -> Auth { Auth::Bearer { token: "b".into() } }

#[test]
fn returns_a_live_token() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), Some(1_000 + 600));
    assert_eq!(c.get(&k, 1_000).as_deref(), Some("tok"));
}

#[test]
fn treats_a_token_as_expired_skew_seconds_early() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), Some(1_000));
    assert!(c.get(&k, 1_000 - SKEW_SECONDS + 1).is_none(), "must expire {SKEW_SECONDS}s early");
    assert!(c.get(&k, 1_000 - SKEW_SECONDS - 5).is_some());
}

#[test]
fn a_token_without_ttl_never_expires() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), None);
    assert_eq!(c.get(&k, u64::MAX / 2).as_deref(), Some("tok"));
}

#[test]
fn changing_the_auth_definition_changes_the_key() {
    assert_ne!(TokenCache::key("api", &auth_a(), "test"), TokenCache::key("api", &auth_b(), "test"));
}

#[test]
fn changing_the_environment_changes_the_key() {
    assert_ne!(TokenCache::key("api", &auth_a(), "test"), TokenCache::key("api", &auth_a(), "prod"));
}

#[test]
fn invalidate_removes_the_entry() {
    let mut c = TokenCache::in_memory();
    let k = TokenCache::key("api", &auth_a(), "test");
    c.put(&k, "tok".into(), Some(9_999_999_999));
    c.invalidate(&k);
    assert!(c.get(&k, 1_000).is_none());
}

#[test]
fn persists_across_instances_with_owner_only_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let k = TokenCache::key("api", &auth_a(), "test");
    {
        let mut c = TokenCache::persistent(&paths);
        c.put(&k, "tok".into(), Some(9_999_999_999));
        c.save().unwrap();
    }
    let reloaded = TokenCache::persistent(&paths);
    assert_eq!(reloaded.get(&k, 1_000).as_deref(), Some("tok"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(paths.cache_file()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "token cache must be owner-only");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test cache`
Expected: FAIL — `unresolved import reqchain_core::cache`.

- [ ] **Step 3: Implement the cache**

`crates/core/src/cache.rs`:

```rust
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
pub struct TokenCache { entries: BTreeMap<String, Entry>, file: Option<PathBuf> }

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TokenCache {
    pub fn in_memory() -> TokenCache { TokenCache::default() }

    pub fn persistent(paths: &Paths) -> TokenCache {
        let file = paths.cache_file();
        let entries = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        TokenCache { entries, file: Some(file) }
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
        self.entries.insert(key.to_string(), Entry { value, expires_at });
    }

    pub fn invalidate(&mut self, key: &str) { self.entries.remove(key); }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(file) = &self.file else { return Ok(()) };
        if let Some(parent) = file.parent() { std::fs::create_dir_all(parent)? }
        let text = serde_json::to_string_pretty(&self.entries).unwrap_or_else(|_| "{}".into());
        std::fs::write(file, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
```

Add `pub mod cache;` to `lib.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test cache`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core && git commit -m "feat(core): cache tokens by auth fingerprint with TTL and clock skew"
```

---

### Task 8: Chained auth — resolution, cycles, refresh

**Files:**
- Create: `crates/core/src/chain.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`
- Test: `crates/core/tests/chain.rs`
- Create: `tests/fixtures/chain.json`, `tests/fixtures/chain-cycle.json`, `tests/fixtures/chain-missing.json`, `tests/fixtures/chain-default-extract.json`

**Interfaces:**
- Consumes: everything above.
- Produces: `chain::Executor::new(Runner, TokenCache, Secrets) -> Executor`; `Executor::run(&mut self, &Api, endpoint_id: &str, env: Option<&str>) -> Result<RunResult, RunError>` — the single entry point the CLI and the UI both call; `Executor::cache_mut(&mut self) -> &mut TokenCache`; `chain::RunError::{Build, Exec, Auth, Chain { message }}`; `chain::MAX_DEPTH: usize = 5`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/chain.rs`:

```rust
use reqchain_core::{cache::TokenCache, chain::Executor, exec::Runner, model::Api, secrets::Secrets};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const CHAIN: &str = include_str!("../../../tests/fixtures/chain.json");

fn api(base: &str, fixture: &str) -> Api {
    Api::from_json(&fixture.replace("BASE_URL", base)).unwrap()
}

/// Issues a new token on every call so a test can tell refreshes apart.
struct TokenIssuer { calls: Arc<AtomicUsize>, expires_in: u64 }

impl Respond for TokenIssuer {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": format!("token-{n}"),
            "expires_in": self.expires_in,
        }))
    }
}

fn executor() -> Executor {
    Executor::new(
        Runner::new(),
        TokenCache::in_memory(),
        Secrets::from_map([
            ("GW_USER".into(), "alice".into()),
            ("GW_PASS".into(), "hunter2".into()),
        ]),
    )
}

#[tokio::test]
async fn fetches_the_token_without_running_the_source_endpoint_by_hand() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .and(header("authorization", "Bearer token-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let res = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(res.auth_trace.len(), 1, "the auth request must be visible");
    assert_eq!(res.auth_trace[0].endpoint_id, "token");
    assert!(res.auth_trace[0].body.contains("access_token"));
}

#[tokio::test]
async fn reuses_the_cached_token_on_a_second_run() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    ex.run(&api, "business", Some("test")).await.unwrap();
    let second = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1, "token must be reused");
    assert!(second.auth_trace[0].from_cache);
}

#[tokio::test]
async fn refreshes_a_token_whose_ttl_has_passed() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    // expires_in below the 30s skew => already expired the moment it is cached
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 1 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    ex.run(&api, "business", Some("test")).await.unwrap();
    ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2, "expired token must be refetched");
}

#[tokio::test]
async fn retries_once_with_a_fresh_token_on_401() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business")).and(header("authorization", "Bearer token-1"))
        .respond_with(ResponseTemplate::new(401)).mount(&server).await;
    Mock::given(method("POST")).and(path("/business")).and(header("authorization", "Bearer token-2"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let res = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn gives_up_after_one_retry_on_persistent_401() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(TokenIssuer { calls: calls.clone(), expires_in: 3600 })
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/business"))
        .respond_with(ResponseTemplate::new(401)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    let res = ex.run(&api, "business", Some("test")).await.unwrap();
    assert_eq!(res.status, 401, "the 401 is reported, not retried forever");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "exactly one refresh");
}

#[tokio::test]
async fn extracts_from_a_response_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).insert_header("x-token", "header-token"))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/header-business"))
        .and(header("authorization", "Bearer header-token"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(ex.run(&api, "header-business", Some("test")).await.unwrap().status, 200);
}

#[tokio::test]
async fn injects_into_a_query_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "qtok"})))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/query-business"))
        .and(query_param("access_token", "qtok"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = api(&server.uri(), CHAIN);
    let mut ex = executor();
    assert_eq!(ex.run(&api, "query-business", Some("test")).await.unwrap().status, 200);
}

#[tokio::test]
async fn detects_a_cycle_and_names_the_path() {
    let api = api("https://example.test", include_str!("../../../tests/fixtures/chain-cycle.json"));
    let mut ex = executor();
    let err = ex.run(&api, "a", Some("test")).await.unwrap_err().to_string();
    assert!(err.contains("cycle"), "got: {err}");
    assert!(err.contains('a') && err.contains('b'), "error must name the path: {err}");
}

#[tokio::test]
async fn missing_source_endpoint_is_an_actionable_error() {
    let api = api("https://example.test", include_str!("../../../tests/fixtures/chain-missing.json"));
    let mut ex = executor();
    let err = ex.run(&api, "business", Some("test")).await.unwrap_err().to_string();
    assert!(err.contains("nonexistent"), "got: {err}");
}

#[tokio::test]
async fn default_extract_path_missing_says_so_explicitly() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "x", "expiry": 1})))
        .mount(&server).await;

    let api = api(&server.uri(), include_str!("../../../tests/fixtures/chain-default-extract.json"));
    let mut ex = executor();
    let err = ex.run(&api, "business", Some("test")).await.unwrap_err().to_string();
    assert!(err.contains("$.access_token"), "got: {err}");
    assert!(err.contains("token") && err.contains("expiry"), "error must list the keys found: {err}");
    assert!(err.contains("auth.extract"), "error must say how to fix it: {err}");
}
```

Create the fixtures. All four use `"baseUrl": "BASE_URL"`, API-level `auth`
`{"type":"none"}`, and one environment `test` with `person_id: "12345678"`.

`tests/fixtures/chain.json` endpoints:

| id | method / path | auth |
|---|---|---|
| `token` | `POST /token` | `basic` with `{{secret:GW_USER}}` / `{{secret:GW_PASS}}`; body `form` with `grant_type: client_credentials` |
| `business` | `POST /business` | `chained` on `token`, extract body `$.access_token`, ttl body `$.expires_in` seconds, inject header `Authorization` template `Bearer {{value}}`, `retryOn: [401, 403]`; body `json` `{"doc":"{{person_id}}"}` |
| `header-business` | `GET /header-business` | `chained` on `token`, `extract` `{"from":"header","name":"x-token"}`, no `ttl`, inject header `Authorization` template `Bearer {{value}}` |
| `query-business` | `GET /query-business` | `chained` on `token`, extract body `$.access_token`, inject `{"into":"query","name":"access_token"}` |

`tests/fixtures/chain-cycle.json`: endpoints `a` (`GET /a`) chained on `b` and `b`
(`GET /b`) chained on `a`, each injecting header `Authorization` with template
`Bearer {{value}}`.

`tests/fixtures/chain-missing.json`: endpoint `business` (`GET /business`) chained on
`{"endpoint":"nonexistent"}`, injecting header `Authorization`.

`tests/fixtures/chain-default-extract.json`: endpoint `token` (`POST /token`, auth
`{"type":"none"}`) and endpoint `business` (`GET /business`) chained on `token` with
**neither** an `extract` nor a `ttl` key, injecting header `Authorization` with template
`Bearer {{value}}`.

Add to `crates/core/Cargo.toml` dependencies: `serde_json_path = "0.7"` and `regex = "1"`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test chain`
Expected: FAIL — `unresolved import reqchain_core::chain`.

- [ ] **Step 3: Implement the executor**

`crates/core/src/chain.rs`:

```rust
use crate::auth::{self, AuthError};
use crate::cache::{now_unix, TokenCache};
use crate::exec::{AuthStep, ExecError, RunResult, Runner};
use crate::model::{Api, Auth, AuthExtract, AuthInject, AuthTtl};
use crate::request::{self, BuildError, EffectiveBody, EffectiveRequest};
use crate::secrets::Secrets;
use crate::vars::Scope;

pub const MAX_DEPTH: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error(transparent)]
    Exec(#[from] ExecError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("{message}")]
    Chain { message: String },
}

pub struct Executor { runner: Runner, cache: TokenCache, secrets: Secrets }

impl Executor {
    pub fn new(runner: Runner, cache: TokenCache, secrets: Secrets) -> Executor {
        Executor { runner, cache, secrets }
    }

    pub fn cache_mut(&mut self) -> &mut TokenCache { &mut self.cache }

    pub async fn run(
        &mut self,
        api: &Api,
        endpoint_id: &str,
        env: Option<&str>,
    ) -> Result<RunResult, RunError> {
        let mut visited = Vec::new();
        self.run_inner(api, endpoint_id, env, &mut visited).await
    }

    async fn run_inner(
        &mut self,
        api: &Api,
        endpoint_id: &str,
        env: Option<&str>,
        visited: &mut Vec<String>,
    ) -> Result<RunResult, RunError> {
        if visited.iter().any(|v| v == endpoint_id) {
            visited.push(endpoint_id.to_string());
            return Err(RunError::Chain {
                message: format!("auth cycle detected: {}", visited.join(" -> ")),
            });
        }
        if visited.len() >= MAX_DEPTH {
            return Err(RunError::Chain {
                message: format!(
                    "auth chain deeper than {MAX_DEPTH} levels: {}",
                    visited.join(" -> ")
                ),
            });
        }
        visited.push(endpoint_id.to_string());

        let endpoint = api.endpoint(endpoint_id).ok_or_else(|| RunError::Chain {
            message: format!("endpoint `{endpoint_id}` not found in API `{}`", api.id),
        })?;
        let scope = Scope::new(api, endpoint, env, &self.secrets);
        let mut req = request::build(api, endpoint, &scope)?;
        auth::apply_static(api, endpoint, &scope, &mut req)?;

        let resolved = auth::resolve(api, endpoint).clone();
        let Auth::Chained { source, extract, ttl, inject, retry_on } = resolved.clone() else {
            let mut res = self.runner.send(&req).await?;
            res.effective = req;
            return Ok(res);
        };

        let fingerprint = env.unwrap_or("").to_string();
        let key = TokenCache::key(&api.id, &resolved, &fingerprint);

        let (value, step) = self
            .token(api, &source.endpoint, env, &extract, &ttl, &key, visited)
            .await?;
        let mut trace = vec![step];

        let mut attempt = req.clone();
        inject_value(&mut attempt, &inject, &value);
        let mut res = self.runner.send(&attempt).await?;
        res.effective = attempt;

        if retry_on.contains(&res.status) {
            self.cache.invalidate(&key);
            // A fresh chain walk: the previous one is already recorded in `trace`.
            let mut retry_visited = vec![endpoint_id.to_string()];
            let (fresh, step2) = self
                .token(api, &source.endpoint, env, &extract, &ttl, &key, &mut retry_visited)
                .await?;
            trace.push(step2);
            let mut retry = req.clone();
            inject_value(&mut retry, &inject, &fresh);
            res = self.runner.send(&retry).await?;
            res.effective = retry;
        }

        res.auth_trace = trace;
        Ok(res)
    }

    #[allow(clippy::too_many_arguments)]
    async fn token(
        &mut self,
        api: &Api,
        source_id: &str,
        env: Option<&str>,
        extract: &Option<AuthExtract>,
        ttl: &Option<AuthTtl>,
        key: &str,
        visited: &mut Vec<String>,
    ) -> Result<(String, AuthStep), RunError> {
        if let Some(cached) = self.cache.get(key, now_unix()) {
            return Ok((
                cached,
                AuthStep {
                    endpoint_id: source_id.to_string(),
                    request: None,
                    status: 0,
                    body: String::new(),
                    from_cache: true,
                },
            ));
        }

        if api.endpoint(source_id).is_none() {
            return Err(RunError::Chain {
                message: format!(
                    "chained auth points at endpoint `{source_id}`, which does not exist in API `{}`",
                    api.id
                ),
            });
        }

        let res = Box::pin(self.run_inner(api, source_id, env, visited)).await?;
        let body_text = res.body_text();
        let value = extract_value(&res, &body_text, extract)?;
        let expires_at = compute_expiry(&body_text, ttl)?;
        self.cache.put(key, value.clone(), expires_at);
        let step = AuthStep {
            endpoint_id: source_id.to_string(),
            request: Some(res.effective.clone()),
            status: res.status,
            body: body_text,
            from_cache: false,
        };
        Ok((value, step))
    }
}

fn default_extract() -> AuthExtract {
    AuthExtract::Body { json_path: Some("$.access_token".into()), xpath: None, regex: None }
}

fn json_keys(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(", ")))
        .unwrap_or_else(|| "<not a JSON object>".into())
}

fn query_one(path: &str, value: &serde_json::Value) -> Result<Option<serde_json::Value>, RunError> {
    let jp = serde_json_path::JsonPath::parse(path).map_err(|e| RunError::Chain {
        message: format!("chained auth: invalid jsonPath `{path}`: {e}"),
    })?;
    Ok(jp.query(value).first().cloned())
}

fn extract_value(
    res: &RunResult,
    body: &str,
    extract: &Option<AuthExtract>,
) -> Result<String, RunError> {
    let was_default = extract.is_none();
    match extract.clone().unwrap_or_else(default_extract) {
        AuthExtract::Status => Ok(res.status.to_string()),
        AuthExtract::Header { name, regex } => {
            let raw = res
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&name))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| RunError::Chain {
                    message: format!(
                        "chained auth: response header `{name}` not found (headers: {})",
                        res.headers.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                })?;
            apply_regex(&raw, regex.as_deref())
        }
        AuthExtract::Body { json_path, xpath: _, regex } => {
            if let Some(rx) = regex {
                return apply_regex(body, Some(&rx));
            }
            let path = json_path.unwrap_or_else(|| "$.access_token".into());
            let value: serde_json::Value = serde_json::from_str(body).map_err(|e| RunError::Chain {
                message: format!("chained auth: auth response is not JSON ({e}), cannot apply `{path}`"),
            })?;
            match query_one(&path, &value)? {
                Some(serde_json::Value::String(s)) => Ok(s),
                Some(other) => Ok(other.to_string().trim_matches('"').to_string()),
                None if was_default => Err(RunError::Chain {
                    message: format!(
                        "chained auth: default extract {path} not found in response body (keys: {}) — set auth.extract explicitly",
                        json_keys(body)
                    ),
                }),
                None => Err(RunError::Chain {
                    message: format!(
                        "chained auth: {path} not found in response body (keys: {})",
                        json_keys(body)
                    ),
                }),
            }
        }
    }
}

fn apply_regex(input: &str, regex: Option<&str>) -> Result<String, RunError> {
    let Some(rx) = regex else { return Ok(input.to_string()) };
    let re = regex::Regex::new(rx).map_err(|e| RunError::Chain {
        message: format!("chained auth: invalid regex `{rx}`: {e}"),
    })?;
    let caps = re.captures(input).ok_or_else(|| RunError::Chain {
        message: format!("chained auth: regex `{rx}` did not match the auth response"),
    })?;
    Ok(caps.get(1).or_else(|| caps.get(0)).map(|m| m.as_str().to_string()).unwrap_or_default())
}

fn compute_expiry(body: &str, ttl: &Option<AuthTtl>) -> Result<Option<u64>, RunError> {
    let ttl = ttl.clone().unwrap_or(AuthTtl::Body {
        json_path: "$.expires_in".into(),
        unit: "seconds".into(),
    });
    match ttl {
        AuthTtl::Fixed { seconds } => Ok(Some(now_unix() + seconds)),
        AuthTtl::Body { json_path, unit } => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else { return Ok(None) };
            let Some(found) = query_one(&json_path, &value)? else { return Ok(None) };
            let Some(n) = found.as_u64().or_else(|| found.as_str().and_then(|s| s.parse().ok()))
            else {
                return Ok(None);
            };
            let seconds = if unit == "milliseconds" { n / 1000 } else { n };
            Ok(Some(now_unix() + seconds))
        }
        AuthTtl::Absolute { json_path } => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else { return Ok(None) };
            let Some(found) = query_one(&json_path, &value)?.and_then(|v| v.as_str().map(String::from))
            else {
                return Ok(None);
            };
            match chrono::DateTime::parse_from_rfc3339(&found) {
                Ok(dt) => Ok(Some(dt.timestamp().max(0) as u64)),
                Err(e) => Err(RunError::Chain {
                    message: format!("chained auth: `{found}` is not an RFC 3339 date ({e})"),
                }),
            }
        }
    }
}

fn inject_value(req: &mut EffectiveRequest, inject: &AuthInject, value: &str) {
    match inject {
        AuthInject::Header { name, template } => {
            let rendered = template.replace("{{value}}", value);
            req.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
            req.headers.push((name.clone(), rendered));
        }
        AuthInject::Query { name, template } => {
            let rendered = template.replace("{{value}}", value);
            let sep = if req.url.contains('?') { '&' } else { '?' };
            req.url.push(sep);
            req.url.push_str(&format!("{name}={rendered}"));
        }
        AuthInject::Body { pointer, template } => {
            let rendered = template.replace("{{value}}", value);
            if let Some(EffectiveBody::Text { content, .. }) = &mut req.body {
                if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(content) {
                    if let Some(slot) = json.pointer_mut(pointer) {
                        *slot = serde_json::Value::String(rendered);
                        *content = json.to_string();
                    }
                }
            }
        }
    }
}
```

Add `pub mod chain;` to `lib.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test chain`
Expected: 10 passed.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test --workspace`
Expected: every test from Tasks 1–8 passes.

- [ ] **Step 6: Commit**

```bash
git add crates/core tests/fixtures && git commit -m "feat(core): resolve chained auth with caching, refresh and cycle detection"
```

---

### Task 9: Validation and the published JSON Schema

**Files:**
- Create: `crates/core/src/validate.rs`, `schema/reqchain-api.schema.json`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`
- Test: `crates/core/tests/validate.rs`

**Interfaces:**
- Consumes: `model`, `auth::resolve`, `expr::SUPPORTED`, `chain::MAX_DEPTH`.
- Produces: `validate::{Severity::{Error, Warning}, Diagnostic { severity, path, message }, validate_text(&str) -> Vec<Diagnostic>, validate_api(&Api) -> Vec<Diagnostic>}`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/validate.rs`:

```rust
use reqchain_core::validate::{validate_text, Diagnostic, Severity};

const GOOD: &str = include_str!("../../../tests/fixtures/example-gateway-test.json");

fn errors(diags: &[Diagnostic]) -> Vec<String> {
    diags.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

#[test]
fn the_acceptance_fixture_is_valid() {
    let diags = validate_text(GOOD);
    assert!(errors(&diags).is_empty(), "unexpected errors: {:?}", errors(&diags));
}

#[test]
fn reports_malformed_json_with_a_location() {
    let msgs = errors(&validate_text("{ nope"));
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].contains("line 1"), "got: {msgs:?}");
}

#[test]
fn reports_a_chained_auth_pointing_at_a_missing_endpoint() {
    let bad = GOOD.replace("\"endpoint\": \"token\"", "\"endpoint\": \"ghost\"");
    let msgs = errors(&validate_text(&bad));
    assert!(msgs.iter().any(|m| m.contains("ghost")), "got: {msgs:?}");
}

#[test]
fn reports_an_unknown_variable() {
    let bad = GOOD.replace("{{person_id}}", "{{person_ido}}");
    let msgs = errors(&validate_text(&bad));
    assert!(msgs.iter().any(|m| m.contains("person_ido")), "got: {msgs:?}");
}

#[test]
fn reports_an_unknown_expression_function() {
    let bad = include_str!("../../../tests/fixtures/exec.json")
        .replace("BASE_URL", "https://example.test")
        .replace("md5(now(", "sha512(now(");
    let msgs = errors(&validate_text(&bad));
    assert!(msgs.iter().any(|m| m.contains("sha512")), "got: {msgs:?}");
}

#[test]
fn reports_a_chain_cycle() {
    let text = include_str!("../../../tests/fixtures/chain-cycle.json")
        .replace("BASE_URL", "https://example.test");
    let msgs = errors(&validate_text(&text));
    assert!(msgs.iter().any(|m| m.contains("cycle")), "got: {msgs:?}");
}

#[test]
fn warns_but_does_not_fail_when_extract_relies_on_the_default() {
    let text = include_str!("../../../tests/fixtures/chain-default-extract.json")
        .replace("BASE_URL", "https://example.test");
    let diags = validate_text(&text);
    assert!(errors(&diags).is_empty(), "defaults are legal: {:?}", errors(&diags));
    let warnings: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Warning).collect();
    assert!(warnings.iter().any(|w| w.message.contains("$.access_token")), "got: {warnings:?}");
    assert!(warnings.iter().any(|w| w.message.contains("$.expires_in")), "got: {warnings:?}");
}

#[test]
fn every_diagnostic_carries_a_path() {
    let bad = GOOD.replace("\"endpoint\": \"token\"", "\"endpoint\": \"ghost\"");
    for d in validate_text(&bad) {
        assert!(!d.path.is_empty(), "diagnostic without a path: {d:?}");
    }
}

#[test]
fn fixtures_satisfy_the_published_json_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../schema/reqchain-api.schema.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("schema itself must be valid");
    for fixture in [GOOD, include_str!("../../../tests/fixtures/precedence.json")] {
        let instance: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let errors: Vec<String> = validator.iter_errors(&instance).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "fixture violates schema: {errors:?}");
    }
}
```

Add `jsonschema = "0.26"` and `serde_json` to `crates/core` dev-dependencies.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test validate`
Expected: FAIL — `unresolved import reqchain_core::validate`.

- [ ] **Step 3: Implement validation**

`crates/core/src/validate.rs`:

```rust
use crate::expr::SUPPORTED;
use crate::model::{Api, Auth, AuthExtract, Body, Endpoint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity { Error, Warning }

#[derive(Debug, Clone)]
pub struct Diagnostic { pub severity: Severity, pub path: String, pub message: String }

impl Diagnostic {
    fn error(path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Error, path: path.into(), message: message.into() }
    }
    fn warning(path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, path: path.into(), message: message.into() }
    }
}

pub fn validate_text(text: &str) -> Vec<Diagnostic> {
    match Api::from_json(text) {
        Err(e) => vec![Diagnostic::error("$", e.to_string())],
        Ok(api) => validate_api(&api),
    }
}

pub fn validate_api(api: &Api) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (i, ep) in api.endpoints.iter().enumerate() {
        let base = format!("$.endpoints[{i}]");
        if !seen.insert(ep.id.clone()) {
            out.push(Diagnostic::error(format!("{base}.id"), format!("duplicate endpoint id `{}`", ep.id)));
        }
        check_variables(api, ep, &base, &mut out);
        check_auth(api, ep, &base, &mut out);
    }
    check_cycles(api, &mut out);
    out
}

fn referenced_vars(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        out.push(after[..end].trim().to_string());
        rest = &after[end + 2..];
    }
    out
}

fn known_var(api: &Api, ep: &Endpoint, name: &str) -> bool {
    if name == "value" { return true } // injection templates
    if let Some(secret) = name.strip_prefix("secret:") {
        // Whether the secret exists is a runtime concern, not a file concern.
        return !secret.trim().is_empty();
    }
    ep.variables.contains_key(name)
        || api.environments.iter().any(|e| e.variables.contains_key(name))
        || api.variables.contains_key(name)
}

fn endpoint_texts(api: &Api, ep: &Endpoint) -> Vec<String> {
    let mut texts = vec![api.base_url.clone(), ep.path.clone()];
    texts.extend(ep.headers.values().cloned());
    texts.extend(ep.query.values().cloned());
    match &ep.body {
        Some(Body::Json { content }) => texts.push(content.to_string()),
        Some(Body::Text { content }) | Some(Body::Xml { content }) => texts.push(content.clone()),
        Some(Body::Form { fields }) => texts.extend(fields.values().cloned()),
        Some(Body::Multipart { fields, files }) => {
            texts.extend(fields.values().cloned());
            texts.extend(files.values().cloned());
        }
        Some(Body::Binary { path }) => texts.push(path.clone()),
        None => {}
    }
    texts
}

fn check_variables(api: &Api, ep: &Endpoint, base: &str, out: &mut Vec<Diagnostic>) {
    for text in endpoint_texts(api, ep) {
        for name in referenced_vars(&text) {
            if !known_var(api, ep, &name) {
                out.push(Diagnostic::error(
                    base,
                    format!(
                        "unknown variable `{name}` in endpoint `{}` — define it on the endpoint, an environment, or the API",
                        ep.id
                    ),
                ));
            }
        }
    }
}

fn check_auth(api: &Api, ep: &Endpoint, base: &str, out: &mut Vec<Diagnostic>) {
    match crate::auth::resolve(api, ep) {
        Auth::Computed { expression, .. } => {
            for name in function_names(expression) {
                if !SUPPORTED.contains(&name.as_str()) {
                    out.push(Diagnostic::error(
                        format!("{base}.auth.expression"),
                        format!(
                            "unknown function `{name}` in endpoint `{}` — supported: {}",
                            ep.id,
                            SUPPORTED.join(", ")
                        ),
                    ));
                }
            }
        }
        Auth::Chained { source, extract, ttl, .. } => {
            if api.endpoint(&source.endpoint).is_none() {
                out.push(Diagnostic::error(
                    format!("{base}.auth.source.endpoint"),
                    format!(
                        "chained auth of `{}` points at endpoint `{}`, which does not exist in this API",
                        ep.id, source.endpoint
                    ),
                ));
            }
            if extract.is_none() {
                out.push(Diagnostic::warning(
                    format!("{base}.auth"),
                    format!(
                        "endpoint `{}` relies on the default extract $.access_token — set auth.extract explicitly if this API returns a different field",
                        ep.id
                    ),
                ));
            }
            if ttl.is_none() {
                out.push(Diagnostic::warning(
                    format!("{base}.auth"),
                    format!(
                        "endpoint `{}` relies on the default ttl $.expires_in — set auth.ttl explicitly if this API reports expiry differently",
                        ep.id
                    ),
                ));
            }
            if let Some(AuthExtract::Body { json_path: Some(p), .. }) = extract {
                if serde_json_path::JsonPath::parse(p).is_err() {
                    out.push(Diagnostic::error(
                        format!("{base}.auth.extract.jsonPath"),
                        format!("invalid jsonPath `{p}`"),
                    ));
                }
            }
        }
        _ => {}
    }
}

/// Every identifier immediately followed by `(` is a function call.
fn function_names(expression: &str) -> Vec<String> {
    let bytes = expression.as_bytes();
    let mut out = Vec::new();
    for end in 0..bytes.len() {
        if bytes[end] != b'(' { continue }
        let mut start = end;
        while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
            start -= 1;
        }
        if start < end {
            out.push(String::from_utf8_lossy(&bytes[start..end]).into_owned());
        }
    }
    out
}

fn check_cycles(api: &Api, out: &mut Vec<Diagnostic>) {
    for ep in &api.endpoints {
        let mut path = vec![ep.id.clone()];
        let mut current = ep.clone();
        loop {
            let Auth::Chained { source, .. } = crate::auth::resolve(api, &current).clone() else { break };
            if path.contains(&source.endpoint) {
                path.push(source.endpoint.clone());
                out.push(Diagnostic::error(
                    "$.endpoints",
                    format!("auth cycle detected: {}", path.join(" -> ")),
                ));
                break;
            }
            path.push(source.endpoint.clone());
            let Some(next) = api.endpoint(&source.endpoint) else { break };
            current = next.clone();
            if path.len() > crate::chain::MAX_DEPTH + 1 {
                out.push(Diagnostic::error(
                    "$.endpoints",
                    format!(
                        "auth chain deeper than {} levels: {}",
                        crate::chain::MAX_DEPTH,
                        path.join(" -> ")
                    ),
                ));
                break;
            }
        }
    }
}
```

Add `pub mod validate;` to `lib.rs`.

- [ ] **Step 4: Write the JSON Schema**

Create `schema/reqchain-api.schema.json`, a Draft 2020-12 schema mirroring `model.rs`
exactly:

- `$schema`, `$id` (`https://reqchain.local/schema/reqchain-api.schema.json`), `title`.
- Root `type: object`, `additionalProperties: false`, required `schemaVersion`, `id`, `name`, `baseUrl`, `endpoints`.
- `schemaVersion`: `{"const": 1}`.
- `method`: `{"enum": ["GET","POST","PUT","PATCH","DELETE","HEAD","OPTIONS"]}`.
- `$defs` for `environment`, `endpoint`, `body`, `auth`, `extract`, `ttl`, `inject`, `chainSource`.
- `body` and `auth` are `oneOf` branches discriminated by `type`; `extract` by `from`; `inject` by `into`. Every branch sets `additionalProperties: false` and lists its own required fields.
- Every property carries a one-sentence `description` — agents read this schema.

Verify the schema itself parses: the `fixtures_satisfy_the_published_json_schema` test
fails loudly if it does not.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test validate`
Expected: 9 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/core schema && git commit -m "feat(core): validate API files with actionable errors and default warnings"
```

---

### Task 10: CLI — list, run, validate

**Files:**
- Create: `crates/cli/Cargo.toml`, `crates/cli/src/main.rs`
- Test: `crates/cli/tests/cli.rs`

**Interfaces:**
- Consumes: the whole of `reqchain-core`.
- Produces: binary `reqchain` with `list`, `run <api> <endpoint> [--env NAME] [--print-command]`, `validate [FILE...]`. Exit codes: `0` success, `1` validation or chain failure, `2` transport failure, `3` usage error (unknown API or endpoint). The HTTP status is printed but never changes the exit code.

- [ ] **Step 1: Write the failing test**

`crates/cli/tests/cli.rs`:

```rust
use std::process::Command;

const GOOD: &str = include_str!("../../../tests/fixtures/example-gateway-test.json");

fn bin() -> Command { Command::new(env!("CARGO_BIN_EXE_reqchain")) }

fn workspace_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let apis = dir.path().join("workspace/apis");
    std::fs::create_dir_all(&apis).unwrap();
    for (name, content) in files { std::fs::write(apis.join(name), content).unwrap() }
    dir
}

#[test]
fn list_prints_apis_and_endpoints() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let out = bin().arg("list").env("REQCHAIN_DIR", dir.path()).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("example-gateway-test"));
    assert!(text.contains("repairQuery"));
}

#[test]
fn validate_succeeds_on_a_good_file() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let out = bin().arg("validate").env("REQCHAIN_DIR", dir.path()).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn validate_fails_with_an_actionable_message() {
    let bad = GOOD.replace("\"endpoint\": \"token\"", "\"endpoint\": \"ghost\"");
    let dir = workspace_with(&[("a.json", &bad)]);
    let out = bin().arg("validate").env("REQCHAIN_DIR", dir.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("ghost"), "got: {text}");
}

#[test]
fn validate_accepts_an_explicit_file_path() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let path = dir.path().join("workspace/apis/a.json");
    let out = bin().arg("validate").arg(&path).output().unwrap();
    assert!(out.status.success());
}

#[test]
fn unknown_endpoint_is_a_usage_error() {
    let dir = workspace_with(&[("a.json", GOOD)]);
    let out = bin()
        .args(["run", "example-gateway-test", "ghost"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("ghost"), "got: {text}");
}
```

`crates/cli/Cargo.toml`:

```toml
[package]
name = "reqchain-cli"
edition.workspace = true
version.workspace = true

[[bin]]
name = "reqchain"
path = "src/main.rs"

[dependencies]
reqchain-core = { path = "../core" }
clap = { version = "4", features = ["derive"] }
tokio.workspace = true
serde_json.workspace = true

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-cli --test cli`
Expected: FAIL — the binary target does not exist yet.

- [ ] **Step 3: Implement the CLI**

`crates/cli/src/main.rs`:

```rust
use clap::{Parser, Subcommand};
use reqchain_core::{
    cache::TokenCache,
    chain::Executor,
    exec::Runner,
    paths::Paths,
    secrets::Secrets,
    shell,
    store::Workspace,
    validate::{self, Severity},
};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "reqchain", about = "HTTP client with request-derived auth")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List APIs and their endpoints
    List,
    /// Run an endpoint, resolving its auth chain
    Run {
        api: String,
        endpoint: String,
        #[arg(long)]
        env: Option<String>,
        /// Print the effective request as a shell command instead of sending it
        #[arg(long = "print-command")]
        print_command: bool,
    },
    /// Validate API files (defaults to every file in the workspace)
    Validate { files: Vec<PathBuf> },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let paths = Paths::from_env();
    match cli.command {
        Cmd::List => list(&paths),
        Cmd::Validate { files } => validate_cmd(&paths, files),
        Cmd::Run { api, endpoint, env, print_command } =>
            run(&paths, &api, &endpoint, env.as_deref(), print_command).await,
    }
}

fn list(paths: &Paths) -> ExitCode {
    let ws = Workspace::load(paths);
    for api in &ws.apis {
        println!("{} — {} ({})", api.id, api.name, api.base_url);
        for ep in &api.endpoints {
            println!("  {:<24} {:?} {}", ep.id, ep.method, ep.path);
        }
    }
    for err in &ws.errors {
        eprintln!("error: {}: {}", err.path.display(), err.message);
    }
    if ws.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn validate_cmd(paths: &Paths, files: Vec<PathBuf>) -> ExitCode {
    let targets: Vec<PathBuf> = if files.is_empty() {
        std::fs::read_dir(paths.apis_dir())
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        files
    };

    let mut failed = false;
    for file in targets {
        let Ok(text) = std::fs::read_to_string(&file) else {
            eprintln!("error: cannot read {}", file.display());
            failed = true;
            continue;
        };
        let diags = validate::validate_text(&text);
        for d in &diags {
            let label = match d.severity { Severity::Error => "error", Severity::Warning => "warning" };
            println!("{}: {}: {} at {}", file.display(), label, d.message, d.path);
        }
        if diags.iter().any(|d| d.severity == Severity::Error) {
            failed = true;
        } else {
            println!("{}: ok", file.display());
        }
    }
    if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

async fn run(
    paths: &Paths,
    api_id: &str,
    endpoint_id: &str,
    env: Option<&str>,
    print_command: bool,
) -> ExitCode {
    let ws = Workspace::load(paths);
    let Some(api) = ws.api(api_id) else {
        eprintln!(
            "error: API `{api_id}` not found. Available: {}",
            ws.apis.iter().map(|a| a.id.as_str()).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::from(3);
    };
    if api.endpoint(endpoint_id).is_none() {
        eprintln!(
            "error: endpoint `{endpoint_id}` not found in `{api_id}`. Available: {}",
            api.endpoints.iter().map(|e| e.id.as_str()).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::from(3);
    }

    let secrets = Secrets::load(paths);
    let secret_values: Vec<String> = secrets.values().cloned().collect();
    let mut executor = Executor::new(Runner::new(), TokenCache::persistent(paths), secrets);

    match executor.run(api, endpoint_id, env).await {
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(if e.to_string().contains("transport") { 2 } else { 1 })
        }
        Ok(res) => {
            let _ = executor.cache_mut().save();
            let masked = res.effective.masked(&secret_values);
            if print_command {
                println!("{}", shell::to_shell_command(&masked));
                return ExitCode::SUCCESS;
            }
            eprintln!("{} {}ms {}B", res.status, res.elapsed_ms, res.size_bytes);
            for step in &res.auth_trace {
                eprintln!(
                    "auth: {} -> {}{}",
                    step.endpoint_id,
                    step.status,
                    if step.from_cache { " (cached)" } else { "" }
                );
            }
            let text = res.body_text();
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or(text)),
                Err(_) => println!("{text}"),
            }
            ExitCode::SUCCESS
        }
    }
}
```

Note: a cached auth step reports status `0`; the printed line reads
`auth: token -> 0 (cached)`, which is why the acceptance test in Task 12 asserts on the
uncached form.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reqchain-cli --test cli`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/cli && git commit -m "feat(cli): add list, run and validate commands"
```

---

### Task 11: SPEC.md and README

**Files:**
- Create: `SPEC.md`, `README.md`
- Create: `tests/fixtures/library.json`
- Test: `crates/core/tests/spec_examples.rs`

**Interfaces:**
- Consumes: `validate`.
- Produces: the agent-facing documentation, plus `tests/fixtures/library.json` (spec §14 case 2).

- [ ] **Step 1: Write the failing test**

`crates/core/tests/spec_examples.rs` — every example in `SPEC.md` must validate, so the
document cannot rot:

```rust
use reqchain_core::validate::{validate_text, Severity};

/// Every fenced ```json block in SPEC.md that looks like a complete API file.
fn spec_examples() -> Vec<String> {
    let spec = include_str!("../../../SPEC.md");
    let mut out = Vec::new();
    let mut rest = spec;
    while let Some(start) = rest.find("```json") {
        let after = &rest[start + 7..];
        let Some(end) = after.find("```") else { break };
        let block = after[..end].trim().to_string();
        if block.contains("\"schemaVersion\"") { out.push(block) }
        rest = &after[end + 3..];
    }
    out
}

#[test]
fn spec_contains_complete_examples() {
    assert!(spec_examples().len() >= 2, "SPEC.md must show at least the two acceptance cases");
}

#[test]
fn every_spec_example_validates() {
    for (i, example) in spec_examples().iter().enumerate() {
        let errors: Vec<_> = validate_text(example)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "SPEC.md example {i} is invalid: {errors:?}");
    }
}

#[test]
fn the_library_fixture_validates() {
    let errors: Vec<_> = validate_text(include_str!("../../../tests/fixtures/library.json"))
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p reqchain-core --test spec_examples`
Expected: FAIL — `SPEC.md` does not exist.

- [ ] **Step 3: Write `tests/fixtures/library.json`**

`id` `library`, `name` `Library (test)`, `baseUrl` `https://api.example.com`, one
environment `test` with `person_id: "12345678"`, API-level `auth` `{"type":"none"}`, and
one endpoint `library-user`: `GET /library/user`, headers
`{"token": "{{secret:API_TOKEN}}", "user": "{{person_id}}"}`, auth
`{"type":"computed","name":"hash","expression":"md5(now(\"YYYYMMDD\") + {{person_id}})"}`.

- [ ] **Step 4: Write `SPEC.md`**

Self-contained, written for an agent with no other context. Sections, in this order:

1. **What this format is** — one paragraph; where files live (`$REQCHAIN_DIR/workspace/apis/<id>.json`, default `~/.config/reqchain`).
2. **Rules** — one API per file; the file name matches `id`; `schemaVersion: 1`; 2-space indent; trailing newline; never write a credential into the file, reference it as `{{secret:NAME}}`; run `reqchain validate <file>` after writing.
3. **Complete example 1** — the exact content of `tests/fixtures/example-gateway-test.json` in a ```json block, with a sentence explaining each block.
4. **Complete example 2** — the exact content of `tests/fixtures/library.json` in a ```json block (the computed-header case).
5. **Field reference** — one table per object (API, Environment, Endpoint, each `body` variant, each `auth` variant, `extract`, `ttl`, `inject`); columns: field, type, required, meaning.
6. **Variables** — `{{var}}` and `{{secret:NAME}}`, precedence endpoint → active environment → API, and that an unknown variable fails before anything is sent.
7. **Expression language** — the grammar, the five functions, the `now` format tokens, and that nothing else is permitted.
8. **Chained auth** — the end-to-end flow, the `$.access_token` / `$.expires_in` defaults with the warning that they only fit OAuth2-shaped responses, the one-retry rule, cycle detection and the depth limit of 5.
9. **Validating** — `reqchain validate <file>`, the exit codes, and a sample error and warning line.

Keep prose minimal: an agent needs rules and examples, not narrative.

- [ ] **Step 5: Write `README.md`**

Prerequisites (`rustup`, `build-essential`), `cargo build --release` and where the binary
lands, the directory layout, how to create the secret store with owner-only permissions,
how to run the acceptance case, and a pointer to `SPEC.md`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p reqchain-core --test spec_examples`
Expected: 3 passed.

- [ ] **Step 7: Commit**

```bash
git add SPEC.md README.md tests/fixtures && git commit -m "docs: add agent-facing file format spec and README"
```

---

### Task 12: End-to-end acceptance against a mock gateway

**Files:**
- Create: `crates/cli/tests/acceptance.rs`
- Modify: `crates/cli/Cargo.toml` (dev-dependencies)

**Interfaces:**
- Consumes: the `reqchain` binary and the shipped fixtures.
- Produces: the executable form of spec §14 — the gate for calling phase 1 done.

- [ ] **Step 1: Write the failing test**

`crates/cli/tests/acceptance.rs`:

```rust
use std::process::Command;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const GATEWAY: &str = include_str!("../../../tests/fixtures/example-gateway-test.json");
const LIBRARY: &str = include_str!("../../../tests/fixtures/library.json");

fn workspace(files: &[(&str, String)], store: serde_json::Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let apis = dir.path().join("workspace/apis");
    std::fs::create_dir_all(&apis).unwrap();
    for (name, content) in files { std::fs::write(apis.join(name), content).unwrap() }
    std::fs::write(dir.path().join("secrets.json"), store.to_string()).unwrap();
    dir
}

/// Spec §14 case 1: Send on repairQuery without having run `token` first.
#[tokio::test]
async fn chained_endpoint_runs_without_calling_token_first() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .and(header("authorization", "Basic YWxpY2U6aHVudGVyMg=="))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "live-token",
            "expires_in": 3600
        })))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/repairs/1.0"))
        .and(header("authorization", "Bearer live-token"))
        .and(body_string_contains("12345678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"Reparaciones": []})))
        .mount(&server).await;

    let api = GATEWAY.replace("https://api-manager.example.com", &server.uri());
    let dir = workspace(
        &[("gateway.json", api)],
        serde_json::json!({"GW_USER": "alice", "GW_PASS": "hunter2"}),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_reqchain"))
        .args(["run", "example-gateway-test", "repairQuery", "--env", "test"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    assert!(stdout.contains("Reparaciones"), "stdout: {stdout}");
    assert!(stderr.contains("auth: token -> 200"), "the auth request must be reported: {stderr}");
}

/// Spec §14 case 2: computed md5 header.
#[tokio::test]
async fn computed_header_endpoint_runs() {
    let server = MockServer::start().await;
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let expected_hash = format!("{:x}", md5::compute(format!("{today}12345678").as_bytes()));
    Mock::given(method("GET")).and(path("/library/user"))
        .and(header("user", "12345678"))
        .and(header("hash", expected_hash.as_str()))
        .and(header("token", "api-token-value"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"user": "ok"})))
        .mount(&server).await;

    let api = LIBRARY.replace("https://api.example.com", &server.uri());
    let dir = workspace(&[("library.json", api)], serde_json::json!({"API_TOKEN": "api-token-value"}));

    let out = Command::new(env!("CARGO_BIN_EXE_reqchain"))
        .args(["run", "library", "library-user", "--env", "test"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[tokio::test]
async fn printed_command_hides_secret_values() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "live-token",
            "expires_in": 3600
        })))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/repairs/1.0"))
        .respond_with(ResponseTemplate::new(200)).mount(&server).await;

    let api = GATEWAY.replace("https://api-manager.example.com", &server.uri());
    let dir = workspace(
        &[("gateway.json", api)],
        serde_json::json!({"GW_USER": "alice", "GW_PASS": "hunter2"}),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_reqchain"))
        .args(["run", "example-gateway-test", "repairQuery", "--env", "test", "--print-command"])
        .env("REQCHAIN_DIR", dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("curl "), "got: {stdout}");
    assert!(!stdout.contains("hunter2"), "export must not leak secret values: {stdout}");
}
```

Add to `crates/cli/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
chrono = "0.4"
md5 = "0.7"
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p reqchain-cli --test acceptance`
Expected: PASS if Tasks 1–11 are complete. If anything fails, read the failure before
touching code — this test is the specification, not a suggestion.

- [ ] **Step 3: Fix whatever the acceptance test exposes**

No new features. A failure here means a defect in an earlier task's code: fix it there and
re-run that task's tests as well.

- [ ] **Step 4: Run the full suite and the linters**

Run: `cargo test --workspace`
Expected: every test passes.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings. Fix them; do not add `#[allow]` beyond the one this plan already
specifies.

Run: `cargo fmt --all`

- [ ] **Step 5: Commit**

```bash
git add crates tests && git commit -m "test: add end-to-end acceptance for both spec cases"
```

---

## Done criteria for phase 1

- `cargo test --workspace` green and `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `reqchain validate` accepts both acceptance fixtures and rejects each broken variant with a message naming the problem.
- `reqchain run example-gateway-test repairQuery --env test` fetches, caches, injects and refreshes the token against the mock, and reports the auth step.
- `SPEC.md`'s examples are checked by a test, so the document cannot drift from the code.

Phase 2 (Tauri UI) and phase 3 (`.deb` packaging) get their own plans, written once this
one lands and the core's public API is fixed.

## Deliberately not in phase 1

Named here so nothing is silently dropped:

- **History** (spec §8) and the **file watcher with hot reload** (spec §9) — both exist to
  serve the UI, so they land in the phase 2 plan alongside it. The CLI does not need them.
- **XPath extraction** from an auth response (spec §7). The model accepts the `xpath`
  field so files written against the schema stay valid, but the executor ignores it.
  Task 9's `check_auth` must therefore emit an **error** for
  `extract: { from: "body", xpath: ... }` reading
  `xpath extraction is not implemented yet — use jsonPath or regex`, so an agent is told
  at validate time instead of getting an empty token at runtime. Add a test for that
  message in `crates/core/tests/validate.rs`.
