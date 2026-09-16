<script lang="ts">
  import { untrack } from "svelte";
  import Editor from "../Editor.svelte";
  import type { Body } from "../model";
  import KeyValueRows from "./KeyValueRows.svelte";
  import { asRecord } from "./keyValueRows";
  import {
    bodyKey,
    bodyStringField,
    bodyTypeOf,
    defaultBodyForType,
    isBodyEmpty,
    jsonContentText,
    parseJsonContent,
    type BodyType,
  } from "./bodyEditor";

  let { body, onChange }: {
    body: Body | undefined;
    onChange: (b: Body | undefined) => void;
  } = $props();

  const TYPES: { value: BodyType; label: string }[] = [
    { value: "none", label: "None" },
    { value: "json", label: "JSON" },
    { value: "form", label: "Form (urlencoded)" },
    { value: "multipart", label: "Multipart" },
    { value: "text", label: "Text" },
    { value: "xml", label: "XML" },
    { value: "binary", label: "Binary" },
  ];

  const currentType = $derived(bodyTypeOf(body));

  // A type the user picked but hasn't confirmed yet — set only while the
  // current body is non-empty and the pick differs from `currentType`. The
  // actual body (and thus the editor shown below) never changes until the
  // switch is confirmed, so nothing is ever silently discarded.
  let pendingType = $state<BodyType | null>(null);

  // The last committed JSON parse error for the `json` sub-editor, kept
  // separate from `body` itself: while the user is typing invalid JSON,
  // `body`/`body.content` deliberately isn't touched (see `onJsonInput`
  // below), so this is the only place that error is visible.
  let jsonParseError = $state<string | null>(null);

  // The `json` sub-editor's OWN text, separate from `body.content`. This is
  // the fix for a real bug: `body.content` is a parsed JS value, and
  // `jsonContentText(body.content)` re-stringifies it canonically — that
  // re-stringified text is almost never byte-identical to what the user
  // actually typed (different indentation, key order preserved but
  // whitespace normalized, ...). If `Editor`'s `value` prop were fed
  // straight from `jsonContentText(body.content)`, then every keystroke
  // that keeps the JSON valid would: commit -> `body.content` changes ->
  // `jsonContentText` produces DIFFERENT text than what's in the widget ->
  // `Editor.svelte`'s echo-vs-external effect sees `external !== lastKnown`
  // -> replaces the whole document and resets the caret to position 0. That
  // is not a rare edge case — it is every valid keystroke while editing an
  // existing json body.
  //
  // The fix: `jsonText` IS what's fed to `Editor`'s `value` prop, and it is
  // updated ONLY in two places — (1) on every `onJsonInput` call, from the
  // exact text `Editor` reports (so our own echo is a no-op: `Editor`
  // already holds that exact text as its own `lastKnown`), and (2) inside
  // the `lastBodyKey`-guarded effect below, which only fires when `body`
  // changed from OUTSIDE this component (a different endpoint, a hot
  // reload, an edit via the JSON tab) — exactly the cases where re-deriving
  // fresh text from `body.content` and resetting the caret is correct.
  let jsonText = $state<string>(
    untrack(() => jsonContentText(body?.type === "json" ? body.content : undefined)),
  );

  // `lastBodyKey` distinguishes "the document changed under us" (a
  // different endpoint selected, or an edit made in the JSON tab) from "this
  // is just the echo of the change this component itself just committed" —
  // same pattern `Editor.svelte`/`KeyValueRows.svelte` use for their own
  // state. `EndpointForm`/`RequestPanel` reuse this same component instance
  // across an endpoint switch (no `{#key}`), so without this, leftover
  // `pendingType`/`jsonParseError`/`jsonText` from a previous endpoint could
  // linger.
  let lastBodyKey: string = untrack(() => bodyKey(body));

  $effect(() => {
    const key = bodyKey(body);
    if (key !== lastBodyKey) {
      lastBodyKey = key;
      pendingType = null;
      jsonParseError = null;
      // A genuine external change — re-derive display text from the new
      // content. Harmless (and necessary) even for a non-json body: it
      // keeps `jsonText` in sync so switching back to a json body later
      // doesn't show stale text from a previous endpoint.
      jsonText = jsonContentText(body?.type === "json" ? body.content : undefined);
    }
  });

  function commit(next: Body | undefined): void {
    lastBodyKey = bodyKey(next);
    onChange(next);
  }

  function onTypeSelect(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as BodyType;
    if (value === currentType) {
      pendingType = null;
      return;
    }
    if (isBodyEmpty(body)) {
      pendingType = null;
      applyTypeSwitch(value);
    } else {
      pendingType = value;
    }
  }

  function confirmTypeChange(): void {
    if (pendingType === null) return;
    const target = pendingType;
    pendingType = null;
    applyTypeSwitch(target);
  }

  /** Applies a (confirmed, or no-confirmation-needed) switch to a fresh
   * empty body of `type`. Also resets `jsonText` when the target is
   * `"json"` — this is a deliberate, user-initiated content change (a
   * brand-new empty body), not an edit inside the json sub-editor, so there
   * is no "text the user typed" to preserve; without this, switching away
   * from and back to json could show stale text left over from a body this
   * endpoint had before. */
  function applyTypeSwitch(type: BodyType): void {
    const next = defaultBodyForType(type);
    if (next?.type === "json") {
      jsonText = jsonContentText(next.content);
    }
    commit(next);
  }

  function cancelTypeChange(): void {
    pendingType = null;
  }

  function onJsonInput(text: string): void {
    // The user's own text is the source of truth for what the widget shows
    // — recorded unconditionally, whether or not it currently parses. This
    // is what makes our own commit below an echo no-op: `Editor` already
    // holds exactly `text` as its `lastKnown`, and we're about to feed it
    // back the SAME string via `jsonText`, not a re-stringified one, so its
    // external-change effect sees no difference and never resets the caret.
    jsonText = text;
    const result = parseJsonContent(text);
    if (result.ok) {
      jsonParseError = null;
      commit({ type: "json", content: result.content });
    } else {
      // Refuse visibly: keep the buffer's committed content untouched (the
      // CodeMirror instance already holds exactly what the user typed —
      // see Editor.svelte's echo-vs-external effect) and surface the error
      // instead of losing or corrupting the last-valid content.
      jsonParseError = result.error;
    }
  }

  function onTextContent(text: string): void {
    if (body?.type !== "text" && body?.type !== "xml") return;
    commit({ type: body.type, content: text });
  }

  function onBinaryPathInput(e: Event): void {
    const value = (e.currentTarget as HTMLInputElement).value;
    commit({ type: "binary", path: value });
  }

  function onFormFieldsChange(fields: Record<string, string>): void {
    commit({ type: "form", fields });
  }

  function onMultipartFieldsChange(fields: Record<string, string>): void {
    if (body?.type !== "multipart") return;
    commit({ type: "multipart", fields, files: asRecord(body.files) });
  }

  function onMultipartFilesChange(files: Record<string, string>): void {
    if (body?.type !== "multipart") return;
    commit({ type: "multipart", fields: asRecord(body.fields), files });
  }
</script>

<div class="body-editor">
  <div class="type-row">
    <label for="body-type">Type</label>
    <select
      id="body-type"
      class="type-select"
      value={pendingType ?? currentType}
      onchange={onTypeSelect}
    >
      {#each TYPES as t (t.value)}
        <option value={t.value}>{t.label}</option>
      {/each}
    </select>
  </div>

  {#if pendingType !== null}
    <div class="confirm-switch" role="alert">
      <p>
        Switching the body type to
        {TYPES.find((t) => t.value === pendingType)?.label} will discard the
        current {TYPES.find((t) => t.value === currentType)?.label} body.
      </p>
      <div class="confirm-actions">
        <button type="button" class="confirm-yes" onclick={confirmTypeChange}>
          Switch and discard
        </button>
        <button type="button" class="confirm-no" onclick={cancelTypeChange}>
          Cancel
        </button>
      </div>
    </div>
  {/if}

  {#if currentType === "none"}
    <p class="hint">No request body.</p>
  {:else if currentType === "json"}
    <div class="sub-editor">
      <Editor value={jsonText} onChange={onJsonInput} />
    </div>
    {#if jsonParseError}
      <p class="field-error" role="alert">Invalid JSON: {jsonParseError}</p>
    {/if}
  {:else if currentType === "form"}
    <KeyValueRows
      rows={body?.type === "form" ? body.fields : {}}
      onChange={onFormFieldsChange}
      keyLabel="Field"
      valueLabel="Value"
    />
  {:else if currentType === "multipart"}
    <section class="fields-section">
      <h4>Fields</h4>
      <KeyValueRows
        rows={body?.type === "multipart" ? body.fields : {}}
        onChange={onMultipartFieldsChange}
        keyLabel="Field"
        valueLabel="Value"
      />
    </section>
    <section class="fields-section">
      <h4>Files</h4>
      <KeyValueRows
        rows={body?.type === "multipart" ? body.files : {}}
        onChange={onMultipartFilesChange}
        keyLabel="File field"
        valueLabel="Path"
      />
    </section>
  {:else if currentType === "text" || currentType === "xml"}
    <div class="sub-editor">
      <Editor
        value={bodyStringField(
          body?.type === "text" || body?.type === "xml" ? body.content : "",
        )}
        onChange={onTextContent}
      />
    </div>
  {:else if currentType === "binary"}
    <div class="field-row">
      <label for="body-binary-path">Path</label>
      <input
        id="body-binary-path"
        class="path-input"
        type="text"
        value={bodyStringField(body?.type === "binary" ? body.path : "")}
        oninput={onBinaryPathInput}
      />
    </div>
  {/if}
</div>

<style>
  .body-editor {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .type-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .type-row label {
    flex-shrink: 0;
    width: 4rem;
    font-size: 0.8rem;
    color: var(--color-text-muted);
  }

  .type-select {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.4rem;
    font-size: 0.85rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  .hint {
    color: var(--color-text-muted);
    font-style: italic;
    font-size: 0.8rem;
    margin: 0;
  }

  .confirm-switch {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.5rem 0.6rem;
    font-size: 0.8rem;
    color: var(--color-error-text);
    background: var(--color-error-bg);
    border: 1px solid var(--color-error-text);
    border-radius: 4px;
  }

  .confirm-switch p {
    margin: 0;
  }

  .confirm-actions {
    display: flex;
    gap: 0.4rem;
  }

  .confirm-yes,
  .confirm-no {
    padding: 0.25rem 0.6rem;
    font-size: 0.75rem;
    border: 1px solid var(--color-error-text);
    border-radius: 4px;
    cursor: pointer;
  }

  .confirm-yes {
    background: var(--color-error-text);
    color: #fff;
  }

  .confirm-no {
    background: var(--color-surface);
    color: var(--color-text);
  }

  .sub-editor {
    height: 12rem;
    min-height: 0;
  }

  .field-error {
    margin: 0;
    font-size: 0.75rem;
    color: var(--color-error-text);
  }

  .fields-section h4 {
    margin: 0 0 0.3rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--color-text-muted);
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

  .path-input {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.4rem;
    font-size: 0.85rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }
</style>
