<script lang="ts">
  import { currentDoc, updateDoc } from "../doc.svelte";
  import type { Endpoint, Method } from "../model";
  import KeyValueRows from "./KeyValueRows.svelte";

  let { endpointId }: { endpointId: string } = $props();

  const METHODS: Method[] = [
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
  ];

  // `currentDoc()` re-parses the selected API's buffer on every reactive
  // read, so this stays live across every keystroke made through this form
  // (and through the JSON tab, since both edit `ui.buffers[apiId]`). The
  // caller (RequestPanel) only mounts this component once the buffer is
  // known to parse — this fallback is for the narrow window right after
  // that check where the selection has moved on again before this
  // component's own effects settle, not a case this form tries to explain
  // to the user.
  const doc = $derived(currentDoc());
  const endpoint = $derived<Endpoint | undefined>(
    doc?.ok ? doc.api.endpoints.find((e) => e.id === endpointId) : undefined,
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
