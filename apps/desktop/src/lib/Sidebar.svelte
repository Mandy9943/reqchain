<script lang="ts">
  import { deleteApi, saveApi, type ApiDto, type EndpointDto } from "./ipc";
  import { emptyApi, emptyEndpoint, serializeApi, slugify } from "./model";
  import { currentDoc, updateDoc } from "./doc.svelte";
  import { reload, select, ui } from "./state.svelte";

  function matches(ep: EndpointDto, query: string): boolean {
    if (!query) return true;
    const q = query.toLowerCase();
    return (
      ep.id.toLowerCase().includes(q) ||
      ep.name.toLowerCase().includes(q) ||
      ep.path.toLowerCase().includes(q)
    );
  }

  function visibleEndpoints(api: ApiDto): EndpointDto[] {
    return api.endpoints.filter((ep) => matches(ep, ui.search));
  }

  function selectEndpoint(api: ApiDto, endpoint: EndpointDto): void {
    select({ apiId: api.id, endpointId: endpoint.id });
  }

  /** Selects the API's own header row — `endpointId: null` (see state.svelte.ts). */
  function selectApi(api: ApiDto): void {
    select({ apiId: api.id, endpointId: null });
  }

  function isEndpointSelected(api: ApiDto, endpoint: EndpointDto): boolean {
    return (
      ui.selected?.apiId === api.id && ui.selected?.endpointId === endpoint.id
    );
  }

  function isApiSelected(api: ApiDto): boolean {
    return ui.selected?.apiId === api.id && ui.selected?.endpointId === null;
  }

  /** Whether this API's editor buffer holds an edit not yet saved to disk. */
  function isDirty(api: ApiDto): boolean {
    const buffer = ui.buffers[api.id];
    return buffer !== undefined && buffer !== api.text;
  }

  // --- Inline "New API" row -------------------------------------------------
  let creatingApi = $state(false);
  let newApiName = $state("");
  let newApiError = $state<string | null>(null);
  let newApiBusy = $state(false);

  function startCreateApi(): void {
    creatingApi = true;
    newApiName = "";
    newApiError = null;
  }

  function cancelCreateApi(): void {
    creatingApi = false;
    newApiName = "";
    newApiError = null;
  }

  async function submitCreateApi(): Promise<void> {
    const trimmed = newApiName.trim();
    if (!trimmed) {
      newApiError = "Enter a name.";
      return;
    }
    const id = slugify(trimmed);
    if (!id) {
      newApiError = "That name has no letters or numbers to build an id from.";
      return;
    }
    if (ui.workspace.apis.some((a) => a.id === id)) {
      newApiError = `An API with id "${id}" already exists.`;
      return;
    }
    newApiBusy = true;
    newApiError = null;
    try {
      const diags = await saveApi(id, serializeApi(emptyApi(id, trimmed)));
      const errors = diags.filter((d) => d.severity === "error");
      if (errors.length > 0) {
        newApiError = errors.map((d) => d.message).join("; ");
        return;
      }
      await reload();
      select({ apiId: id, endpointId: null });
      creatingApi = false;
      newApiName = "";
    } catch (e) {
      newApiError = e instanceof Error ? e.message : String(e);
    } finally {
      newApiBusy = false;
    }
  }

  // A Svelte action, not the `autofocus` attribute: the attribute trips
  // svelte-check's a11y_autofocus lint (the build must stay zero-warning),
  // and an action expresses the same "focus this input when this inline
  // row appears" intent without it.
  function autofocus(node: HTMLInputElement): void {
    node.focus();
  }

  function onNewApiKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void submitCreateApi();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelCreateApi();
    }
  }

  // --- Inline "New endpoint" row, per API -----------------------------------
  let creatingEndpointFor = $state<string | null>(null);
  let newEndpointName = $state("");
  let newEndpointError = $state<string | null>(null);
  let newEndpointBusy = $state(false);

  function startCreateEndpoint(api: ApiDto): void {
    creatingEndpointFor = api.id;
    newEndpointName = "";
    newEndpointError = null;
  }

  function cancelCreateEndpoint(): void {
    creatingEndpointFor = null;
    newEndpointName = "";
    newEndpointError = null;
  }

  async function submitCreateEndpoint(api: ApiDto): Promise<void> {
    const trimmed = newEndpointName.trim();
    if (!trimmed) {
      newEndpointError = "Enter a name.";
      return;
    }
    const id = slugify(trimmed);
    if (!id) {
      newEndpointError =
        "That name has no letters or numbers to build an id from.";
      return;
    }

    newEndpointBusy = true;
    newEndpointError = null;
    try {
      // `updateDoc` mutates the buffer of whatever API is currently
      // selected, so select this one first — this also means any unsaved
      // edit already sitting in this API's JSON buffer is carried along
      // into the save below, per the "one buffer" rule (constraints.md).
      select({ apiId: api.id, endpointId: null });
      const doc = currentDoc();
      if (!doc || !doc.ok) {
        newEndpointError =
          "This API's buffer is not valid JSON — fix it in the JSON tab first.";
        return;
      }
      if (doc.api.endpoints.some((ep) => ep.id === id)) {
        newEndpointError = `An endpoint with id "${id}" already exists.`;
        return;
      }
      updateDoc((mut) => {
        mut.endpoints.push(emptyEndpoint(id, trimmed));
      });
      const text = ui.buffers[api.id];
      if (text === undefined) {
        newEndpointError = "internal error: no buffer to save";
        return;
      }
      const diags = await saveApi(api.id, text);
      const errors = diags.filter((d) => d.severity === "error");
      if (errors.length > 0) {
        newEndpointError = errors.map((d) => d.message).join("; ");
        return;
      }
      await reload();
      select({ apiId: api.id, endpointId: id });
      creatingEndpointFor = null;
      newEndpointName = "";
    } catch (e) {
      newEndpointError = e instanceof Error ? e.message : String(e);
    } finally {
      newEndpointBusy = false;
    }
  }

  function onNewEndpointKeydown(event: KeyboardEvent, api: ApiDto): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void submitCreateEndpoint(api);
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelCreateEndpoint();
    }
  }

  // --- Delete API (destructive: removes a file) -----------------------------
  let confirmingDeleteApi = $state<string | null>(null);
  let deleteApiError = $state<string | null>(null);
  let deleteApiBusy = $state(false);

  /** Whether Delete can even be started for this API right now. */
  function canDeleteApi(api: ApiDto): boolean {
    // Refuse to start (or open the confirmation for) a delete while a save
    // for this same file is in flight — see `ui.savingIds`'s doc comment in
    // state.svelte.ts. Without this, `save_api` could write the file right
    // back moments after `delete_api` removed it.
    return !ui.savingIds[api.id];
  }

  function startDeleteApi(api: ApiDto): void {
    if (!canDeleteApi(api)) return;
    confirmingDeleteApi = api.id;
    deleteApiError = null;
  }

  function cancelDeleteApi(): void {
    confirmingDeleteApi = null;
    deleteApiError = null;
  }

  async function confirmDeleteApi(api: ApiDto): Promise<void> {
    if (!canDeleteApi(api)) {
      deleteApiError = "A save is in progress for this API — wait for it to finish.";
      return;
    }
    deleteApiBusy = true;
    ui.deletingIds[api.id] = true;
    deleteApiError = null;
    try {
      await deleteApi(api.id);
      if (ui.selected?.apiId === api.id) {
        select(null);
      }
      // The file is gone — drop this api's leftover per-id UI state too, or
      // it lingers forever (a dead key in three maps that nothing else ever
      // clears) and would resurface with stale content if an API with the
      // same id is ever created again.
      delete ui.buffers[api.id];
      delete ui.env[api.id];
      delete ui.diskChanged[api.id];
      await reload();
      confirmingDeleteApi = null;
    } catch (e) {
      deleteApiError = e instanceof Error ? e.message : String(e);
    } finally {
      deleteApiBusy = false;
      delete ui.deletingIds[api.id];
    }
  }

  // --- Delete endpoint (edits the API's document, does not touch a file
  //     directly — the only destructive, file-removing action is deleting
  //     an API) --------------------------------------------------------------
  // Keyed by `${apiId}/${endpointId}`, not the bare endpoint id: endpoint ids
  // are only unique WITHIN an API (nothing enforces global uniqueness), so
  // two different APIs sharing an endpoint id (e.g. both defining `ping`)
  // would otherwise both show their delete confirmation at once.
  let confirmingDeleteEndpoint = $state<string | null>(null);
  let deleteEndpointError = $state<string | null>(null);
  let deleteEndpointBusy = $state(false);

  function endpointKey(api: ApiDto, endpoint: EndpointDto): string {
    return `${api.id}/${endpoint.id}`;
  }

  function startDeleteEndpoint(api: ApiDto, endpoint: EndpointDto): void {
    confirmingDeleteEndpoint = endpointKey(api, endpoint);
    deleteEndpointError = null;
  }

  function cancelDeleteEndpoint(): void {
    confirmingDeleteEndpoint = null;
    deleteEndpointError = null;
  }

  async function confirmDeleteEndpoint(
    api: ApiDto,
    endpoint: EndpointDto,
  ): Promise<void> {
    deleteEndpointBusy = true;
    deleteEndpointError = null;
    try {
      select({ apiId: api.id, endpointId: null });
      const doc = currentDoc();
      if (!doc || !doc.ok) {
        deleteEndpointError =
          "This API's buffer is not valid JSON — fix it in the JSON tab first.";
        return;
      }
      updateDoc((mut) => {
        mut.endpoints = mut.endpoints.filter((ep) => ep.id !== endpoint.id);
      });
      const text = ui.buffers[api.id];
      if (text === undefined) {
        deleteEndpointError = "internal error: no buffer to save";
        return;
      }
      const diags = await saveApi(api.id, text);
      const errors = diags.filter((d) => d.severity === "error");
      if (errors.length > 0) {
        deleteEndpointError = errors.map((d) => d.message).join("; ");
        return;
      }
      await reload();
      confirmingDeleteEndpoint = null;
    } catch (e) {
      deleteEndpointError = e instanceof Error ? e.message : String(e);
    } finally {
      deleteEndpointBusy = false;
    }
  }
</script>

<aside class="sidebar">
  <input
    class="search"
    type="search"
    placeholder="Search endpoints..."
    aria-label="Search endpoints"
    bind:value={ui.search}
  />

  <div class="apis">
    <!-- Keyed by INDEX, not by id. Nothing guarantees these ids are unique:
         `Workspace::load` drops a duplicate API id, but it does not lint
         ENDPOINT ids, so a file declaring the same endpoint id twice loads
         fine and reaches this list (the validator reports it as a
         diagnostic, it is not a load failure). A duplicate key makes Svelte
         throw `each_key_duplicate`, which takes down the whole window — the
         opposite of the spec's "other APIs keep working". Index keys cannot
         collide; the only cost is that rows are not reused across a
         reorder, which is invisible for a list this size. -->
    {#each ui.workspace.apis as api, apiIndex (apiIndex)}
      <section class="api">
        <div class="api-header-row">
          <button
            type="button"
            class="api-header"
            class:selected={isApiSelected(api)}
            onclick={() => selectApi(api)}
          >
            <span class="api-name">{api.name}</span>
          </button>
          {#if api.environments.length > 0}
            <select
              class="env-select"
              bind:value={ui.env[api.id]}
              aria-label={`Environment for ${api.name}`}
            >
              {#each api.environments as env (env)}
                <option value={env}>{env}</option>
              {/each}
            </select>
          {/if}
        </div>

        <div class="api-actions">
          <button
            type="button"
            class="link-button"
            onclick={() => startCreateEndpoint(api)}
          >
            + New endpoint
          </button>
          <button
            type="button"
            class="link-button link-button-danger"
            disabled={!canDeleteApi(api)}
            title={canDeleteApi(api)
              ? undefined
              : "A save is in progress for this API"}
            onclick={() => startDeleteApi(api)}
          >
            Delete
          </button>
        </div>

        {#if confirmingDeleteApi === api.id}
          <div class="confirm-row confirm-row-danger">
            <p class="confirm-text">
              Delete <strong>{api.name}</strong>? This removes the file
              <code>{api.path}</code> from disk — this cannot be undone.
            </p>
            {#if isDirty(api)}
              <p class="confirm-text">
                This API also has unsaved changes in the editor — those go
                with it too.
              </p>
            {/if}
            {#if deleteApiError}
              <p class="inline-error">{deleteApiError}</p>
            {/if}
            <div class="confirm-buttons">
              <button
                type="button"
                class="danger-button"
                disabled={deleteApiBusy || !canDeleteApi(api)}
                onclick={() => confirmDeleteApi(api)}
              >
                {deleteApiBusy ? "Deleting…" : "Delete file"}
              </button>
              <button
                type="button"
                class="cancel-button"
                disabled={deleteApiBusy}
                onclick={cancelDeleteApi}
              >
                Cancel
              </button>
            </div>
          </div>
        {/if}

        {#if creatingEndpointFor === api.id}
          <div class="new-row">
            <input
              type="text"
              class="new-input"
              placeholder="Endpoint name"
              aria-label={`New endpoint name for ${api.name}`}
              bind:value={newEndpointName}
              onkeydown={(e) => onNewEndpointKeydown(e, api)}
              disabled={newEndpointBusy}
              use:autofocus
            />
            <button
              type="button"
              class="confirm-button"
              disabled={newEndpointBusy}
              onclick={() => submitCreateEndpoint(api)}
            >
              {newEndpointBusy ? "Adding…" : "Add"}
            </button>
            <button
              type="button"
              class="cancel-button"
              disabled={newEndpointBusy}
              onclick={cancelCreateEndpoint}
            >
              Cancel
            </button>
          </div>
          {#if newEndpointError}
            <p class="inline-error">{newEndpointError}</p>
          {/if}
        {/if}

        <ul class="endpoints">
          {#each visibleEndpoints(api) as endpoint, endpointIndex (endpointIndex)}
            <li>
              <div class="endpoint-row">
                <button
                  type="button"
                  class="endpoint"
                  class:selected={isEndpointSelected(api, endpoint)}
                  onclick={() => selectEndpoint(api, endpoint)}
                >
                  <span class="method method-{endpoint.method.toLowerCase()}"
                    >{endpoint.method}</span
                  >
                  <span class="endpoint-name">{endpoint.name}</span>
                  {#if endpoint.authKind === "chained"}
                    <span class="chain-marker" title="Chained auth">chain</span
                    >
                  {/if}
                </button>
                <button
                  type="button"
                  class="link-button link-button-danger endpoint-delete"
                  onclick={() => startDeleteEndpoint(api, endpoint)}
                  aria-label={`Delete ${endpoint.name}`}
                >
                  ×
                </button>
              </div>

              {#if confirmingDeleteEndpoint === endpointKey(api, endpoint)}
                <div class="confirm-row confirm-row-danger">
                  <p class="confirm-text">
                    Remove endpoint <strong>{endpoint.name}</strong> from
                    <code>{api.name}</code>?
                  </p>
                  {#if deleteEndpointError}
                    <p class="inline-error">{deleteEndpointError}</p>
                  {/if}
                  <div class="confirm-buttons">
                    <button
                      type="button"
                      class="danger-button"
                      disabled={deleteEndpointBusy}
                      onclick={() => confirmDeleteEndpoint(api, endpoint)}
                    >
                      {deleteEndpointBusy ? "Removing…" : "Remove"}
                    </button>
                    <button
                      type="button"
                      class="cancel-button"
                      disabled={deleteEndpointBusy}
                      onclick={cancelDeleteEndpoint}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/each}

    {#if ui.workspace.apis.length === 0}
      <!-- A fresh install has no workspace files, and a bare empty panel
           gives no clue where they are meant to go. -->
      <p class="empty-state">
        No APIs yet. Create one below, or drop a JSON file into
        <code>~/.config/reqchain/workspace/apis/</code> — the app picks it up
        as soon as it is saved. <code>SPEC.md</code> describes the format, and
        <code>reqchain validate &lt;file&gt;</code> checks one.
      </p>
    {/if}

    <div class="new-api">
      {#if creatingApi}
        <div class="new-row">
          <input
            type="text"
            class="new-input"
            placeholder="API name"
            aria-label="New API name"
            bind:value={newApiName}
            onkeydown={onNewApiKeydown}
            disabled={newApiBusy}
            use:autofocus
          />
          <button
            type="button"
            class="confirm-button"
            disabled={newApiBusy}
            onclick={submitCreateApi}
          >
            {newApiBusy ? "Creating…" : "Create"}
          </button>
          <button
            type="button"
            class="cancel-button"
            disabled={newApiBusy}
            onclick={cancelCreateApi}
          >
            Cancel
          </button>
        </div>
        {#if newApiError}
          <p class="inline-error">{newApiError}</p>
        {/if}
      {:else}
        <button type="button" class="new-api-button" onclick={startCreateApi}>
          + New API
        </button>
      {/if}
    </div>
  </div>

  {#if ui.workspace.errors.length > 0}
    <div class="errors">
      {#each ui.workspace.errors as err, errIndex (errIndex)}
        <div class="error-row">
          {err.path.split("/").pop()} — {err.message}
        </div>
      {/each}
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    border-right: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .search {
    margin: 0.5rem;
    padding: 0.4rem 0.6rem;
    font-size: 0.9rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .apis {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .api {
    margin-bottom: 0.5rem;
  }

  .api-header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0 0.6rem;
  }

  .api-header {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    padding: 0.35rem 0.4rem;
    font-weight: 600;
    font-size: 0.85rem;
    color: var(--color-text-muted);
    border: none;
    background: transparent;
    text-align: left;
    cursor: pointer;
    border-radius: 3px;
  }

  .api-header:hover {
    background: var(--color-hover);
  }

  .api-header.selected {
    background: var(--color-selected);
  }

  .api-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .api-actions {
    display: flex;
    gap: 0.6rem;
    padding: 0 0.6rem 0.25rem;
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

  .endpoint-row {
    display: flex;
    align-items: center;
    gap: 0.2rem;
  }

  .endpoint-delete {
    flex-shrink: 0;
    padding: 0 0.4rem;
    font-size: 0.9rem;
    line-height: 1;
  }

  .new-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.6rem;
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

  .confirm-row {
    margin: 0.15rem 0.6rem 0.4rem;
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

  .confirm-text code {
    word-break: break-all;
  }

  .confirm-buttons {
    display: flex;
    gap: 0.4rem;
  }

  .inline-error {
    margin: 0 0.6rem 0.4rem;
    font-size: 0.75rem;
    color: var(--color-error-text);
  }

  .new-api {
    padding: 0.4rem 0.6rem;
  }

  .new-api-button {
    width: 100%;
    padding: 0.35rem 0.6rem;
    font-size: 0.8rem;
    border: 1px dashed var(--color-border);
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .new-api-button:hover {
    background: var(--color-hover);
  }

  .env-select {
    font-size: 0.75rem;
    max-width: 8rem;
    background: var(--color-bg);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: 3px;
  }

  .endpoints {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .endpoint {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    padding: 0.3rem 0.6rem;
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: 0.85rem;
    text-align: left;
    cursor: pointer;
  }

  .endpoint:hover {
    background: var(--color-hover);
  }

  .endpoint.selected {
    background: var(--color-selected);
  }

  .method {
    flex-shrink: 0;
    font-size: 0.65rem;
    font-weight: 700;
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
    background: var(--color-border);
    color: var(--color-text);
    min-width: 2.8rem;
    text-align: center;
  }

  .method-get {
    background: var(--color-method-get);
  }
  .method-post {
    background: var(--color-method-post);
  }
  .method-put {
    background: var(--color-method-put);
  }
  .method-patch {
    background: var(--color-method-patch);
  }
  .method-delete {
    background: var(--color-method-delete);
  }

  .endpoint-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .chain-marker {
    flex-shrink: 0;
    font-size: 0.65rem;
    color: var(--color-text-muted);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0.05rem 0.25rem;
  }

  .empty-state {
    margin: 0;
    padding: 1rem 0.75rem;
    font-size: 0.82rem;
    line-height: 1.5;
    color: var(--color-text-muted);
  }

  .empty-state code {
    font-size: 0.78rem;
    word-break: break-all;
  }

  .errors {
    flex-shrink: 0;
    max-height: 30%;
    overflow-y: auto;
    border-top: 1px solid var(--color-border);
    background: var(--color-error-bg);
    color: var(--color-error-text);
    font-size: 0.75rem;
  }

  .error-row {
    padding: 0.3rem 0.6rem;
    border-bottom: 1px solid var(--color-border);
  }
</style>
