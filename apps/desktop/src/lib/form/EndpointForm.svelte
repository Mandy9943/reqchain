<script lang="ts">
  import { updateDoc } from "../doc.svelte";
  import type { Api, Body, Endpoint, Method } from "../model";
  import BodyEditor from "./BodyEditor.svelte";
  import KeyValueRows from "./KeyValueRows.svelte";

  // `api` is the already-parsed document `RequestPanel` computed — passed
  // in rather than re-derived here via `currentDoc()`. `currentDoc()`
  // requires the selection to still be "live" (`selectedApi()`), which is
  // NOT true for a just-removed-but-still-displayed endpoint
  // (`ui.removedSelected`); `RequestPanel`'s own `parsedDoc` is built from
  // its removed-tolerant `api`/`bufferText` fallbacks instead, so it stays
  // renderable in that case (mirroring what the JSON tab's `Editor` already
  // does). Re-deriving via `currentDoc()` here would silently go
  // `undefined` in exactly that case and dead-end the form on "not
  // available" — Form is the default tab, so that dead end would be the
  // first and only thing such a user sees.
  let { api, endpointId }: { api: Api; endpointId: string } = $props();

  const METHODS: Method[] = [
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
  ];

  const endpoint = $derived<Endpoint | undefined>(
    api.endpoints.find((e) => e.id === endpointId),
  );

  /** Runs `fn` against the live endpoint inside the document `updateDoc`
   * gives us — never against the stale `endpoint` snapshot read above,
   * which could be one buffer-generation behind by the time a mutator
   * runs. */
  function mutateEndpoint(fn: (ep: Endpoint) => void): void {
    updateDoc((api) => {
      const ep = api.endpoints.find((e) => e.id === endpointId);
      if (!ep) return;
      fn(ep);
    });
  }

  function onMethodChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as Method;
    mutateEndpoint((ep) => {
      ep.method = value;
    });
  }

  function onPathChange(e: Event): void {
    const value = (e.currentTarget as HTMLInputElement).value;
    mutateEndpoint((ep) => {
      ep.path = value;
    });
  }

  function onHeadersChange(headers: Record<string, string>): void {
    mutateEndpoint((ep) => {
      ep.headers = headers;
    });
  }

  function onQueryChange(query: Record<string, string>): void {
    mutateEndpoint((ep) => {
      ep.query = query;
    });
  }

  function onBodyChange(body: Body | undefined): void {
    mutateEndpoint((ep) => {
      if (body === undefined) {
        delete ep.body;
      } else {
        ep.body = body;
      }
    });
  }
</script>

{#if !endpoint}
  <p class="placeholder">Endpoint not available in the current document.</p>
{:else}
  <div class="endpoint-form">
    <div class="field-row">
      <label for="endpoint-method">Method</label>
      <select
        id="endpoint-method"
        class="method-select"
        value={endpoint.method}
        onchange={onMethodChange}
      >
        {#each METHODS as m (m)}
          <option value={m}>{m}</option>
        {/each}
      </select>
    </div>

    <div class="field-row">
      <label for="endpoint-path">Path</label>
      <input
        id="endpoint-path"
        class="path-input"
        type="text"
        value={endpoint.path}
        oninput={onPathChange}
      />
    </div>

    <section class="fields-section">
      <h3>Headers</h3>
      <KeyValueRows
        rows={endpoint.headers}
        onChange={onHeadersChange}
        keyLabel="Header"
        valueLabel="Value"
      />
    </section>

    <section class="fields-section">
      <h3>Query</h3>
      <KeyValueRows
        rows={endpoint.query}
        onChange={onQueryChange}
        keyLabel="Param"
        valueLabel="Value"
      />
    </section>

    <section class="fields-section">
      <h3>Body</h3>
      <BodyEditor body={endpoint.body} onChange={onBodyChange} />
    </section>
  </div>
{/if}

<style>
  .placeholder {
    color: var(--color-text-muted);
    font-style: italic;
  }

  .endpoint-form {
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
    width: 4rem;
    font-size: 0.8rem;
    color: var(--color-text-muted);
  }

  .method-select,
  .path-input {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.4rem;
    font-size: 0.85rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .path-input {
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }

  .fields-section h3 {
    margin: 0 0 0.4rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--color-text-muted);
  }
</style>
