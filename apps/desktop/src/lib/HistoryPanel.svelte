<script lang="ts">
  import { history as fetchHistory, type HistoryEntry } from "./ipc";
  import {
    selectedApiOrRemoved,
    selectedEndpointOrRemoved,
    ui,
  } from "./state.svelte";

  let entries = $state<HistoryEntry[]>([]);
  let loadError = $state<string | null>(null);
  let expandedIndex = $state<number | null>(null);
  let collapsed = $state(true);
  // Monotonic counter so a `history` response for an endpoint the user has
  // since navigated away from is dropped instead of being applied — same
  // sequence-counter pattern used across the other panels.
  let seq = 0;

  // Use the removed-aware selectors, not the live-only ones: history is a
  // plain id lookup against a persisted log, not something that needs the
  // endpoint to still be listed in the parsed workspace, so a hot-reload
  // removed selection should still show its history strip — consistent
  // with RequestPanel/ResponsePanel still showing the rest of that
  // endpoint's last-known state instead of going blank.
  const api = $derived(selectedApiOrRemoved());
  const endpoint = $derived(selectedEndpointOrRemoved());

  // Refetch whenever the selection changes, and after every run (a new run
  // produces a new `ui.response` object, so watching it re-triggers this
  // even though `api`/`endpoint` didn't change).
  $effect(() => {
    const a = api;
    const ep = endpoint;
    void ui.response;
    expandedIndex = null;
    if (!a || !ep) {
      entries = [];
      loadError = null;
      return;
    }
    const apiId = a.id;
    const endpointId = ep.id;
    const mySeq = ++seq;
    fetchHistory(apiId, endpointId)
      .then((result) => {
        if (mySeq === seq) {
          entries = result;
          loadError = null;
        }
      })
      .catch((e) => {
        if (mySeq === seq) {
          loadError = e instanceof Error ? e.message : String(e);
        }
      });
  });

  function formatTime(at: string): string {
    const d = new Date(at);
    return Number.isNaN(d.getTime()) ? at : d.toLocaleString();
  }

  function formatSize(bytes: number): string {
    return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(2)} KB`;
  }

  function statusClass(status: number): string {
    const leading = Math.floor(status / 100);
    if (leading === 2) return "ok";
    if (leading === 3) return "redirect";
    return "err";
  }

  function toggle(i: number): void {
    expandedIndex = expandedIndex === i ? null : i;
  }
</script>

<div class="history-panel">
  <button
    type="button"
    class="history-toggle"
    onclick={() => (collapsed = !collapsed)}
    aria-expanded={!collapsed}
  >
    {collapsed ? "▸" : "▾"} History{#if entries.length}
      ({entries.length}){/if}
  </button>

  {#if !collapsed}
    {#if loadError}
      <div class="history-error">{loadError}</div>
    {:else if !api || !endpoint}
      <p class="placeholder">Select an endpoint to see its run history.</p>
    {:else if entries.length === 0}
      <p class="placeholder">No runs recorded yet.</p>
    {:else}
      <ul class="history-list">
        {#each entries as entry, i (i)}
          <li>
            <button
              type="button"
              class="history-row"
              onclick={() => toggle(i)}
              aria-expanded={expandedIndex === i}
            >
              <span class="history-time">{formatTime(entry.at)}</span>
              <span class="status status-{statusClass(entry.status)}"
                >{entry.status}</span
              >
              <span class="history-meta">{entry.elapsedMs} ms</span>
              <span class="history-meta">{formatSize(entry.sizeBytes)}</span>
              <span class="history-method">{entry.method}</span>
              <span class="history-url">{entry.url}</span>
            </button>

            {#if expandedIndex === i}
              <div class="history-detail">
                {#if entry.requestBody === undefined && entry.responseBody === undefined}
                  <p class="placeholder">bodies not stored</p>
                {:else}
                  <h4>Request body</h4>
                  {#if entry.requestBody === undefined}
                    <p class="placeholder">request body not stored</p>
                  {:else}
                    <pre class="body-text">{entry.requestBody || "(empty)"}</pre>
                  {/if}
                  <h4>Response body</h4>
                  {#if entry.responseBody === undefined}
                    <p class="placeholder">response body not stored</p>
                  {:else}
                    <pre class="body-text">{entry.responseBody || "(empty)"}</pre>
                  {/if}
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .history-panel {
    flex-shrink: 0;
    max-height: 40%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-top: 1px solid var(--color-border);
    padding-top: 0.4rem;
  }

  .placeholder {
    color: var(--color-text-muted);
    font-style: italic;
    font-size: 0.8rem;
  }

  .history-toggle {
    flex-shrink: 0;
    align-self: flex-start;
    padding: 0.15rem 0.3rem;
    font-size: 0.8rem;
    font-weight: 600;
    border: none;
    background: none;
    color: var(--color-text);
    cursor: pointer;
  }

  .history-error {
    font-size: 0.8rem;
    color: var(--color-error-text);
    background: var(--color-error-bg);
    border: 1px solid var(--color-error-text);
    border-radius: 4px;
    padding: 0.3rem 0.5rem;
  }

  .history-list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
  }

  .history-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.25rem 0.4rem;
    border: none;
    border-bottom: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-text);
    font-size: 0.75rem;
    text-align: left;
    cursor: pointer;
  }

  .history-row:hover {
    background: var(--color-hover);
  }

  .history-time {
    flex-shrink: 0;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .status {
    flex-shrink: 0;
    font-weight: 700;
    padding: 0.1rem 0.35rem;
    border-radius: 3px;
  }

  .status-ok {
    background: var(--color-method-get);
    color: #1a1d21;
  }

  .status-redirect {
    background: var(--color-method-put);
    color: #1a1d21;
  }

  .status-err {
    background: var(--color-error-bg);
    color: var(--color-error-text);
  }

  .history-meta {
    flex-shrink: 0;
    color: var(--color-text-muted);
  }

  .history-method {
    flex-shrink: 0;
    font-weight: 600;
  }

  .history-url {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }

  .history-detail {
    padding: 0.4rem 0.6rem 0.6rem;
    border-bottom: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .history-detail h4 {
    margin: 0;
    font-size: 0.7rem;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .body-text {
    margin: 0;
    padding: 0.4rem;
    white-space: pre-wrap;
    word-break: break-word;
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.75rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-surface);
  }
</style>
