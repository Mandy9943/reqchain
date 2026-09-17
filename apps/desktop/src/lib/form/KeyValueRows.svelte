<script lang="ts">
  import { untrack } from "svelte";
  import {
    recordToRows,
    recordsEqual,
    resolvedKey,
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
  // The last refused edit, if any. While set, `local` still holds every
  // row's text exactly as typed, but nothing has been written back to the
  // document — the fields have the same identity that made the commit fail
  // (see keyValueRows.ts's `RebuildResult`).
  let fieldError = $state<{ kind: "duplicate" | "integer-like"; keys: string[] } | null>(
    null,
  );

  $effect(() => {
    const external = rowsProp;
    if (!recordsEqual(external, lastEmitted)) {
      local = withTrailingBlank(recordToRows(external));
      lastEmitted = { ...external };
      fieldError = null;
    }
  });

  function commit(next: Row[]): void {
    local = withTrailingBlank(next);
    const result = rowsToRecord(local);
    if (result.ok) {
      fieldError = null;
      lastEmitted = result.record;
      // Every row's identity now tracks what was actually just committed —
      // this is what lets a LATER clear-to-rename fall back to the key the
      // row most recently held, not whatever it started as when this
      // component mounted. A row that never resolves to a real key (the
      // permanent trailing blank) stays `hadKey: null`.
      local = local.map((row) => ({ ...row, hadKey: resolvedKey(row) }));
      onChange(result.record);
    } else {
      // Refuse visibly: keep every row exactly as typed, do not call
      // onChange, and flag the offending keys. Never silently collapse a
      // duplicate or reorder the document out from under the user — that
      // would drop or corrupt a value without saying so.
      fieldError = { kind: result.kind, keys: result.keys };
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

  function rowHasError(row: Row): boolean {
    if (!fieldError) return false;
    const key = resolvedKey(row);
    return key !== null && fieldError.keys.includes(key);
  }
</script>

<div class="key-value-rows">
  {#if fieldError}
    <p class="field-error" role="alert">
      {#if fieldError.kind === "duplicate"}
        Duplicate {fieldError.keys.length === 1 ? "key" : "keys"}: {fieldError.keys.join(", ")}
        — fix the duplicate to keep this change.
      {:else}
        {fieldError.keys.length === 1 ? "This key" : "These keys"} would reorder
        every row ({fieldError.keys.join(", ")}) — a purely-numeric key sorts
        first in JSON regardless of where it's typed. Use a non-numeric name.
      {/if}
    </p>
  {/if}
  {#each local as row, index (index)}
    <div class="row" class:row-error={rowHasError(row)}>
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
        disabled={resolvedKey(row) === null && row.value === "" && index === local.length - 1}
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

  .row-error .row-key {
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

  .field-error {
    margin: 0;
    font-size: 0.75rem;
    color: var(--color-error-text);
  }
</style>
