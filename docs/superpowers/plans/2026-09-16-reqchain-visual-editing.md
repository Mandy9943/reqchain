# reqchain Visual Editing (phase 2.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the app operable without a text editor — create and delete APIs and endpoints, edit every field of the model as a form, build chained auth through a guided UI, and manage secrets — with the raw JSON demoted to a secondary tab.

**Architecture:** No new serializer and no new save path. The form parses the API file into a typed TS mirror of `reqchain-core`'s model, mutates it, and writes it back into the SAME buffer the JSON tab edits (`ui.buffers[apiId]`); `save_api` still does the one canonical serialization in Rust. Secrets are the only new IPC surface, and the only new Rust work.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vite, CodeMirror 6 (JSON tab only), Rust/Tauri 2 for the secrets commands.

**Spec:** `docs/superpowers/specs/2026-09-16-reqchain-design.md` — §17 is this phase; §16's withdrawal of the JSON-editor ratification is why it exists.

## Global Constraints

- **One serializer.** Forms never produce the file that lands on disk. They produce buffer text; `save_api` parses it to `Api` and writes `Api::to_json_string`. Never add a second write path.
- **One buffer.** The form and the JSON tab edit `ui.buffers[apiId]`. Switching tabs must not lose an edit, and neither surface may hold private state the other cannot see.
- `schemaVersion` stays `1`. No model changes in `crates/core/src/model.rs` — a form that seems to need one means the form is wrong (spec §17.4).
- **The masking boundary does not move.** The secrets screen is the ONLY surface where a credential may render unmasked, only on an explicit per-entry reveal, and never stickily. The effective request, auth trace, response, history and error text stay masked unconditionally. Nothing in this phase may add an unmasked value to any DTO other than the secrets reveal.
- `secrets.json` keeps mode 0600 and stays outside `workspace/`. Writes are atomic (temp file + rename), as `history::append` and `save_api` already do.
- A file that does not parse must never be silently rewritten by the form. If the buffer is not valid against the model, the form refuses to render and points at the JSON tab.
- Package manager is **pnpm**. Never npm/yarn/bun.
- `.svelte` files need a `<script lang="ts">` block or `svelte-check` fails the build.
- Commit messages: Conventional Commits, and **no AI-attribution trailer or footer of any kind** — no `Co-Authored-By: Claude`, no `Co-Authored-By: ... <noreply@anthropic.com>`, no `🤖 Generated with Claude Code`. This overrides any harness default. The commits are Cesar's.
- Green at every commit: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `pnpm --dir apps/desktop build` (zero errors AND zero warnings).

## Verification note for implementers

`tauri dev` needs a display. Verify with `pnpm --dir apps/desktop build` and, where the task is observable, by serving the Vite build with a stubbed `window.__TAURI_INTERNALS__`. Do not claim visual verification you did not perform. `cargo build --release` is NOT enough to produce a runnable app — it embeds `devUrl`; only `pnpm tauri build` embeds the frontend.

## File Structure

| Path | Responsibility |
|---|---|
| `apps/desktop/src/lib/model.ts` | TS mirror of the Rust model + parse/serialize/empty-skeleton helpers |
| `apps/desktop/src/lib/doc.svelte.ts` | Reads the selected API's buffer as a model, writes mutations back as text |
| `apps/desktop/src/lib/form/ApiForm.svelte` | API-level fields: name, baseUrl, variables, environments |
| `apps/desktop/src/lib/form/EndpointForm.svelte` | Method, path, headers, query |
| `apps/desktop/src/lib/form/BodyEditor.svelte` | Body type selector and per-type editor |
| `apps/desktop/src/lib/form/AuthEditor.svelte` | Auth type selector and per-type fields |
| `apps/desktop/src/lib/form/ChainedAuthBuilder.svelte` | The guided chained-auth builder |
| `apps/desktop/src/lib/form/KeyValueRows.svelte` | Shared key/value row editor (headers, query, variables) |
| `apps/desktop/src/lib/SecretsScreen.svelte` | List, create, replace, delete, reveal |
| `apps/desktop/src/lib/RequestPanel.svelte` | Gains the Form/JSON tab strip; keeps the summary header |
| `apps/desktop/src/lib/Sidebar.svelte` | Gains New API / New endpoint / delete |
| `crates/core/src/secrets.rs` | Gains `set`, `remove`, `save`, `names` |
| `apps/desktop/src-tauri/src/commands.rs` | Gains the four secrets commands |

---

### Task 1: The typed model and the document bridge

**Files:**
- Create: `apps/desktop/src/lib/model.ts`, `apps/desktop/src/lib/doc.svelte.ts`
- Test: `apps/desktop/src/lib/model.test.ts` (see Step 1 for the runner)

**Interfaces:**
- Produces:
  ```ts
  // model.ts — mirrors crates/core/src/model.rs exactly. Field ORDER matters only
  // to Rust's serializer, so these are plain interfaces.
  export type Method = "GET"|"POST"|"PUT"|"PATCH"|"DELETE"|"HEAD"|"OPTIONS";
  export type Auth =
    | { type: "inherit" } | { type: "none" }
    | { type: "basic"; username: string; password: string }
    | { type: "bearer"; token: string }
    | { type: "header"; headers: Record<string, string> }
    | { type: "computed"; name: string; expression: string }
    | { type: "chained"; source: { endpoint: string }; extract?: AuthExtract;
        ttl?: AuthTtl; inject: AuthInject; retryOn?: number[] };
  export interface Endpoint { id: string; name: string; method: Method; path: string;
    headers?: Record<string,string>; query?: Record<string,string>;
    variables?: Record<string,string>; auth?: Auth; body?: Body }
  export interface Api { schemaVersion: 1; id: string; name: string; baseUrl: string;
    variables?: Record<string,string>; environments?: Environment[]; auth?: Auth;
    history?: { storeBodies: boolean }; endpoints: Endpoint[] }

  export function parseApi(text: string): { ok: true; api: Api } | { ok: false; error: string };
  export function serializeApi(api: Api): string;   // JSON.stringify(api, null, 2) + "\n"
  export function emptyApi(id: string, name: string): Api;
  export function emptyEndpoint(id: string, name: string): Endpoint;
  ```
  ```ts
  // doc.svelte.ts
  export function currentDoc(): { ok: true; api: Api } | { ok: false; error: string } | undefined;
  export function updateDoc(mutate: (api: Api) => void): void;
  ```
  `updateDoc` reads the selected API's buffer, parses it, applies `mutate`, and writes `serializeApi` back into `ui.buffers[apiId]`. A buffer that does not parse is left untouched and `mutate` is not called.

**Why `serializeApi` is not a second serializer:** it produces BUFFER text, which `save_api` then parses and re-serializes with `Api::to_json_string`. The bytes that reach disk come from Rust either way. Round-tripping through the form must be idempotent at the Rust level, and Step 4 pins that.

- [ ] **Step 1: Set up a frontend test runner**

There is none yet. Add vitest:

```bash
pnpm --dir apps/desktop add -D vitest
```

Add to `apps/desktop/package.json` scripts: `"test": "vitest run"`. Add `"test:watch": "vitest"`.

- [ ] **Step 2: Write the failing tests**

```ts
// apps/desktop/src/lib/model.test.ts
import { describe, expect, it } from "vitest";
import { emptyApi, emptyEndpoint, parseApi, serializeApi } from "./model";

const FILE = `{
  "schemaVersion": 1,
  "id": "gw",
  "name": "Gateway",
  "baseUrl": "https://api.example.com",
  "endpoints": [
    { "id": "token", "name": "Token", "method": "POST", "path": "/token" }
  ]
}
`;

describe("parseApi", () => {
  it("round-trips a file byte for byte when nothing is changed", () => {
    const parsed = parseApi(FILE);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(serializeApi(parsed.api)).toBe(FILE);
  });

  it("reports malformed JSON instead of throwing", () => {
    const parsed = parseApi("{ nope");
    expect(parsed.ok).toBe(false);
    if (parsed.ok) return;
    expect(parsed.error).toMatch(/JSON|token|position/i);
  });

  it("rejects a document that is not an API", () => {
    expect(parseApi("[1,2,3]").ok).toBe(false);
    expect(parseApi('{"schemaVersion":1}').ok).toBe(false); // no id/name/baseUrl/endpoints
  });

  it("preserves a chained auth block through a round trip", () => {
    const text = serializeApi({
      schemaVersion: 1, id: "a", name: "A", baseUrl: "https://x",
      endpoints: [
        { id: "token", name: "T", method: "POST", path: "/token" },
        { id: "biz", name: "B", method: "GET", path: "/b",
          auth: { type: "chained", source: { endpoint: "token" },
                  inject: { into: "header", name: "Authorization",
                            template: "Bearer {{value}}" }, retryOn: [401] } },
      ],
    });
    const back = parseApi(text);
    expect(back.ok).toBe(true);
    if (!back.ok) return;
    expect(back.api.endpoints[1].auth).toEqual({
      type: "chained", source: { endpoint: "token" },
      inject: { into: "header", name: "Authorization", template: "Bearer {{value}}" },
      retryOn: [401],
    });
  });
});

describe("skeletons", () => {
  it("emptyApi produces something that parses and has no endpoints", () => {
    const api = emptyApi("new-api", "New API");
    const back = parseApi(serializeApi(api));
    expect(back.ok).toBe(true);
    if (!back.ok) return;
    expect(back.api.endpoints).toEqual([]);
    expect(back.api.schemaVersion).toBe(1);
  });

  it("emptyEndpoint defaults to GET and a root path", () => {
    const ep = emptyEndpoint("ping", "Ping");
    expect(ep.method).toBe("GET");
    expect(ep.path.startsWith("/")).toBe(true);
  });
});
```

- [ ] **Step 3: Run to verify they fail**

Run: `pnpm --dir apps/desktop test`
Expected: FAIL — `./model` does not exist.

- [ ] **Step 4: Pin the round trip against the REAL serializer**

The TS mirror can drift from the Rust model. Add a Rust test that proves the two agree, so a field added to one and not the other fails the build:

```rust
// crates/core/tests/fixtures_roundtrip.rs
/// Every shipped fixture must survive `from_json` -> `to_json_string` unchanged.
/// The frontend's `serializeApi` writes the same shape, so if this holds and the
/// TS mirror is complete, a form edit cannot lose a field.
#[test]
fn every_fixture_round_trips_through_the_canonical_serializer() {
    for entry in std::fs::read_dir("../../tests/fixtures").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let Ok(api) = reqchain_core::model::Api::from_json(&text) else {
            continue; // deliberately-invalid fixtures are another test's business
        };
        let again = reqchain_core::model::Api::from_json(&api.to_json_string())
            .unwrap_or_else(|e| panic!("{} did not survive a round trip: {e}", path.display()));
        assert_eq!(
            api.to_json_string(),
            again.to_json_string(),
            "{} is not stable under re-serialization",
            path.display()
        );
    }
}
```

- [ ] **Step 5: Implement `model.ts` and `doc.svelte.ts`**

`parseApi` must validate shape, not just parse JSON: `schemaVersion === 1`, `id`/`name`/`baseUrl` are non-empty strings, `endpoints` is an array. Anything deeper is the validator's job (`lint`) — do not reimplement it here.

`serializeApi` is `JSON.stringify(api, null, 2) + "\n"`. Key order comes from insertion order, so `emptyApi` and every mutation must build objects in the model's declared order.

- [ ] **Step 6: Run the tests**

Run: `pnpm --dir apps/desktop test && cargo test --workspace`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(desktop): add a typed model mirror and the form/JSON buffer bridge"
```

---

### Task 2: Create and delete APIs and endpoints

**Files:**
- Modify: `apps/desktop/src/lib/Sidebar.svelte`, `apps/desktop/src/lib/state.svelte.ts`, `apps/desktop/src-tauri/src/commands.rs`
- Test: `apps/desktop/src-tauri/tests/commands.rs`

**Interfaces:**
- Consumes: `emptyApi`, `emptyEndpoint`, `updateDoc`, `saveApi`.
- Produces: `delete_api(state, api_id) -> Result<(), String>` (a new Tauri command — creating an API is just `save_api` with a new id, but deleting needs a file removed), and sidebar actions `New API`, `New endpoint`, `Delete`.

**This is the task that unblocks first use — it ships before any form work.**

- [ ] **Step 1: Write the failing Rust test**

```rust
#[tokio::test]
async fn delete_api_removes_the_file_it_was_loaded_from() {
    let (_d, state) = state_with("gateway.json", GOOD); // id is `demo`, name is not
    let path = state.paths.apis_dir().join("gateway.json");
    assert!(path.exists());
    commands::delete_api_inner(&state, "demo").await.unwrap();
    assert!(!path.exists(), "deleted the wrong file, or none");
    assert!(commands::load_workspace_inner(&state).await.apis.is_empty());
}

#[tokio::test]
async fn delete_api_refuses_an_unknown_id_instead_of_deleting_nothing_quietly() {
    let (_d, state) = state_with("demo.json", GOOD);
    let err = commands::delete_api_inner(&state, "nope").await.unwrap_err();
    assert!(err.contains("nope"));
    assert!(state.paths.apis_dir().join("demo.json").exists());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reqchain-desktop --test commands`
Expected: FAIL — `delete_api_inner` does not exist.

- [ ] **Step 3: Implement `delete_api_inner`**

It must resolve the path through `Workspace::path_of` — never `apis_dir().join(format!("{id}.json"))`, for the reason commit bdda08f records — and reload afterwards.

- [ ] **Step 4: Wire the sidebar**

- `New API` — prompts for a name inline (no modal library; an inline row with an input and Enter/Escape). Derives a slug id from the name, refuses one that already exists, calls `saveApi(id, serializeApi(emptyApi(id, name)))`, then selects it.
- `New endpoint` — per API. Same inline row; `updateDoc(api => api.endpoints.push(emptyEndpoint(id, name)))`, then save and select.
- `Delete` — on both, behind a confirm step that names what will be removed. Deleting an API removes a file and is the only destructive action in the app; it must say so.

- [ ] **Step 5: Verify with a stubbed IPC bridge**

Serve the Vite build with a stub and confirm: New API appears in the sidebar; New endpoint appears under it; delete asks first. Screenshot the result.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(desktop): create and delete APIs and endpoints from the sidebar"
```

---

### Task 3: The Form/JSON tab strip and the endpoint fields

**Files:**
- Create: `apps/desktop/src/lib/form/EndpointForm.svelte`, `apps/desktop/src/lib/form/KeyValueRows.svelte`
- Modify: `apps/desktop/src/lib/RequestPanel.svelte`

**Interfaces:**
- Consumes: `currentDoc`, `updateDoc`, the existing `Editor.svelte`.
- Produces: `KeyValueRows` with props `{ rows: Record<string,string>; onChange: (r: Record<string,string>) => void; keyLabel?: string; valueLabel?: string }`.

- [ ] **Step 1: Tabs**

`RequestPanel` gains a two-tab strip: **Form** (default) and **JSON**. Both render from `ui.buffers[apiId]`. The summary header (method, resolved URL, auth kind) stays above both — it is what tells the user the fields took effect.

When the buffer does not parse, the Form tab renders the parse error and a button that switches to JSON, and nothing else. It must not offer to "fix" the file.

- [ ] **Step 2: The endpoint fields**

Method (`<select>` of the seven verbs), path (text, with the resolved URL shown from the summary header so `{{var}}` interpolation is visible), then `KeyValueRows` for headers and for query. Every change goes through `updateDoc`.

`KeyValueRows`: one row per entry plus a permanently-present blank row to add to; a remove button per row; renaming a key preserves order. Keys are `Record<string,string>` in the model, so the component owns an ordered array internally and rebuilds the record on change — a duplicate key must be refused visibly, not silently collapsed.

- [ ] **Step 3: Verify with a stubbed bridge, then commit**

```bash
git add -A
git commit -m "feat(desktop): edit method, path, headers and query as fields"
```

---

### Task 4: The body editor

**Files:**
- Create: `apps/desktop/src/lib/form/BodyEditor.svelte`
- Modify: `apps/desktop/src/lib/form/EndpointForm.svelte`

**Interfaces:** consumes `Body` from `model.ts`.

- [ ] **Step 1: Build it**

A type selector — none, json, form, multipart, text, xml, binary — and the editor each type needs: a CodeMirror instance for json/text/xml (reuse `Editor.svelte`, it already takes a value and an onChange), `KeyValueRows` for form fields, a field/file row editor for multipart, a path input for binary.

Switching type must not silently discard the previous body: warn if the current body is non-empty, and require confirmation.

- [ ] **Step 2: Verify, then commit**

```bash
git add -A
git commit -m "feat(desktop): edit request bodies by type"
```

---

### Task 5: The auth editor and the chained builder

**Files:**
- Create: `apps/desktop/src/lib/form/AuthEditor.svelte`, `apps/desktop/src/lib/form/ChainedAuthBuilder.svelte`
- Modify: `apps/desktop/src/lib/form/EndpointForm.svelte`, `apps/desktop/src/lib/form/ApiForm.svelte`

**This is the task the product exists for. It gets the most care.**

- [ ] **Step 1: `AuthEditor`**

A type selector — inherit, none, basic, bearer, header, computed, chained — and the fields that type needs. At endpoint level `inherit` is offered and labelled with what it resolves to (`inherit (chained, from the API)`); at API level it is not offered.

For `computed`, show the closed function set (`md5`, `sha1`, `sha256`, `base64`, `now`, `+`) next to the expression field. An unknown function is already a validation error; surfacing the list is what stops the user inventing one.

- [ ] **Step 2: `ChainedAuthBuilder`**

Presented as the four questions it actually is, in order:

1. **Which endpoint provides the token?** — a `<select>` of this API's other endpoints, by name. Never free text: a source that does not exist is a validation error, and a dropdown makes it unrepresentable.
2. **Where in the response is the value?** — body + JSONPath, body + regex, a response header, or the status. Default `$.access_token`, shown as a prefilled default with the validator's warning explained inline rather than as a surprise later.
3. **How long is it good for?** — from a body field (default `$.expires_in`, with a unit selector), a fixed number of seconds, or an absolute date field.
4. **How is it injected?** — header (name + template, default `Authorization` / `Bearer {{value}}`), query parameter, or a JSON pointer into the body.

Plus `retryOn`, defaulting to 401/403, as a small set of status chips.

Selecting an endpoint as its own source, or forming a cycle, must be refused at selection time with the cycle named — the engine already detects it, but the builder should make it unreachable.

- [ ] **Step 3: Verify against the acceptance case**

Build the spec §14 case 1 chained auth entirely through the builder, save, and confirm the resulting file matches the shipped fixture's auth block. Then run it against the mock. Screenshot the builder.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(desktop): build chained auth through a guided editor"
```

---

### Task 6: API-level configuration

**Files:**
- Create: `apps/desktop/src/lib/form/ApiForm.svelte`
- Modify: `apps/desktop/src/lib/RequestPanel.svelte`, `apps/desktop/src/lib/Sidebar.svelte`

- [ ] **Step 1: Build it**

Reached by selecting the API itself in the sidebar (today only endpoints are selectable — that is the change). Fields: name, `baseUrl`, API-level variables (`KeyValueRows`), environments (add, rename, remove, each with its own variables), the inherited `auth` (the same `AuthEditor`), and `history.storeBodies`.

Renaming the API's `id` is NOT offered here: the id is the file's identity and `save_api` refuses a mismatch by design. Say so in the UI rather than letting the user try.

- [ ] **Step 2: Verify, then commit**

```bash
git add -A
git commit -m "feat(desktop): edit API settings, variables and environments"
```

---

### Task 7: The secrets screen

**Files:**
- Modify: `crates/core/src/secrets.rs`, `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/src/dto.rs`, `apps/desktop/src/lib/ipc.ts`
- Create: `apps/desktop/src/lib/SecretsScreen.svelte`
- Test: `crates/core/tests/secrets.rs`, `apps/desktop/src-tauri/tests/commands.rs`

**Interfaces:**
- Produces, in core:
  ```rust
  impl Secrets {
      pub fn names(&self) -> Vec<String>;                  // sorted
      pub fn set(&mut self, name: &str, value: String);
      pub fn remove(&mut self, name: &str) -> bool;
      pub fn save(&self, paths: &Paths) -> std::io::Result<()>;  // 0600, atomic
  }
  ```
- Produces, over IPC:
  ```
  list_secrets(state) -> string[]                       // names only
  set_secret(state, name, value) -> Result<(), String>
  delete_secret(state, name) -> Result<(), String>
  reveal_secret(state, name) -> Result<string, String>  // the ONE unmasked path
  ```

**Security contract — read this before writing a line.** `list_secrets` returns names, never values. `reveal_secret` is the only function in the entire application that returns a stored credential to the webview, it returns exactly one value per call, and it is called only from an explicit user action on this screen. It must NOT be used by any other component, and the value it returns must not be stored in `ui` state that outlives the screen.

- [ ] **Step 1: Write the failing core tests**

```rust
#[test]
fn set_and_save_writes_an_0600_file_that_loads_back() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let mut s = Secrets::empty();
    s.set("GW_PASS", "hunter2".into());
    s.save(&paths).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(paths.secrets_file()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "secrets file must not be group/world readable");
    }

    let back = Secrets::load(&paths);
    assert_eq!(back.get("GW_PASS"), Some("hunter2"));
}

#[test]
fn saving_never_leaves_a_readable_temp_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::at(dir.path());
    let mut s = Secrets::empty();
    s.set("A", "b".into());
    s.save(&paths).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

#[test]
fn remove_reports_whether_anything_was_removed() {
    let mut s = Secrets::from_map([("A".to_string(), "1".to_string())]);
    assert!(s.remove("A"));
    assert!(!s.remove("A"));
    assert!(s.names().is_empty());
}
```

- [ ] **Step 2: Write the failing boundary test**

```rust
/// The whole point of `list_secrets` returning names: a screen that lists
/// secrets must not ship their values to the webview to do it.
#[tokio::test]
async fn listing_secrets_never_returns_a_value() {
    let (_d, state) = state_with_secrets(&[("GW_PASS", "hunter2")]);
    let names = commands::list_secrets_inner(&state);
    assert_eq!(names, vec!["GW_PASS".to_string()]);
    let json = serde_json::to_string(&names).unwrap();
    assert!(!json.contains("hunter2"));
}

#[tokio::test]
async fn revealing_returns_exactly_one_value_and_only_by_name() {
    let (_d, state) = state_with_secrets(&[("A", "one"), ("B", "two")]);
    assert_eq!(commands::reveal_secret_inner(&state, "A").unwrap(), "one");
    assert!(commands::reveal_secret_inner(&state, "nope").is_err());
}

/// A secret set through the UI must be usable by a run immediately — the
/// executor holds its own `Secrets` snapshot, and c1769c1 fixed exactly this
/// class of staleness for rotation. Setting one here goes through the same
/// reload path.
#[tokio::test]
async fn a_secret_set_through_the_command_is_visible_to_the_next_run() { /* wiremock */ }
```

- [ ] **Step 3: Implement**

`save` mirrors `history::append`: serialize, write a temp file in the same directory **created with mode 0600 at open time** (not chmod'ed afterwards — the phase-1 review caught that exact bug in the token cache), then rename. `set_secret_inner` and `delete_secret_inner` must call `state.reload()` so the executor's snapshot follows.

- [ ] **Step 4: Build the screen**

Reached from a control in the sidebar footer. A row per name: the name, a masked value (`••••••••`, fixed width — never the real length, which leaks something), a **Show** toggle, an **Edit** action, and a **Delete** action behind a confirm. Plus a row to add a new one.

**Show** calls `reveal_secret`, displays the value, and re-masks when the screen is left or the toggle is clicked again. Only one may be revealed at a time. The revealed value lives in a local `$state` inside the component and is cleared in `onDestroy` — never in `ui`.

Below the list, show every `{{secret:NAME}}` referenced by a workspace file that has no entry yet, so a missing secret is visible before a request fails with it.

- [ ] **Step 5: Verify**

Stubbed-bridge check of the screen, plus a real `pnpm tauri build` run confirming a secret can be added, revealed, and used by a request. Screenshot.

- [ ] **Step 6: Commit**

```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
pnpm --dir apps/desktop test && pnpm --dir apps/desktop build
git add -A
git commit -m "feat(desktop): manage secrets from the app"
```

---

## Deliberately not in phase 2.5

- Renaming an API's `id`, or moving an endpoint between APIs.
- Reordering endpoints by drag.
- A reveal for `copy as curl` — a separate surface and a separate decision (spec §17.3).
- Anything from the spec's §2 non-goals.
- Packaging — still phase 3.

## Carried debt this phase should not forget

- History stores response bodies uncapped; a large response still locks the History strip (the response pane is capped, the history one is not).
- A dead host blocks Save for up to 30 s behind the executor mutex, silently.
- CodeMirror's undo history is not reset when the selected API changes.
