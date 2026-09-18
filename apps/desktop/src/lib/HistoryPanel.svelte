<script lang="ts">
  import { history as fetchHistory, type HistoryEntry } from "./ipc";
  import { formatSize, statusClass } from "./format";
  import {
    selectedApiOrRemoved,
    selectedEndpointOrRemoved,
    ui,
  } from "./state.svelte";
  import Icon from "./ui/Icon.svelte";
  import MethodChip from "./ui/MethodChip.svelte";

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

  function toggle(i: number): void {
    expandedIndex = expandedIndex === i ? null : i;
  }

  /**
   * The latency sparkline over this endpoint's recent runs, oldest on the
   * left. It answers the question the row list makes you read for — "is
   * this endpoint getting slower, and did anything fail lately?" — in one
   * glance, so the drawer stays worth collapsing.
   */
  const SPARK_MAX = 24;

  const spark = $derived.by(() => {
    // `entries` comes back newest-first; a time series reads left to right.
    const recent = entries.slice(0, SPARK_MAX).reverse();
    if (recent.length === 0) return [];
    const slowest = Math.max(...recent.map((e) => e.elapsedMs), 1);
    return recent.map((e) => ({
      // A floor of 2px: a 3 ms run next to a 4 s one must still be a mark,
      // not an invisible zero-height bar.
      height: Math.max(2, Math.round((e.elapsedMs / slowest) * 16)),
      kind: statusClass(e.status),
      label: `${e.status} · ${e.elapsedMs} ms`,
    }));
  });
</script>

<div class="history-panel" class:history-panel-open={!collapsed}>
  <div class="history-head">
    <button
      type="button"
      class="history-toggle"
      onclick={() => (collapsed = !collapsed)}
      aria-expanded={!collapsed}
    >
      <Icon name={collapsed ? "chevron-right" : "chevron-down"} size={12} />
      <span class="history-label">History</span>
      {#if entries.length}
        <span class="history-count">{entries.length} runs</span>
      {/if}
    </button>

    <span class="spacer"></span>

    {#if spark.length > 0}
      <span
        class="spark"
        title={`Last ${spark.length} runs, oldest first`}
        aria-hidden="true"
      >
        {#each spark as bar, i (i)}
          <span
            class="spark-bar spark-{bar.kind}"
            style="height: {bar.height}px"
            title={bar.label}
          ></span>
        {/each}
      </span>
    {/if}
  </div>

  {#if !collapsed}
    <div class="history-body">
      {#if loadError}
        <p class="notice notice-error">{loadError}</p>
      {:else if !api || !endpoint}
        <p class="notice">Select an endpoint to see its run history.</p>
      {:else if entries.length === 0}
        <p class="notice">No runs recorded yet.</p>
      {:else}
        <ul class="history-list">
          {#each entries as entry, i (i)}
            <li>
              <button
                type="button"
                class="history-row"
                class:history-row-open={expandedIndex === i}
                onclick={() => toggle(i)}
                aria-expanded={expandedIndex === i}
              >
                <span class="history-time">{formatTime(entry.at)}</span>
                <span class="history-status status-{statusClass(entry.status)}">
                  {entry.status}
                </span>
                <span class="history-meta">{entry.elapsedMs} ms</span>
                <span class="history-meta history-meta-quiet">
                  {formatSize(entry.sizeBytes)}
                </span>
                <MethodChip method={entry.method} />
                <span class="history-url">{entry.url}</span>
              </button>

              {#if expandedIndex === i}
                <div class="history-detail">
                  {#if entry.requestBody === undefined && entry.responseBody === undefined}
                    <p class="notice">
                      Bodies were not stored for this run.
                    </p>
                  {:else}
                    <span class="section-label">Request body</span>
                    {#if entry.requestBody === undefined}
                      <p class="notice">Request body not stored.</p>
                    {:else}
                      <pre class="body-text">{entry.requestBody || "(empty)"}</pre>
                    {/if}
                    <span class="section-label">Response body</span>
                    {#if entry.responseBody === undefined}
                      <p class="notice">Response body not stored.</p>
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
    </div>
  {/if}
</div>

<style>
  .history-panel {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .history-panel-open {
    max-height: 45%;
  }

  .spacer {
    flex-grow: 1;
  }

  .history-head {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-5);
  }

  .history-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) 0;
    border: none;
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .history-toggle:hover {
    color: var(--color-text-strong);
  }

  .history-label {
    font-family: var(--font-condensed);
    font-weight: 600;
    font-size: 0.625rem;
    letter-spacing: 0.13em;
    text-transform: uppercase;
  }

  .history-count {
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
    font-variant-numeric: tabular-nums;
  }

  /* --- Sparkline -------------------------------------------------------- */

  .spark {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 16px;
  }

  .spark-bar {
    width: 3px;
    border-radius: 1px;
    background: var(--color-text-faint);
  }

  .spark-ok {
    background: var(--color-ok);
    opacity: 0.55;
  }

  .spark-redirect {
    background: var(--color-warn);
    opacity: 0.7;
  }

  .spark-err {
    background: var(--color-err);
  }

  /* The newest run is the one you just fired: it reads at full strength. */
  .spark-bar:last-child {
    opacity: 1;
  }

  /* --- Rows ------------------------------------------------------------- */

  .history-body {
    flex-grow: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .history-list {
    list-style: none;
    margin: 0;
    padding: 0 0 var(--space-3);
  }

  .history-row {
    display: grid;
    grid-template-columns: 11rem 2.5rem 4rem 4rem 2.875rem minmax(0, 1fr);
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: var(--row-height);
    padding: 0 var(--space-5);
    border: none;
    background: none;
    text-align: left;
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .history-row:hover {
    background: var(--color-hover);
  }

  .history-row-open {
    background: var(--color-accent-tint);
  }

  .history-time {
    font-family: var(--font-mono);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .history-status {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .status-ok {
    color: var(--color-ok);
  }

  .status-redirect {
    color: var(--color-warn);
  }

  .status-err {
    color: var(--color-err);
  }

  .history-meta {
    font-family: var(--font-mono);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .history-meta-quiet {
    color: var(--color-text-faint);
  }

  .history-url {
    font-family: var(--font-mono);
    color: var(--color-text-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .history-detail {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-5) var(--space-4);
    background: var(--color-bg);
    border-top: 1px solid var(--color-border-soft);
    border-bottom: 1px solid var(--color-border-soft);
  }

  .section-label {
    font-family: var(--font-condensed);
    font-weight: 600;
    font-size: 0.625rem;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--color-text-faint);
  }

  .body-text {
    margin: 0;
    padding: var(--space-3) var(--space-4);
    max-height: 12rem;
    overflow: auto;
    background: var(--color-field);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.6;
    color: var(--color-text-soft);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .notice {
    margin: 0;
    padding: var(--space-3) var(--space-5);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .notice-error {
    color: var(--color-err);
  }
</style>
