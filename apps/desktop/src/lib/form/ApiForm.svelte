<script lang="ts">
  import { updateDoc } from "../doc.svelte";
  import type { Api, Auth, Environment } from "../model";
  import { ui } from "../state.svelte";
  import AuthEditor from "./AuthEditor.svelte";
  import KeyValueRows from "./KeyValueRows.svelte";
  import {
    asEnvironments,
    emptyEnvironment,
    nextEnvSelection,
    validateEnvironmentName,
  } from "./environments";
  import { setApiField } from "./fieldOrder";

  // `api` is the already-parsed document `RequestPanel` computed (its own
  // removed-tolerant `api`/`bufferText` fallbacks) — passed in rather than
  // re-derived here, for exactly the reason `EndpointForm`'s doc comment
  // gives for doing the same: re-deriving via `currentDoc()` would go
  // `undefined` for a removed-but-still-displayed selection and dead-end the
  // default tab.
  let { api }: { api: Api } = $props();

  /** Runs `fn` against the live document `updateDoc` hands us, never
   * against the `api` snapshot read above — same pattern `EndpointForm`'s
   * `mutateEndpoint` uses, just with nothing to look up first: there is
   * exactly one `Api` per document. */
  function mutateApi(fn: (a: Api) => void): void {
    updateDoc(fn);
  }

  /** After any environment add/rename/remove, keeps `ui.env[apiId]` from
   * pointing at a name that no longer exists — the same rule `reload()`
   * already applies once a fresh workspace load arrives, applied
   * immediately instead of leaving a dangling selection in the interim
   * until the buffer is saved and reloaded. Reads the environments back off
   * the just-mutated buffer (via `updateDoc`'s own re-parse), not off the
   * stale `api` prop, so it reflects the edit that was just made. */
  function fixEnvSelection(): void {
    updateDoc((a) => {
      const current = ui.env[a.id] ?? null;
      ui.env[a.id] = nextEnvSelection(asEnvironments(a.environments), current);
    });
  }

  function onNameChange(e: Event): void {
    const value = (e.currentTarget as HTMLInputElement).value;
    mutateApi((a) => {
      a.name = value;
    });
  }

  function onBaseUrlChange(e: Event): void {
    const value = (e.currentTarget as HTMLInputElement).value;
    mutateApi((a) => {
      a.baseUrl = value;
    });
  }

  function onVariablesChange(variables: Record<string, string>): void {
    // `variables` has a serde default with no `skip_serializing_if`, so it
    // is legally ABSENT from a hand-written or older file even though
    // `emptyApi` always writes it — `setApiField` inserts it at its
    // declared position (see fieldOrder.ts) instead of a plain `a.variables
    // = ...`, which would append it after `endpoints` on such a file.
    mutateApi((a) => {
      setApiField(a, "variables", variables);
    });
  }

  function onAuthChange(auth: Auth): void {
    // Same reasoning as `onVariablesChange` — `auth` has a serde default
    // (`Auth::none`) with no `skip_serializing_if`.
    mutateApi((a) => {
      setApiField(a, "auth", auth);
    });
  }

  const storeBodies = $derived(
    typeof api.history?.storeBodies === "boolean" ? api.history.storeBodies : true,
  );

  function onStoreBodiesChange(e: Event): void {
    const checked = (e.currentTarget as HTMLInputElement).checked;
    mutateApi((a) => {
      // `history` is optional (`Option<HistoryConfig>`, no
      // `skip_serializing_if` other than "is None") and `storeBodies`
      // defaults to `true` — so the true/default case simply omits the
      // key, matching what `emptyApi` already does, rather than writing an
      // explicit value that merely equals the default. Deleting an
      // already-present key never moves any other key's position, so no
      // helper is needed for that half; `setApiField` handles the
      // insertion half (see fieldOrder.ts).
      if (checked) {
        delete a.history;
      } else {
        setApiField(a, "history", { storeBodies: false });
      }
    });
  }

  // --- Environments: add / rename / remove ----------------------------------
  const environments = $derived(asEnvironments(api.environments));

  /**
   * Returns `a.environments` as a real, live, mutable array — the actual
   * array reference already on the document whenever one is already there
   * (so mutating it in place, e.g. via `.splice`, or mutating one of its
   * entries directly, e.g. `env.name = ...`, keeps every OTHER field's
   * position untouched), falling back to a freshly-inserted empty array
   * only when `environments` is genuinely absent or not an array at all —
   * legal per model.rs's `#[serde(default)]` (an older/hand-written file
   * may omit it, or a JSON-tab edit may have corrupted it), just never
   * produced by this app itself (`emptyApi` always writes `[]`). Routes
   * that fallback through `setApiField` so it lands at `environments`'s
   * declared position (between `variables` and `auth`), not appended after
   * `endpoints`.
   */
  function liveEnvironments(a: Api): Environment[] {
    if (Array.isArray(a.environments)) return a.environments;
    setApiField(a, "environments", []);
    return a.environments;
  }

  let addingEnvironment = $state(false);
  let newEnvironmentName = $state("");
  let newEnvironmentError = $state<string | null>(null);

  function startAddEnvironment(): void {
    addingEnvironment = true;
    newEnvironmentName = "";
    newEnvironmentError = null;
  }

  function cancelAddEnvironment(): void {
    addingEnvironment = false;
    newEnvironmentName = "";
    newEnvironmentError = null;
  }

  function submitAddEnvironment(): void {
    const result = validateEnvironmentName(environments, newEnvironmentName, null);
    if (!result.ok) {
      newEnvironmentError = result.error;
      return;
    }
    mutateApi((a) => {
      liveEnvironments(a).push(emptyEnvironment(result.name));
    });
    addingEnvironment = false;
    newEnvironmentName = "";
    newEnvironmentError = null;
  }

  function onNewEnvironmentKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      submitAddEnvironment();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelAddEnvironment();
    }
  }

  let renamingIndex = $state<number | null>(null);
  let renameDraft = $state("");
  let renameError = $state<string | null>(null);

  function startRenameEnvironment(index: number, currentName: string): void {
    renamingIndex = index;
    renameDraft = currentName;
    renameError = null;
  }

  function cancelRenameEnvironment(): void {
    renamingIndex = null;
    renameDraft = "";
    renameError = null;
  }

  function submitRenameEnvironment(): void {
    if (renamingIndex === null) return;
    const index = renamingIndex;
    const result = validateEnvironmentName(environments, renameDraft, index);
    if (!result.ok) {
      renameError = result.error;
      return;
    }
    const oldName = environments[index]?.name;
    mutateApi((a) => {
      const env = liveEnvironments(a)[index];
      if (!env) return;
      env.name = result.name;
    });
    if (oldName !== undefined && oldName !== result.name) {
      // The renamed-away name may have been the one selected in the
      // sidebar's environment picker.
      fixEnvSelection();
    }
    renamingIndex = null;
    renameDraft = "";
    renameError = null;
  }

  function onRenameKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      submitRenameEnvironment();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelRenameEnvironment();
    }
  }

  let confirmingRemoveIndex = $state<number | null>(null);

  function startRemoveEnvironment(index: number): void {
    confirmingRemoveIndex = index;
  }

  function cancelRemoveEnvironment(): void {
    confirmingRemoveIndex = null;
  }

  function confirmRemoveEnvironment(index: number): void {
    mutateApi((a) => {
      liveEnvironments(a).splice(index, 1);
    });
    fixEnvSelection();
    confirmingRemoveIndex = null;
  }

  function onEnvironmentVariablesChange(
    index: number,
    variables: Record<string, string>,
  ): void {
    mutateApi((a) => {
      const env = liveEnvironments(a)[index];
      if (!env) return;
      env.variables = variables;
    });
  }
</script>

<div class="api-form">
  <div class="field-row">
    <label for="api-id">Id</label>
    <input id="api-id" type="text" value={api.id} disabled />
  </div>
  <p class="hint">
    The id is the file's identity — renaming it is not offered here. To move
    this API to a different id, create a new API and delete this one.
  </p>

  <div class="field-row">
    <label for="api-name">Name</label>
    <input id="api-name" type="text" value={api.name} oninput={onNameChange} />
  </div>

  <div class="field-row">
    <label for="api-base-url">Base URL</label>
    <input
      id="api-base-url"
      class="mono"
      type="text"
      value={api.baseUrl}
      oninput={onBaseUrlChange}
    />
  </div>

  <section class="fields-section">
    <h3>Variables</h3>
    <KeyValueRows
      rows={api.variables}
      onChange={onVariablesChange}
      keyLabel="Name"
      valueLabel="Value"
    />
  </section>

  <section class="fields-section">
    <h3>Environments</h3>
    <ul class="environments">
      {#each environments as env, i (i)}
        <li class="environment">
          <div class="environment-header">
            {#if renamingIndex === i}
              <input
                type="text"
                class="rename-input"
                bind:value={renameDraft}
                onkeydown={onRenameKeydown}
                aria-label={`Rename environment ${env.name}`}
              />
              <button type="button" class="confirm-button" onclick={submitRenameEnvironment}>
                Save
              </button>
              <button type="button" class="cancel-button" onclick={cancelRenameEnvironment}>
                Cancel
              </button>
            {:else}
              <span class="environment-name">{env.name}</span>
              <button
                type="button"
                class="link-button"
                onclick={() => startRenameEnvironment(i, env.name)}
              >
                Rename
              </button>
              <button
                type="button"
                class="link-button link-button-danger"
                onclick={() => startRemoveEnvironment(i)}
              >
                Remove
              </button>
            {/if}
          </div>
          {#if renamingIndex === i && renameError}
            <p class="inline-error">{renameError}</p>
          {/if}
          {#if confirmingRemoveIndex === i}
            <div class="confirm-row confirm-row-danger">
              <p class="confirm-text">
                Remove environment <strong>{env.name}</strong> and its variables?
              </p>
              <div class="confirm-buttons">
                <button
                  type="button"
                  class="danger-button"
                  onclick={() => confirmRemoveEnvironment(i)}
                >
                  Remove
                </button>
                <button type="button" class="cancel-button" onclick={cancelRemoveEnvironment}>
                  Cancel
                </button>
              </div>
            </div>
          {/if}
          <div class="environment-variables">
            <KeyValueRows
              rows={env.variables}
              onChange={(vars) => onEnvironmentVariablesChange(i, vars)}
              keyLabel="Name"
              valueLabel="Value"
            />
          </div>
        </li>
      {/each}
    </ul>

    {#if addingEnvironment}
      <div class="new-row">
        <input
          type="text"
          class="new-input"
          placeholder="Environment name"
          aria-label="New environment name"
          bind:value={newEnvironmentName}
          onkeydown={onNewEnvironmentKeydown}
        />
        <button type="button" class="confirm-button" onclick={submitAddEnvironment}>
          Add
        </button>
        <button type="button" class="cancel-button" onclick={cancelAddEnvironment}>
          Cancel
        </button>
      </div>
      {#if newEnvironmentError}
        <p class="inline-error">{newEnvironmentError}</p>
      {/if}
    {:else}
      <button type="button" class="new-environment-button" onclick={startAddEnvironment}>
        + New environment
      </button>
    {/if}
  </section>

  <section class="fields-section">
    <h3>Auth</h3>
    <p class="hint">
      This is the default auth every endpoint inherits unless it sets its
      own. There is nothing above the API level, so "inherit" is not offered
      here.
    </p>
    {#key api.id}
      <!-- Same reasoning as `EndpointForm`'s `{#key endpointId}` around its
           own `AuthEditor` (task 5's report flags this explicitly as task
           6's job): nothing in `AuthEditor`/`ChainedAuthBuilder` holds local
           state on its own, but the `header` auth type's `KeyValueRows`
           does, and `ApiForm` stays mounted across a switch between two
           DIFFERENT APIs (the `{#if isApiOnlySelected}` block in
           RequestPanel does not unmount just because `api.id` changed).
           Keying on `api.id` forces a remount on every such switch. -->
      <AuthEditor
        auth={api.auth}
        onChange={onAuthChange}
        {api}
        endpointId={null}
        allowInherit={false}
      />
    {/key}
  </section>

  <section class="fields-section">
    <h3>History</h3>
    <label class="checkbox-row">
      <input type="checkbox" checked={storeBodies} onchange={onStoreBodiesChange} />
      Store request/response bodies in history
    </label>
  </section>
</div>

<style>
  .api-form {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    overflow-y: auto;
    min-height: 0;
    padding: 0.25rem 0;
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .field-row label {
    flex-shrink: 0;
    width: 6rem;
    font-size: 0.8rem;
    color: var(--color-text-muted);
  }

  .field-row input {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.4rem;
    font-size: 0.85rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .field-row input:disabled {
    color: var(--color-text-muted);
    cursor: not-allowed;
  }

  .mono {
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }

  .hint {
    margin: 0;
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .fields-section h3 {
    margin: 0 0 0.4rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--color-text-muted);
  }

  .environments {
    list-style: none;
    margin: 0 0 0.5rem;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .environment {
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0.5rem 0.6rem;
  }

  .environment-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .environment-name {
    flex: 1;
    font-weight: 600;
    font-size: 0.85rem;
  }

  .rename-input {
    flex: 1;
    min-width: 0;
    padding: 0.25rem 0.4rem;
    font-size: 0.8rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .environment-variables {
    margin-top: 0.5rem;
  }

  .link-button {
    padding: 0;
    font-size: 0.75rem;
    border: none;
    background: none;
    color: var(--color-accent);
    cursor: pointer;
  }

  .link-button-danger {
    color: var(--color-error-text);
  }

  .new-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .new-input {
    flex: 1;
    min-width: 0;
    padding: 0.25rem 0.4rem;
    font-size: 0.8rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .confirm-button,
  .cancel-button,
  .danger-button {
    flex-shrink: 0;
    padding: 0.2rem 0.5rem;
    font-size: 0.75rem;
    border-radius: 4px;
    cursor: pointer;
  }

  .confirm-button {
    border: 1px solid var(--color-accent);
    background: var(--color-accent);
    color: #fff;
  }

  .cancel-button {
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .danger-button {
    border: 1px solid var(--color-error-text);
    background: var(--color-error-bg);
    color: var(--color-error-text);
  }

  .new-environment-button {
    width: 100%;
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    border: 1px dashed var(--color-border);
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .new-environment-button:hover {
    background: var(--color-hover);
  }

  .confirm-row {
    margin: 0.4rem 0;
    padding: 0.4rem 0.5rem;
    border-radius: 4px;
    font-size: 0.78rem;
  }

  .confirm-row-danger {
    background: var(--color-error-bg);
    border: 1px solid var(--color-error-text);
    color: var(--color-error-text);
  }

  .confirm-text {
    margin: 0 0 0.35rem;
  }

  .confirm-buttons {
    display: flex;
    gap: 0.4rem;
  }

  .inline-error {
    margin: 0.3rem 0 0;
    font-size: 0.75rem;
    color: var(--color-error-text);
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
    color: var(--color-text);
  }
</style>
