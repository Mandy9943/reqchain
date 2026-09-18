<script lang="ts">
  import { deleteApi, saveApi, type ApiDto, type EndpointDto } from "./ipc";
  import { emptyApi, emptyEndpoint, serializeApi, slugify } from "./model";
  import { currentDoc, updateDoc } from "./doc.svelte";
  import { canWriteApiDoc, reload, select, ui } from "./state.svelte";
  import Icon from "./ui/Icon.svelte";
  import MethodChip from "./ui/MethodChip.svelte";

  // Which API groups the user has folded shut. Keyed by api id and stored
  // here rather than in `ui`, because it is pure view state: it must not
  // survive a workspace reload into a file that no longer exists, and
  // nothing outside this list cares about it.
  let collapsedApis = $state<Record<string, boolean>>({});

  /** A searching user wants hits, not folds: a live query opens every group. */
  function isCollapsed(api: ApiDto): boolean {
    if (ui.search.trim()) return false;
    return !!collapsedApis[api.id];
  }

  function toggleCollapsed(api: ApiDto): void {
    collapsedApis[api.id] = !collapsedApis[api.id];
  }

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
    // The collision check above reads the workspace as it is right now, but a
    // save or a delete already in flight for this id has not landed there yet:
    // a slug that collides with an API being deleted at this instant would
    // pass the check, and the delete would then remove the file we just wrote.
    // Every other write path consults this; so does this one.
    if (!canWriteApiDoc(id)) {
      newApiError = `"${id}" is busy — a save or delete for it is still in flight.`;
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
    if (!canWriteApiDoc(api.id)) return;
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
    if (!canWriteApiDoc(api.id)) {
      newEndpointError =
        "A save or delete is in progress for this API — wait for it to finish.";
      return;
    }
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
    ui.savingIds[api.id] = true;
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
      delete ui.savingIds[api.id];
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
      // Same reason as the three above: ids are slugified from the name, so
      // recreating an API called the same thing lands on the same key, and a
      // leftover `true` here would open the new API folded shut for no
      // reason the user can see.
      delete collapsedApis[api.id];
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
    if (!canWriteApiDoc(api.id)) return;
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
    if (!canWriteApiDoc(api.id)) {
      deleteEndpointError =
        "A save or delete is in progress for this API — wait for it to finish.";
      return;
    }
    deleteEndpointBusy = true;
    deleteEndpointError = null;
    ui.savingIds[api.id] = true;
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
      delete ui.savingIds[api.id];
    }
  }
</script>


<aside class="sidebar">
  <div class="search-row">
    <span class="search-icon"><Icon name="search" size={13} /></span>
    <input
      class="search"
      type="search"
      placeholder="Search endpoints"
      aria-label="Search endpoints"
      bind:value={ui.search}
    />
    <kbd class="keycap">Ctrl K</kbd>
  </div>

  <div class="apis">
    <div class="section-head">
      <span class="section-label">Workspace</span>
      <button
        type="button"
        class="icon-button"
        onclick={startCreateApi}
        aria-label="New API"
        title="New API"
      >
        <Icon name="plus" size={12} />
      </button>
    </div>

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
    {/if}

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
        <div class="api-header-row" class:selected={isApiSelected(api)}>
          <button
            type="button"
            class="disclosure"
            onclick={() => toggleCollapsed(api)}
            aria-expanded={!isCollapsed(api)}
            aria-label={`${isCollapsed(api) ? "Expand" : "Collapse"} ${api.name}`}
          >
            <Icon name={isCollapsed(api) ? "chevron-right" : "chevron-down"} size={12} />
          </button>

          <button
            type="button"
            class="api-header"
            onclick={() => selectApi(api)}
            title={api.path}
          >
            <span class="api-name">{api.name}</span>
          </button>

          {#if isDirty(api)}
            <span class="dirty-dot" title="Unsaved changes"></span>
          {/if}

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

          <span class="row-actions">
            <button
              type="button"
              class="icon-button"
              disabled={!canWriteApiDoc(api.id)}
              title={canWriteApiDoc(api.id)
                ? "New endpoint"
                : "A save or delete is in progress for this API"}
              aria-label={`New endpoint in ${api.name}`}
              onclick={() => startCreateEndpoint(api)}
            >
              <Icon name="plus" size={12} />
            </button>
            <button
              type="button"
              class="icon-button icon-button-danger"
              disabled={!canDeleteApi(api)}
              title={canDeleteApi(api)
                ? "Delete this API's file"
                : "A save is in progress for this API"}
              aria-label={`Delete ${api.name}`}
              onclick={() => startDeleteApi(api)}
            >
              <Icon name="trash" size={12} />
            </button>
          </span>

          <span class="count">{api.endpoints.length}</span>
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
          <div class="new-row new-row-nested">
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

        {#if !isCollapsed(api)}
          <ul class="endpoints">
            {#each visibleEndpoints(api) as endpoint, endpointIndex (endpointIndex)}
              <li>
                <div
                  class="endpoint-row"
                  class:selected={isEndpointSelected(api, endpoint)}
                >
                  <button
                    type="button"
                    class="endpoint"
                    onclick={() => selectEndpoint(api, endpoint)}
                    title={endpoint.path}
                  >
                    <MethodChip method={endpoint.method} />
                    <span class="endpoint-name">{endpoint.name}</span>
                    <span class="endpoint-path">{endpoint.path}</span>
                  </button>

                  {#if endpoint.authKind === "chained"}
                    <span class="chain-marker" title="Chained auth">
                      <Icon name="chain" size={11} />
                    </span>
                  {/if}

                  <span class="row-actions">
                    <button
                      type="button"
                      class="icon-button icon-button-danger"
                      disabled={!canWriteApiDoc(api.id)}
                      title={canWriteApiDoc(api.id)
                        ? "Remove this endpoint"
                        : "A save or delete is in progress for this API"}
                      onclick={() => startDeleteEndpoint(api, endpoint)}
                      aria-label={`Delete ${endpoint.name}`}
                    >
                      <Icon name="close" size={12} />
                    </button>
                  </span>
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

            {#if visibleEndpoints(api).length === 0}
              <li class="no-match">
                {ui.search.trim()
                  ? "No endpoint here matches the search."
                  : "No endpoints yet."}
              </li>
            {/if}
          </ul>
        {/if}
      </section>
    {/each}

    {#if ui.workspace.apis.length === 0 && !creatingApi}
      <!-- A fresh install has no workspace files, and a bare empty panel
           gives no clue where they are meant to go. -->
      <div class="empty-state">
        <span class="empty-glyph"><Icon name="chain" size={22} /></span>
        <p>
          No APIs yet. Create one with <strong>+</strong> above, or drop a JSON
          file into <code>~/.config/reqchain/workspace/apis/</code> — the app
          picks it up as soon as it is saved.
        </p>
        <p class="empty-hint">
          <code>SPEC.md</code> describes the format, and
          <code>reqchain validate &lt;file&gt;</code> checks one.
        </p>
      </div>
    {/if}
  </div>

  {#if ui.workspace.errors.length > 0}
    <div class="errors">
      {#each ui.workspace.errors as err, errIndex (errIndex)}
        <div class="error-row">
          <Icon name="warning" size={12} />
          <span class="error-file">{err.path.split("/").pop()}</span>
          <span class="error-message">{err.message}</span>
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
    min-height: 0;
    overflow: hidden;
    border-right: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  /* --- Search ----------------------------------------------------------- */

  .search-row {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-4);
    padding: 0 var(--space-3);
    height: var(--control-height);
    background: var(--color-inset);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .search-row:focus-within {
    border-color: var(--color-accent-line);
  }

  .search-icon {
    display: flex;
    color: var(--color-text-faint);
  }

  .search {
    flex-grow: 1;
    min-width: 0;
    border: none;
    background: transparent;
    font-size: var(--text-base);
  }

  .search:focus {
    outline: none;
  }

  .search::-webkit-search-cancel-button {
    filter: grayscale(1);
    opacity: 0.6;
  }

  .keycap {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 1px 4px;
  }

  /* --- The list --------------------------------------------------------- */

  .apis {
    flex-grow: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-1) var(--space-2) var(--space-2);
  }

  .section-label {
    font-family: var(--font-condensed);
    font-weight: 600;
    font-size: 0.625rem;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--color-text-faint);
  }

  .api {
    display: flex;
    flex-direction: column;
  }

  .api-header-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding-right: var(--space-2);
    border-radius: var(--radius-md);
  }

  .api-header-row:hover {
    background: var(--color-hover);
  }

  .api-header-row.selected {
    background: var(--color-accent-tint);
  }

  .disclosure {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    flex-shrink: 0;
    border: none;
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .api-header {
    flex-grow: 1;
    min-width: 0;
    text-align: left;
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
  }

  .api-name {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
  }

  .dirty-dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-accent);
  }

  .env-select {
    flex-shrink: 0;
    max-width: 6.5rem;
    height: 1.375rem;
    padding: 0 var(--space-1);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    background-color: var(--color-panel);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .count {
    flex-shrink: 0;
    min-width: 1rem;
    text-align: right;
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
    font-variant-numeric: tabular-nums;
  }

  /* Row actions are progressive disclosure: they appear on hover or when
     something inside them has keyboard focus, so a 40-endpoint workspace
     isn't a wall of always-on buttons. */
  .row-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0;
  }

  .api-header-row:hover .row-actions,
  .api-header-row:focus-within .row-actions,
  .endpoint-row:hover .row-actions,
  .endpoint-row:focus-within .row-actions {
    opacity: 1;
  }

  .icon-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .icon-button:hover:not(:disabled) {
    background: var(--color-border);
    color: var(--color-text-strong);
  }

  .icon-button:disabled {
    color: var(--color-text-faint);
    opacity: 0.5;
    cursor: default;
  }

  .icon-button-danger:hover:not(:disabled) {
    background: var(--color-err-tint);
    color: var(--color-err);
  }

  /* --- Endpoints -------------------------------------------------------- */

  .endpoints {
    list-style: none;
    margin: 0;
    padding: 0 0 var(--space-2) var(--space-4);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .endpoint-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding-right: var(--space-2);
    border-radius: var(--radius-md);
    border-left: 2px solid transparent;
  }

  .endpoint-row:hover {
    background: var(--color-hover);
  }

  .endpoint-row.selected {
    background: var(--color-accent-tint);
    border-left-color: var(--color-accent);
  }

  .endpoint {
    flex-grow: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 0 0 var(--space-2);
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
  }

  .endpoint-name {
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-base);
    color: var(--color-text-soft);
  }

  .selected .endpoint-name {
    color: var(--color-text-strong);
    font-weight: 500;
  }

  /* The path is context for the name, not a second label: it gives up its
     space first and disappears rather than pushing the name out. */
  .endpoint-path {
    flex-shrink: 1000000;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
  }

  .chain-marker {
    flex-shrink: 0;
    display: flex;
    color: var(--color-text-muted);
  }

  .no-match {
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-xs);
    color: var(--color-text-faint);
  }

  /* --- Inline create rows ----------------------------------------------- */

  .new-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) 0;
  }

  .new-row-nested {
    padding-left: var(--space-5);
  }

  .new-input {
    flex-grow: 1;
    min-width: 0;
    height: var(--control-height);
    padding: 0 var(--space-3);
    font-size: var(--text-base);
    background-color: var(--color-field);
    border: 1px solid var(--color-accent-line);
    border-radius: var(--radius-md);
  }

  .new-input:focus {
    outline: none;
    border-color: var(--color-accent);
  }

  .confirm-button,
  .cancel-button,
  .danger-button {
    flex-shrink: 0;
    height: 1.75rem;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    background: var(--color-inset);
    color: var(--color-text);
    cursor: pointer;
  }

  .confirm-button {
    border-color: var(--color-accent);
    background: var(--color-accent);
    color: var(--color-accent-on);
    font-weight: 600;
  }

  .danger-button {
    border-color: var(--color-err);
    background: var(--color-err);
    color: var(--color-accent-on);
    font-weight: 600;
  }

  .confirm-button:disabled,
  .cancel-button:disabled,
  .danger-button:disabled {
    opacity: 0.55;
    cursor: default;
  }

  /* --- Confirmations, errors -------------------------------------------- */

  .confirm-row {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: var(--space-2) 0 var(--space-3);
    padding: var(--space-4);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .confirm-row-danger {
    border: 1px solid var(--color-err-line);
    background: var(--color-err-tint);
  }

  .confirm-text {
    margin: 0;
    line-height: 1.55;
    color: var(--color-text);
  }

  .confirm-text code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-err);
  }

  .confirm-buttons {
    display: flex;
    gap: var(--space-2);
  }

  .inline-error {
    margin: 0 0 var(--space-2);
    padding: 0 var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-err);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-6) var(--space-2);
    padding: var(--space-6) var(--space-4);
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-lg);
    text-align: center;
  }

  .empty-glyph {
    color: var(--color-border);
  }

  .empty-state p {
    margin: 0;
    font-size: var(--text-sm);
    line-height: 1.6;
    color: var(--color-text-muted);
  }

  .empty-hint {
    color: var(--color-text-faint);
  }

  .empty-state code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .errors {
    flex-shrink: 0;
    max-height: 30%;
    overflow-y: auto;
    border-top: 1px solid var(--color-border);
    background: var(--color-err-tint);
  }

  .error-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    font-size: var(--text-xs);
    line-height: 1.5;
    color: var(--color-err);
    border-bottom: 1px solid var(--color-err-line);
  }

  .error-row:last-child {
    border-bottom: none;
  }

  .error-file {
    font-family: var(--font-mono);
    flex-shrink: 0;
  }

  .error-message {
    color: var(--color-text-muted);
    min-width: 0;
  }
</style>
