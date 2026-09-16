<script lang="ts">
  import { lint, previewEndpoint, saveApi, type DiagnosticDto } from "./ipc";
  import {
    canWriteApiDoc,
    discardLocalChanges,
    selectedApi,
    selectedApiOrRemoved,
    selectedEndpoint,
    selectedEndpointOrRemoved,
    ui,
  } from "./state.svelte";
  import Editor from "./Editor.svelte";
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

  function handleDiscard(): void {
    if (liveApi) {
      discardLocalChanges(liveApi.id);
    }
  }
</script>

<div class="request-panel-inner">
  {#if !api}
    <p class="placeholder">Select an endpoint</p>
  {:else if isApiOnlySelected}
    <div class="api-settings-placeholder" data-testid="api-settings-placeholder">
      <p class="placeholder">
        <strong>{api.name}</strong> — API settings (coming soon)
      </p>
    </div>
  {:else if !endpoint}
    <p class="placeholder">Select an endpoint</p>
  {:else}
    {#if isRemoved}
      <div class="removed-banner">
        This endpoint no longer exists in the workspace files — showing the
        last known content{#if !liveApi} (the API file itself is gone){/if}.
      </div>
    {/if}

    <header class="summary">
      <div class="summary-row">
        <span class="method method-{endpoint.method.toLowerCase()}"
          >{endpoint.method}</span
        >
        {#if isRemoved}
          <span class="url url-error">endpoint removed</span>
        {:else if previewUrl}
          <span class="url">{previewUrl}</span>
        {:else if previewError}
          <span class="url url-error">{previewError}</span>
        {:else}
          <span class="url url-pending">resolving…</span>
        {/if}
      </div>
      <div class="summary-row">
        <span class="auth-kind">auth: {endpoint.authKind}</span>
        {#if diskChanged}
          <span class="badge badge-disk-changed">changed on disk</span>
          <button type="button" class="discard-button" onclick={handleDiscard}>
            Discard mine
          </button>
        {/if}
        <button
          type="button"
          class="save-button"
          disabled={!liveApi || !dirty || saving}
          onclick={handleSave}
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
      {#if saveError}
        <div class="save-error">{saveError}</div>
      {/if}
    </header>

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
                class="switch-to-json"
                onclick={() => (activeTab = "json")}
              >
                Switch to JSON
              </button>
            {/if}
          </div>
        {:else}
          <EndpointForm endpointId={endpoint.id} />
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
            {d.path} — {d.message}
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
    gap: 0.5rem;
  }

  .placeholder {
    color: var(--color-text-muted);
    font-style: italic;
  }

  .api-settings-placeholder {
    border: 1px dashed var(--color-border);
    border-radius: 4px;
    padding: 1rem;
  }

  .summary {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    flex-shrink: 0;
  }

  .summary-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    justify-content: space-between;
  }

  .method {
    flex-shrink: 0;
    font-size: 0.7rem;
    font-weight: 700;
    padding: 0.15rem 0.4rem;
    border-radius: 3px;
    background: var(--color-border);
    color: var(--color-text);
    min-width: 3rem;
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

  .url {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.8rem;
  }

  .url-pending,
  .url-error {
    color: var(--color-text-muted);
    font-style: italic;
    font-family: inherit;
  }

  .auth-kind {
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .save-button {
    padding: 0.3rem 0.75rem;
    font-size: 0.8rem;
    border: 1px solid var(--color-accent);
    border-radius: 4px;
    background: var(--color-accent);
    color: #fff;
    cursor: pointer;
  }

  .save-button:disabled {
    background: var(--color-border);
    border-color: var(--color-border);
    color: var(--color-text-muted);
    cursor: default;
  }

  .save-error {
    font-size: 0.75rem;
    color: var(--color-error-text);
  }

  .removed-banner {
    flex-shrink: 0;
    font-size: 0.8rem;
    padding: 0.4rem 0.6rem;
    background: var(--color-error-bg);
    color: var(--color-error-text);
    border: 1px solid var(--color-error-text);
    border-radius: 4px;
  }

  .badge {
    flex-shrink: 0;
    font-size: 0.7rem;
    padding: 0.1rem 0.35rem;
    border-radius: 3px;
  }

  .badge-disk-changed {
    background: var(--color-method-put);
    color: #1a1d21;
  }

  .discard-button {
    flex-shrink: 0;
    padding: 0.2rem 0.5rem;
    font-size: 0.75rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-surface);
    cursor: pointer;
  }

  .tab-strip {
    flex-shrink: 0;
    display: flex;
    gap: 0.25rem;
    border-bottom: 1px solid var(--color-border);
  }

  .tab {
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .tab-active {
    color: var(--color-text);
    border-bottom-color: var(--color-accent);
  }

  .tab-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .parse-error {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem;
    font-size: 0.8rem;
    color: var(--color-error-text);
    background: var(--color-error-bg);
    border: 1px solid var(--color-error-text);
    border-radius: 4px;
  }

  .switch-to-json {
    align-self: flex-start;
    padding: 0.25rem 0.6rem;
    font-size: 0.75rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-surface);
    cursor: pointer;
  }

  .editor-wrap {
    height: 100%;
    min-height: 0;
  }

  .diagnostics {
    flex-shrink: 0;
    max-height: 30%;
    overflow-y: auto;
    border-top: 1px solid var(--color-border);
    font-size: 0.75rem;
  }

  .diagnostic {
    padding: 0.25rem 0.4rem;
    border-bottom: 1px solid var(--color-border);
  }

  .diagnostic-error {
    background: var(--color-error-bg);
    color: var(--color-error-text);
  }

  .diagnostic-warning {
    background: #fdf3d9;
    color: #8a5a00;
  }
</style>
