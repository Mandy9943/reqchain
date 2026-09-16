<script lang="ts">
  import { untrack } from "svelte";
  import {
    recordToRows,
    recordsEqual,
    rowsToRecord,
    withTrailingBlank,
    type Row,
  } from "./keyValueRows";

  let {
    rows: rowsProp,
    onChange,
    keyLabel = "Key",
    valueLabel = "Value",
  }: {
    rows: Record<string, string>;
    onChange: (r: Record<string, string>) => void;
    keyLabel?: string;
    valueLabel?: string;
  } = $props();

  // The component's own ordered rows — a `Record<string,string>` has no
  // stable notion of row order or "this row is mid-rename", so this is the
  // one place that owns that shape. `lastEmitted` is the record this
  // component itself last produced via `onChange`; the sync effect below
  // uses it to tell "the parent's document changed under us" (a real
  // external change: a different endpoint, a hot reload, an edit made in
  // the JSON tab) apart from "the parent just echoed our own edit back"
  // (which must NOT reset `local`, or every keystroke would clobber
  // whatever the user is mid-typing in an unrelated field of the same row).
  let local = $state<Row[]>(
    untrack(() => withTrailingBlank(recordToRows(rowsProp))),
  );
  let lastEmitted: Record<string, string> = untrack(() => ({ ...rowsProp }));
  // Keys currently colliding, if any. While non-empty, the last edit was
  // refused: `local` still holds every row's text exactly as typed, but
  // nothing has been written back to the document.
  let duplicateKeys = $state<Set<string>>(new Set());

  $effect(() => {
    const external = rowsProp;
    if (!recordsEqual(external, lastEmitted)) {
      local = withTrailingBlank(recordToRows(external));
      lastEmitted = { ...external };
      duplicateKeys = new Set();
    }
  });

  function commit(next: Row[]): void {
    local = withTrailingBlank(next);
    const result = rowsToRecord(local);
    if (result.ok) {
      duplicateKeys = new Set();
      lastEmitted = result.record;
      onChange(result.record);
    } else {
      // Refuse visibly: keep every row exactly as typed, do not call
      // onChange, and flag the offending keys. Never silently collapse a
      // duplicate — that would delete the user's other row without saying
      // so.
      duplicateKeys = new Set(result.duplicateKeys);
    }
  }

  function setKey(index: number, key: string): void {
    commit(local.map((r, i) => (i === index ? { ...r, key } : r)));
  }

  function setValue(index: number, value: string): void {
    commit(local.map((r, i) => (i === index ? { ...r, value } : r)));
  }

  function removeRow(index: number): void {
    commit(local.filter((_, i) => i !== index));
  }
</script>

<div class="key-value-rows">
  {#if duplicateKeys.size > 0}
    <p class="duplicate-error" role="alert">
      Duplicate {duplicateKeys.size === 1 ? "key" : "keys"}: {[...duplicateKeys].join(", ")}
      — fix the duplicate to keep this change.
    </p>
  {/if}
  {#each local as row, index (index)}
    <div class="row" class:row-duplicate={row.key !== "" && duplicateKeys.has(row.key)}>
      <input
        class="row-key"
        type="text"
        placeholder={keyLabel}
        aria-label={`${keyLabel} ${index + 1}`}
        value={row.key}
        oninput={(e) => setKey(index, (e.currentTarget as HTMLInputElement).value)}
      />
      <input
        class="row-value"
        type="text"
        placeholder={valueLabel}
        aria-label={`${valueLabel} ${index + 1}`}
        value={row.value}
        oninput={(e) => setValue(index, (e.currentTarget as HTMLInputElement).value)}
      />
      <button
        type="button"
        class="row-remove"
        aria-label="Remove row {index + 1}"
        disabled={row.key === "" && row.value === "" && index === local.length - 1}
        onclick={() => removeRow(index)}
      >
        ×
      </button>
    </div>
  {/each}
</div>

<style>
  .key-value-rows {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .row-duplicate .row-key {
    border-color: var(--color-error-text);
  }

  .row-key,
  .row-value {
    flex: 1;
    min-width: 0;
    padding: 0.25rem 0.4rem;
    font-size: 0.8rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }

  .row-remove {
    flex-shrink: 0;
    width: 1.6rem;
    height: 1.6rem;
    line-height: 1;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-surface);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .row-remove:disabled {
    visibility: hidden;
  }

  .duplicate-error {
    margin: 0;
    font-size: 0.75rem;
    color: var(--color-error-text);
  }
</style>
