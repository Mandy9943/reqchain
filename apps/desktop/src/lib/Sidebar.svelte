<script lang="ts">
  import { select, ui } from "./state.svelte";
  import type { ApiDto, EndpointDto } from "./ipc";

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

  function isSelected(api: ApiDto, endpoint: EndpointDto): boolean {
    return (
      ui.selected?.apiId === api.id && ui.selected?.endpointId === endpoint.id
    );
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
        <header class="api-header">
          <span class="api-name">{api.name}</span>
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
        </header>

        <ul class="endpoints">
          {#each visibleEndpoints(api) as endpoint, endpointIndex (endpointIndex)}
            <li>
              <button
                type="button"
                class="endpoint"
                class:selected={isSelected(api, endpoint)}
                onclick={() => selectEndpoint(api, endpoint)}
              >
                <span class="method method-{endpoint.method.toLowerCase()}"
                  >{endpoint.method}</span
                >
                <span class="endpoint-name">{endpoint.name}</span>
                {#if endpoint.authKind === "chained"}
                  <span class="chain-marker" title="Chained auth">chain</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/each}

    {#if ui.workspace.apis.length === 0}
      <!-- A fresh install has no workspace files, and a bare empty panel
           gives no clue where they are meant to go. -->
      <p class="empty-state">
        No APIs yet. Drop a JSON file into
        <code>~/.config/reqchain/workspace/apis/</code> — the app picks it up
        as soon as it is saved. <code>SPEC.md</code> describes the format, and
        <code>reqchain validate &lt;file&gt;</code> checks one.
      </p>
    {/if}
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

  .api-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.35rem 0.6rem;
    font-weight: 600;
    font-size: 0.85rem;
    color: var(--color-text-muted);
  }

  .api-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
