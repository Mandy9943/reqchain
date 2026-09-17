<script lang="ts">
  // The one screen in the whole app where a stored credential may render
  // unmasked (spec §17.3, task-7 brief). Everything here follows the
  // security contract: `listSecrets` never carries a value; `revealSecret`
  // is called ONLY from this screen's own "Show" action, its result lives
  // only in `revealedValue` below (component-local `$state`, never `ui`),
  // only one entry may be revealed at a time, and `onDestroy` clears it so
  // leaving the screen re-masks unconditionally — even if a network error or
  // a bug elsewhere skipped the toggle's own re-mask path.
  import { onDestroy } from "svelte";
  import {
    deleteSecret,
    listSecrets,
    revealSecret,
    setSecret,
  } from "./ipc";
  import { missingSecretRefs } from "./secretRefs";
  import { ui } from "./state.svelte";

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  // Fixed-width placeholder — deliberately NOT sized to the real value's
  // length, which is itself information a shared screen or a screenshot
  // could leak.
  const MASK = "••••••••";

  let names = $state<string[]>([]);
  let loadError = $state<string | null>(null);
  let loading = $state(true);

  // The ONLY place a revealed credential is held. Local, component-scoped,
  // never assigned into `ui`. `revealedName` names which row it belongs to;
  // `revealedValue` is the value itself. Revealing a different row simply
  // overwrites both, which is what "only one may be revealed at once" means
  // in practice — there is nowhere for a second one to live.
  let revealedName = $state<string | null>(null);
  let revealedValue = $state<string | null>(null);
  let revealErrorName = $state<string | null>(null);
  let revealError = $state<string | null>(null);
  let revealBusy = $state<string | null>(null);

  function reMask(): void {
    revealedName = null;
    revealedValue = null;
    revealErrorName = null;
    revealError = null;
  }

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      names = (await listSecrets()).slice().sort();
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function toggleShow(name: string): Promise<void> {
    if (revealedName === name) {
      reMask();
      return;
    }
    // Revealing a new row implicitly re-masks whatever was previously
    // shown — there is only one `revealedValue` slot.
    reMask();
    revealBusy = name;
    try {
      revealedValue = await revealSecret(name);
      revealedName = name;
    } catch (e) {
      revealErrorName = name;
      revealError = e instanceof Error ? e.message : String(e);
    } finally {
      revealBusy = null;
    }
  }

  // --- Add / edit ------------------------------------------------------
  let editingName = $state<string | null>(null);
  let editValue = $state("");
  let editBusy = $state(false);
  let editError = $state<string | null>(null);

  let addingNew = $state(false);
  let newName = $state("");
  let newValue = $state("");
  let newError = $state<string | null>(null);
  let newBusy = $state(false);

  function startAdd(): void {
    addingNew = true;
    newName = "";
    newValue = "";
    newError = null;
  }

  function cancelAdd(): void {
    addingNew = false;
    newName = "";
    newValue = "";
    newError = null;
  }

  async function submitAdd(): Promise<void> {
    const name = newName.trim();
    if (!name) {
      newError = "Enter a name.";
      return;
    }
    if (names.includes(name)) {
      newError = `A secret named "${name}" already exists — use Edit instead.`;
      return;
    }
    newBusy = true;
    newError = null;
    try {
      await setSecret(name, newValue);
      await load();
      cancelAdd();
    } catch (e) {
      newError = e instanceof Error ? e.message : String(e);
    } finally {
      newBusy = false;
    }
  }

  function startEdit(name: string): void {
    // Editing does not pre-fill the current value — that would be a second,
    // implicit reveal outside the explicit "Show" action. The user types a
    // new value; if they need to see the old one first, they use Show.
    editingName = name;
    editValue = "";
    editError = null;
  }

  function cancelEdit(): void {
    editingName = null;
    editValue = "";
    editError = null;
  }

  async function submitEdit(name: string): Promise<void> {
    editBusy = true;
    editError = null;
    try {
      await setSecret(name, editValue);
      if (revealedName === name) {
        // The old revealed value is now stale — never show it as current.
        reMask();
      }
      cancelEdit();
    } catch (e) {
      editError = e instanceof Error ? e.message : String(e);
    } finally {
      editBusy = false;
    }
  }

  // --- Delete (destructive, behind a confirm) ---------------------------
  let confirmingDelete = $state<string | null>(null);
  let deleteBusy = $state(false);
  let deleteError = $state<string | null>(null);

  function startDelete(name: string): void {
    confirmingDelete = name;
    deleteError = null;
  }

  function cancelDelete(): void {
    confirmingDelete = null;
    deleteError = null;
  }

  async function confirmDelete(name: string): Promise<void> {
    deleteBusy = true;
    deleteError = null;
    try {
      await deleteSecret(name);
      if (revealedName === name) {
        reMask();
      }
      await load();
      confirmingDelete = null;
    } catch (e) {
      deleteError = e instanceof Error ? e.message : String(e);
    } finally {
      deleteBusy = false;
    }
  }

  // Every `{{secret:NAME}}` referenced by a loaded workspace file that has
  // no entry among `names` yet — computed from the raw text already sitting
  // in `ui.workspace.apis`, so no extra Rust command is needed for this.
  let missing = $derived(
    missingSecretRefs(
      ui.workspace.apis.map((a) => a.text),
      names,
    ),
  );

  void load();

  onDestroy(reMask);
</script>

<div class="secrets-screen">
  <header class="secrets-header">
    <h2>Secrets</h2>
    <button type="button" class="close-button" onclick={onClose}>
      Close
    </button>
  </header>

  <p class="hint">
    Values are stored in plaintext at <code>secrets.json</code>, outside the
    workspace. This screen exists so you can check a stored credential
    without a terminal — reveal one entry at a time; leaving this screen
    re-masks everything.
  </p>

  {#if loading}
    <p class="loading">Loading…</p>
  {:else if loadError}
    <p class="inline-error">{loadError}</p>
  {:else}
    <ul class="secret-list">
      {#each names as name (name)}
        <li class="secret-row">
          <div class="secret-main">
            <span class="secret-name">{name}</span>
            {#if revealedName === name && revealedValue !== null}
              <span class="secret-value revealed">{revealedValue}</span>
            {:else}
              <span class="secret-value masked" aria-label="hidden value"
                >{MASK}</span
              >
            {/if}
          </div>
          <div class="secret-actions">
            <button
              type="button"
              class="link-button"
              disabled={revealBusy === name}
              onclick={() => toggleShow(name)}
            >
              {#if revealedName === name}
                Hide
              {:else if revealBusy === name}
                Loading…
              {:else}
                Show
              {/if}
            </button>
            <button
              type="button"
              class="link-button"
              onclick={() => startEdit(name)}
            >
              Edit
            </button>
            <button
              type="button"
              class="link-button link-button-danger"
              onclick={() => startDelete(name)}
            >
              Delete
            </button>
          </div>

          {#if revealErrorName === name && revealError}
            <p class="inline-error">{revealError}</p>
          {/if}

          {#if editingName === name}
            <div class="edit-row">
              <input
                type="text"
                class="edit-input"
                placeholder="New value"
                aria-label={`New value for ${name}`}
                bind:value={editValue}
                disabled={editBusy}
              />
              <button
                type="button"
                class="confirm-button"
                disabled={editBusy}
                onclick={() => submitEdit(name)}
              >
                {editBusy ? "Saving…" : "Save"}
              </button>
              <button
                type="button"
                class="cancel-button"
                disabled={editBusy}
                onclick={cancelEdit}
              >
                Cancel
              </button>
            </div>
            {#if editError}
              <p class="inline-error">{editError}</p>
            {/if}
          {/if}

          {#if confirmingDelete === name}
            <div class="confirm-row confirm-row-danger">
              <p class="confirm-text">
                Delete secret <strong>{name}</strong>? Anything still
                referencing <code>{`{{secret:${name}}}`}</code> will fail the
                next time it runs.
              </p>
              {#if deleteError}
                <p class="inline-error">{deleteError}</p>
              {/if}
              <div class="confirm-buttons">
                <button
                  type="button"
                  class="danger-button"
                  disabled={deleteBusy}
                  onclick={() => confirmDelete(name)}
                >
                  {deleteBusy ? "Deleting…" : "Delete"}
                </button>
                <button
                  type="button"
                  class="cancel-button"
                  disabled={deleteBusy}
                  onclick={cancelDelete}
                >
                  Cancel
                </button>
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>

    {#if names.length === 0}
      <p class="empty-state">No secrets stored yet.</p>
    {/if}

    <div class="new-secret">
      {#if addingNew}
        <div class="new-row">
          <input
            type="text"
            class="new-input"
            placeholder="Name (e.g. GW_PASS)"
            aria-label="New secret name"
            bind:value={newName}
            disabled={newBusy}
          />
          <input
            type="text"
            class="new-input"
            placeholder="Value"
            aria-label="New secret value"
            bind:value={newValue}
            disabled={newBusy}
          />
          <button
            type="button"
            class="confirm-button"
            disabled={newBusy}
            onclick={submitAdd}
          >
            {newBusy ? "Adding…" : "Add"}
          </button>
          <button
            type="button"
            class="cancel-button"
            disabled={newBusy}
            onclick={cancelAdd}
          >
            Cancel
          </button>
        </div>
        {#if newError}
          <p class="inline-error">{newError}</p>
        {/if}
      {:else}
        <button type="button" class="new-secret-button" onclick={startAdd}>
          + New secret
        </button>
      {/if}
    </div>

    {#if missing.length > 0}
      <div class="missing-secrets">
        <h3>Referenced but not set</h3>
        <ul>
          {#each missing as name (name)}
            <li><code>{`{{secret:${name}}}`}</code></li>
          {/each}
        </ul>
      </div>
    {/if}
  {/if}
</div>

<style>
  .secrets-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow-y: auto;
    padding: 1rem 1.25rem;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .secrets-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.5rem;
  }

  .secrets-header h2 {
    margin: 0;
    font-size: 1.1rem;
  }

  .close-button {
    padding: 0.3rem 0.7rem;
    font-size: 0.8rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  .hint {
    margin: 0 0 1rem;
    font-size: 0.8rem;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  .hint code {
    font-size: 0.78rem;
  }

  .loading {
    font-size: 0.85rem;
    color: var(--color-text-muted);
  }

  .secret-list {
    list-style: none;
    margin: 0 0 0.75rem;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .secret-row {
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0.5rem 0.6rem;
    background: var(--color-surface);
  }

  .secret-main {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .secret-name {
    font-weight: 600;
    font-size: 0.88rem;
    flex-shrink: 0;
    min-width: 8rem;
  }

  .secret-value {
    font-family: monospace;
    font-size: 0.85rem;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .secret-value.masked {
    color: var(--color-text-muted);
    letter-spacing: 0.1em;
  }

  .secret-value.revealed {
    color: var(--color-text);
  }

  .secret-actions {
    display: flex;
    gap: 0.6rem;
    margin-top: 0.35rem;
  }

  .link-button {
    padding: 0;
    font-size: 0.78rem;
    border: none;
    background: none;
    color: var(--color-accent);
    cursor: pointer;
  }

  .link-button-danger {
    color: var(--color-error-text);
  }

  .edit-row,
  .new-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-top: 0.4rem;
  }

  .edit-input,
  .new-input {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.45rem;
    font-size: 0.82rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .confirm-button,
  .cancel-button,
  .danger-button {
    flex-shrink: 0;
    padding: 0.25rem 0.55rem;
    font-size: 0.78rem;
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

  .confirm-row {
    margin-top: 0.4rem;
    padding: 0.4rem 0.5rem;
    border-radius: 4px;
    font-size: 0.8rem;
  }

  .confirm-row-danger {
    background: var(--color-error-bg);
    border: 1px solid var(--color-error-text);
    color: var(--color-error-text);
  }

  .confirm-text {
    margin: 0 0 0.35rem;
  }

  .confirm-text code {
    word-break: break-all;
  }

  .confirm-buttons {
    display: flex;
    gap: 0.4rem;
  }

  .inline-error {
    margin: 0.35rem 0 0;
    font-size: 0.78rem;
    color: var(--color-error-text);
  }

  .empty-state {
    font-size: 0.85rem;
    color: var(--color-text-muted);
  }

  .new-secret {
    margin-top: 0.25rem;
  }

  .new-secret-button {
    padding: 0.35rem 0.6rem;
    font-size: 0.8rem;
    border: 1px dashed var(--color-border);
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .new-secret-button:hover {
    background: var(--color-hover);
  }

  .missing-secrets {
    margin-top: 1.25rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--color-border);
  }

  .missing-secrets h3 {
    margin: 0 0 0.4rem;
    font-size: 0.85rem;
    color: var(--color-text-muted);
  }

  .missing-secrets ul {
    margin: 0;
    padding-left: 1.1rem;
    font-size: 0.82rem;
    color: var(--color-error-text);
  }

  .missing-secrets code {
    font-size: 0.8rem;
  }
</style>
