<script lang="ts">
  import { lint, previewEndpoint, saveApi, type DiagnosticDto } from "./ipc";
  import {
    canSendSelected,
    canWriteApiDoc,
    discardLocalChanges,
    selectedApi,
    selectedApiOrRemoved,
    selectedEndpoint,
    selectedEndpointOrRemoved,
    sendSelected,
    ui,
  } from "./state.svelte";
  import Icon from "./ui/Icon.svelte";
  import MethodChip from "./ui/MethodChip.svelte";
  import Editor from "./Editor.svelte";
  import ApiForm from "./form/ApiForm.svelte";
  import EndpointForm from "./form/EndpointForm.svelte";
  import { parseApi } from "./model";

  // Which of the two tabs is showing. Reset to "form" (the default) on every
  // selection change, below — a user switching endpoints always lands back
  // on the fields view, not wherever they last left the JSON tab for a
  // *different* endpoint.
  let activeTab = $state<"form" | "json">("form");

  let diagnostics = $state<DiagnosticDto[]>([]);
  let previewUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  // In-flight saves live in `ui.savingIds` (state.svelte.ts), not
  // component-local state: Sidebar's Delete action needs to see them too, to
  // avoid racing a delete against a save of the same file. Per-apiId (not a
  // single flag) so saving API A never blocks — or gets clobbered by — a
  // save of API B started while A is still pending; a second save of the
  // *same* apiId is refused instead.
  // apiId -> text that was last successfully written to disk by us. Used to
  // adopt the canonical (re-serialized) text once the post-save reload comes
  // back, but only if the user hasn't kept typing in the meantime.
  let cleanSnapshot: Record<string, string> = {};
  // Bumped after a save succeeds for whatever API is selected at that
  // moment, to re-trigger the preview effect below (its own dependencies
  // don't otherwise change on save).
  let previewGeneration = $state(0);
  // Monotonic counters so a stale `lint`/`preview_endpoint` response — one
  // that was in flight when the selection, environment or buffer moved on —
  // is dropped instead of overwriting state for whatever is on screen now.
  let lintSeq = 0;
  let previewSeq = 0;

  // `liveApi`/`liveEndpoint` are only defined when the current selection
  // still resolves in the workspace — everything that talks to the backend
  // (save, lint, preview) must gate on these, never on the display
  // fallbacks below. `api`/`endpoint` fall back to the last-known snapshot
  // of a removed selection (state.svelte.ts's `ui.removedSelected`) so the
  // panel keeps showing it instead of going blank.
  const liveApi = $derived(selectedApi());
  const liveEndpoint = $derived(selectedEndpoint());
  const api = $derived(selectedApiOrRemoved());
  const endpoint = $derived(selectedEndpointOrRemoved());
  const isRemoved = $derived(ui.removedSelected !== null);
  // The API's own header row is selected (`endpointId: null`) — an API
  // with zero endpoints is always in this state right after creation, since
  // there is nothing else to select. Task 6 fills this branch with the API
  // settings form; for now it's a clearly-marked placeholder so the state
  // is visibly reachable rather than silently falling through to "Select an
  // endpoint" (which would make a brand-new, endpoint-less API look broken).
  const isApiOnlySelected = $derived(
    api !== undefined && ui.selected !== null && ui.selected.endpointId === null,
  );
  const bufferText = $derived(api ? ui.buffers[api.id] : undefined);
  // Both tabs render from this buffer (never a second parse path) — see
  // constraints.md's "one buffer" rule. Computed off `api`/`bufferText`
  // (which already fall back to the removed-selection snapshot), not
  // `currentDoc()`, so the parse check keeps working even in the rare case
  // where the selected endpoint's API is no longer "live" (see the doc
  // comment in EndpointForm.svelte for why that distinction matters there).
  const parsedDoc = $derived(
    api ? parseApi(bufferText ?? api.text) : undefined,
  );
  const dirty = $derived(
    api !== undefined && bufferText !== undefined && bufferText !== api.text,
  );
  const diskChanged = $derived(
    liveApi !== undefined && !!ui.diskChanged[liveApi.id],
  );
  const saving = $derived(liveApi ? !!ui.savingIds[liveApi.id] : false);

  // Seed the buffer for a newly selected API, without ever clobbering a
  // buffer the user (or task 10's hot-reload logic) already owns. Only
  // meaningful for a live api — there's nothing to seed from once it's
  // gone, and the buffer (if any) already exists from before it vanished.
  $effect(() => {
    const current = liveApi;
    if (current && ui.buffers[current.id] === undefined) {
      ui.buffers[current.id] = current.text;
    }
  });

  // Once a save lands and the workspace reload brings back the canonical
  // (re-serialized) text, adopt it into the buffer — but only while the
  // buffer still holds exactly what we saved.
  $effect(() => {
    const current = liveApi;
    if (!current) return;
    const clean = cleanSnapshot[current.id];
    if (
      clean !== undefined &&
      ui.buffers[current.id] === clean &&
      current.text !== clean
    ) {
      ui.buffers[current.id] = current.text;
      cleanSnapshot[current.id] = current.text;
      // The post-save reload saw the file move under a buffer it still
      // considered dirty and raised `diskChanged`. It was OUR save that moved
      // it, and the buffer now holds exactly what is on disk, so leaving the
      // flag set shows "changed on disk / Discard mine" after essentially
      // every save — a false alarm that reads like "you are about to lose
      // work" against a buffer that is byte-identical to the file.
      ui.diskChanged[current.id] = false;
    }
  });

  // Selection changed: drop feedback that belonged to whatever was
  // previously on screen so it can't be shown against an unrelated
  // endpoint. `lint`/preview effects below repopulate diagnostics/URL for
  // the new selection on their own.
  $effect(() => {
    void ui.selected;
    saveError = null;
    diagnostics = [];
    activeTab = "form";
  });

  // Diagnostics strip: relint on every buffer change, debounced. Guarded by
  // `lintSeq` against a stale response (from a previous keystroke, or a
  // since-abandoned API/selection) landing after a newer one already did.
  $effect(() => {
    // Re-keyed on `ui.selected` explicitly (not just `bufferText`): fix
    // chosen for the "diagnostics vanish on endpoint switch" review finding
    // was to re-key rather than drop the unconditional clear in the
    // selection-change effect above. `bufferText` alone doesn't change when
    // switching between two endpoints of the *same* API — diagnostics lint
    // the whole file, not a single endpoint, so its derived value is
    // identical and this effect would never re-fire, leaving the strip
    // cleared until the next keystroke.
    void ui.selected;
    const text = bufferText;
    if (!liveApi || text === undefined) {
      return;
    }
    const handle = setTimeout(() => {
      const seq = ++lintSeq;
      void lint(text)
        .then((d) => {
          if (seq === lintSeq) {
            diagnostics = d;
          }
        })
        .catch(() => {
          // lint never rejects per the IPC surface; ignore defensively.
        });
    }, 300);
    return () => clearTimeout(handle);
  });

  // Effective URL preview. `preview_endpoint` performs no network I/O at all
  // (a chained auth renders as a placeholder — see `Executor::preview`) and
  // only ever reads the *persisted* file, so its result cannot change from
  // unsaved keystrokes; firing it on every valid-JSON edit would buy nothing.
  // So this only fires on: selection change, environment change for the
  // selected API, and a successful save (via `previewGeneration`). Debounced 300 ms to
  // coalesce rapid selection/environment changes; `previewSeq` drops a
  // response that's no longer for the current selection/environment.
  $effect(() => {
    const sel = ui.selected;
    if (!sel || !liveEndpoint) {
      // No selection, or the selected endpoint no longer resolves (removed)
      // — `preview_endpoint` needs a real endpoint id, so there's nothing
      // to resolve.
      previewUrl = null;
      previewError = null;
      return;
    }
    const apiId = sel.apiId;
    // `liveEndpoint` (checked above) guarantees this is a resolved endpoint
    // selection, not an API-only one — read the id off it rather than
    // `sel.endpointId`, which is `string | null`.
    const endpointId = liveEndpoint.id;
    const env = ui.env[apiId] ?? null;
    void previewGeneration;
    const handle = setTimeout(() => {
      const seq = ++previewSeq;
      previewEndpoint(apiId, endpointId, env)
        .then((effective) => {
          if (seq === previewSeq) {
            previewUrl = effective.url;
            previewError = null;
          }
        })
        .catch((e) => {
          if (seq === previewSeq) {
            previewUrl = null;
            previewError = e instanceof Error ? e.message : String(e);
          }
        });
    }, 300);
    return () => clearTimeout(handle);
  });

  function onEditorChange(text: string): void {
    if (api) {
      ui.buffers[api.id] = text;
    }
  }

  /** Whether a save can actually be triggered for `apiId` right now. */
  function canSave(apiId: string): boolean {
    const text = ui.buffers[apiId];
    const current = ui.workspace.apis.find((a) => a.id === apiId);
    if (!current || text === undefined || text === current.text) return false;
    return canWriteApiDoc(apiId);
  }

  async function handleSave(): Promise<void> {
    const current = liveApi;
    if (!current) return;
    // Capture everything off the reactive graph now — the selection (and
    // hence `liveApi`) can change while `saveApi` is in flight, and the
    // continuation below must keep acting on the API it was actually asked
    // to save, never on whatever happens to be selected when it resolves.
    const apiId = current.id;
    if (!canSave(apiId)) return;
    const text = ui.buffers[apiId]!;

    const stillSelected = () => selectedApi()?.id === apiId;
    ui.savingIds[apiId] = true;
    if (stillSelected()) saveError = null;

    try {
      const result = await saveApi(apiId, text);
      if (stillSelected()) {
        diagnostics = result;
      }
      const hasError = result.some((d) => d.severity === "error");
      if (!hasError) {
        cleanSnapshot[apiId] = text;
        // A save just resolved successfully, so there's no "changed on
        // disk while dirty" conflict from it — even if the buffer has
        // since moved on (below), that's a fresh local edit, not a
        // hot-reload conflict.
        ui.diskChanged[apiId] = false;
        // The buffer is deliberately NOT touched here. A keystroke that
        // landed while the save was in flight must never be reverted (the
        // lost-work class commit e57b89b closed), and if the buffer still
        // holds exactly `text` there is nothing to assign — the
        // cleanSnapshot-merge effect above adopts the canonical
        // (re-serialized) text once the post-save reload arrives, and only
        // while the buffer still equals exactly what was saved.
        if (ui.selected?.apiId === apiId) {
          previewGeneration++;
        }
      }
    } catch (e) {
      if (stillSelected()) {
        saveError = e instanceof Error ? e.message : String(e);
      }
    } finally {
      ui.savingIds[apiId] = false;
    }
  }

  /**
   * Imperative entry point for the Ctrl+S shortcut (App.svelte, via
   * `bind:this`). Returns whether a save was actually triggered, so the
   * caller only calls `preventDefault` when the shortcut did something.
   */
  export function saveCurrent(): boolean {
    const current = liveApi;
    if (!current || !canSave(current.id)) return false;
    void handleSave();
    return true;
  }

  /**
   * Sends the selected endpoint. The control lives here, on the request's
   * own URL bar, rather than above the response: the thing that fires a
   * request belongs next to the request it fires, and `sendSelected` already
   * owns the in-flight flag and run-sequence guard in state.svelte.ts.
   */
  function handleSend(): void {
    void sendSelected();
  }

  function handleDiscard(): void {
    if (liveApi) {
      discardLocalChanges(liveApi.id);
    }
  }
</script>

<div class="request-panel-inner">
  {#if !api || (!isApiOnlySelected && !endpoint)}
    <div class="placeholder-pane">
      <span class="placeholder-glyph"><Icon name="chain" size={24} /></span>
      <p>
        Pick an endpoint on the left, or press <kbd class="keycap">Ctrl K</kbd>
        to search the workspace.
      </p>
    </div>
  {:else}
    {#if isRemoved}
      <div class="banner banner-error">
        <Icon name="warning" size={13} />
        <span>
          This endpoint no longer exists in the workspace files — showing the
          last known content{#if !liveApi} (the API file itself is gone){/if}.
        </span>
      </div>
    {/if}

    {#if isApiOnlySelected}
      <header class="bar">
        <span class="api-settings-title">{api.name}</span>
        <span class="file-path">{api.path}</span>
      </header>
    {:else if endpoint}
      <header class="bar">
        <MethodChip method={endpoint.method} size="md" />

        <div class="url-box" class:url-box-problem={isRemoved || !!previewError}>
          {#if isRemoved}
            <span class="url-note">endpoint removed</span>
          {:else if previewUrl}
            <span class="url">{previewUrl}</span>
          {:else if previewError}
            <span class="url-note">{previewError}</span>
          {:else}
            <span class="url-note">resolving…</span>
          {/if}
        </div>

        <button
          type="button"
          class="send-button"
          disabled={!canSendSelected()}
          onclick={handleSend}
          title="Send this request (Ctrl+Enter)"
        >
          <Icon name="send" size={12} />
          {ui.running ? "Sending…" : "Send"}
        </button>
      </header>
    {/if}

    <div class="meta-row">
      {#if !isApiOnlySelected && endpoint}
        <span class="auth-chip" class:auth-chip-chained={endpoint.authKind === "chained"}>
          {#if endpoint.authKind === "chained"}
            <Icon name="chain" size={11} />
          {/if}
          auth: {endpoint.authKind}
        </span>
      {/if}

      <span class="spacer"></span>

      {#if diskChanged}
        <span class="badge badge-warn">
          <Icon name="warning" size={11} />
          changed on disk
        </span>
        <button type="button" class="ghost-button" onclick={handleDiscard}>
          Discard mine
        </button>
      {/if}

      {#if dirty}
        <span class="dirty-flag">
          <span class="dirty-dot"></span>
          Unsaved
        </span>
      {/if}

      <button
        type="button"
        class="save-button"
        disabled={!liveApi || !dirty || saving}
        onclick={handleSave}
      >
        {saving ? "Saving…" : "Save"}
        <kbd class="keycap keycap-quiet">Ctrl S</kbd>
      </button>
    </div>

    {#if saveError}
      <div class="banner banner-error">
        <Icon name="warning" size={13} />
        <span>{saveError}</span>
      </div>
    {/if}

    <div class="tab-strip" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "form"}
        class="tab"
        class:tab-active={activeTab === "form"}
        onclick={() => (activeTab = "form")}
      >
        Form
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "json"}
        class="tab"
        class:tab-active={activeTab === "json"}
        onclick={() => (activeTab = "json")}
      >
        JSON
      </button>
      <span class="spacer"></span>
      <span class="file-name">{api.path.split("/").pop()}</span>
    </div>

    <div class="tab-content">
      {#if activeTab === "form"}
        {#if !parsedDoc || !parsedDoc.ok}
          <div class="parse-error">
            <p>
              {parsedDoc
                ? parsedDoc.error
                : "Nothing to edit — the selection no longer resolves."}
            </p>
            {#if parsedDoc}
              <button
                type="button"
                class="ghost-button"
                onclick={() => (activeTab = "json")}
              >
                Switch to JSON
              </button>
            {/if}
          </div>
        {:else if isApiOnlySelected}
          {#key parsedDoc.api.id}
            <ApiForm api={parsedDoc.api} />
          {/key}
        {:else if endpoint}
          <EndpointForm endpointId={endpoint.id} api={parsedDoc.api} />
        {/if}
      {:else}
        <div class="editor-wrap">
          <Editor
            value={bufferText ?? api.text}
            onChange={onEditorChange}
            {diagnostics}
          />
        </div>
      {/if}
    </div>

    {#if diagnostics.length > 0}
      <div class="diagnostics">
        {#each diagnostics as d, i (i)}
          <div class="diagnostic diagnostic-{d.severity}">
            <span class="diagnostic-dot"></span>
            <span class="diagnostic-path">{d.path}</span>
            <span class="diagnostic-message">{d.message}</span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .request-panel-inner {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--color-bg);
  }

  .spacer {
    flex-grow: 1;
  }

  /* --- Empty state ------------------------------------------------------ */

  .placeholder-pane {
    flex-grow: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-4);
    padding: var(--space-6);
    text-align: center;
  }

  .placeholder-glyph {
    color: var(--color-border);
  }

  .placeholder-pane p {
    margin: 0;
    max-width: 32ch;
    font-size: var(--text-base);
    line-height: 1.6;
    color: var(--color-text-muted);
  }

  .keycap {
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 1px 4px;
  }

  .keycap-quiet {
    border-color: transparent;
    color: inherit;
    opacity: 0.6;
  }

  /* --- The URL bar ------------------------------------------------------ */

  .bar {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--color-border-soft);
  }

  .api-settings-title {
    flex-shrink: 0;
    font-size: var(--text-md);
    font-weight: 600;
    color: var(--color-text-strong);
  }

  .file-path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-text-faint);
  }

  .url-box {
    flex-grow: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    height: 2.125rem;
    padding: 0 var(--space-4);
    background: var(--color-field);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .url-box-problem {
    border-color: var(--color-err-line);
  }

  .url {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-text);
  }

  .url-note {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  .url-box-problem .url-note {
    color: var(--color-err);
  }

  .send-button {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 2.125rem;
    padding: 0 var(--space-5);
    border: none;
    border-radius: var(--radius-md);
    background: var(--color-accent);
    color: var(--color-accent-on);
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
  }

  .send-button:hover:not(:disabled) {
    background: var(--color-accent-strong);
  }

  .send-button:disabled {
    background: var(--color-border);
    color: var(--color-text-faint);
    cursor: default;
  }

  /* --- Meta row --------------------------------------------------------- */

  .meta-row {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-5);
    border-bottom: 1px solid var(--color-border-soft);
  }

  .auth-chip {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    background: var(--color-inset);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .auth-chip-chained {
    color: var(--color-text-soft);
  }

  .badge {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-shrink: 0;
    font-size: var(--text-xs);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  .badge-warn {
    background: var(--color-warn-tint);
    color: var(--color-warn);
  }

  .dirty-flag {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-accent);
  }

  .dirty-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
  }

  .ghost-button {
    flex-shrink: 0;
    align-self: flex-start;
    height: 1.75rem;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-inset);
    color: var(--color-text);
    cursor: pointer;
  }

  .ghost-button:hover {
    border-color: var(--color-text-faint);
  }

  .save-button {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 1.75rem;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    font-weight: 500;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-inset);
    color: var(--color-text);
    cursor: pointer;
  }

  .save-button:hover:not(:disabled) {
    border-color: var(--color-accent-line);
    color: var(--color-accent);
  }

  .save-button:disabled {
    color: var(--color-text-faint);
    cursor: default;
  }

  /* --- Tabs ------------------------------------------------------------- */

  .tab-strip {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--color-border-soft);
  }

  .tab {
    padding: var(--space-3) var(--space-3);
    font-size: var(--text-base);
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    box-shadow: inset 0 -2px 0 0 transparent;
  }

  .tab:hover {
    color: var(--color-text);
  }

  .tab-active {
    color: var(--color-text-strong);
    font-weight: 600;
    box-shadow: inset 0 -2px 0 0 var(--color-accent);
  }

  .file-name {
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
  }

  .tab-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }

  .editor-wrap {
    height: 100%;
    min-height: 0;
  }

  /* --- Banners, diagnostics --------------------------------------------- */

  .banner {
    flex-shrink: 0;
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-5);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .banner-error {
    background: var(--color-err-tint);
    color: var(--color-err);
    border-bottom: 1px solid var(--color-err-line);
  }

  .parse-error {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: var(--space-5);
    padding: var(--space-4);
    font-size: var(--text-sm);
    line-height: 1.55;
    color: var(--color-err);
    background: var(--color-err-tint);
    border: 1px solid var(--color-err-line);
    border-radius: var(--radius-lg);
  }

  .parse-error p {
    margin: 0;
  }

  .diagnostics {
    flex-shrink: 0;
    max-height: 30%;
    overflow-y: auto;
    border-top: 1px solid var(--color-border);
  }

  .diagnostic {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-5);
    font-size: var(--text-xs);
    border-bottom: 1px solid var(--color-border-soft);
  }

  .diagnostic:last-child {
    border-bottom: none;
  }

  .diagnostic-dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .diagnostic-path {
    flex-shrink: 0;
    font-family: var(--font-mono);
  }

  .diagnostic-message {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-muted);
  }

  .diagnostic-error {
    background: var(--color-err-tint);
  }

  .diagnostic-error .diagnostic-dot {
    background: var(--color-err);
  }

  .diagnostic-error .diagnostic-path {
    color: var(--color-err);
  }

  .diagnostic-warning {
    background: var(--color-warn-tint);
  }

  .diagnostic-warning .diagnostic-dot {
    background: var(--color-warn);
  }

  .diagnostic-warning .diagnostic-path {
    color: var(--color-warn);
  }
</style>
