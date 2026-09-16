<script lang="ts">
  import { lint, previewEndpoint, saveApi, type DiagnosticDto } from "./ipc";
  import { selectedApi, selectedEndpoint, ui } from "./state.svelte";
  import Editor from "./Editor.svelte";

  let diagnostics = $state<DiagnosticDto[]>([]);
  let previewUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let saving = $state(false);
  // apiId -> text that was last successfully written to disk by us. Used to
  // adopt the canonical (re-serialized) text once the post-save reload comes
  // back, but only if the user hasn't kept typing in the meantime.
  let cleanSnapshot: Record<string, string> = {};

  function parsesAsJson(text: string): boolean {
    try {
      JSON.parse(text);
      return true;
    } catch {
      return false;
    }
  }

  const api = $derived(selectedApi());
  const endpoint = $derived(selectedEndpoint());
  const bufferText = $derived(api ? ui.buffers[api.id] : undefined);
  const dirty = $derived(
    api !== undefined && bufferText !== undefined && bufferText !== api.text,
  );

  // Seed the buffer for a newly selected API, without ever clobbering a
  // buffer the user (or task 10's hot-reload logic) already owns.
  $effect(() => {
    const current = api;
    if (current && ui.buffers[current.id] === undefined) {
      ui.buffers[current.id] = current.text;
    }
  });

  // Once a save lands and the workspace reload brings back the canonical
  // (re-serialized) text, adopt it into the buffer — but only while the
  // buffer still holds exactly what we saved.
  $effect(() => {
    const current = api;
    if (!current) return;
    const clean = cleanSnapshot[current.id];
    if (
      clean !== undefined &&
      ui.buffers[current.id] === clean &&
      current.text !== clean
    ) {
      ui.buffers[current.id] = current.text;
      cleanSnapshot[current.id] = current.text;
    }
  });

  // Diagnostics strip: relint on every buffer change, debounced.
  $effect(() => {
    const current = api;
    const text = bufferText;
    if (!current || text === undefined) {
      diagnostics = [];
      return;
    }
    const handle = setTimeout(() => {
      void lint(text)
        .then((d) => {
          diagnostics = d;
        })
        .catch(() => {
          // lint never rejects per the IPC surface; ignore defensively.
        });
    }, 300);
    return () => clearTimeout(handle);
  });

  // Effective URL preview: debounced, and skipped while the buffer holds
  // unsaved changes that don't even parse (nothing sane to preview yet, and
  // this avoids hammering a real chained-auth endpoint on every keystroke of
  // a broken document).
  $effect(() => {
    const sel = ui.selected;
    const current = api;
    if (!sel || !current) {
      previewUrl = null;
      previewError = null;
      return;
    }
    const text = bufferText;
    const hasUnparsedEdits =
      text !== undefined && text !== current.text && !parsesAsJson(text);
    if (hasUnparsedEdits) {
      return;
    }
    const env = ui.env[current.id] ?? null;
    const handle = setTimeout(() => {
      previewEndpoint(current.id, sel.endpointId, env)
        .then((effective) => {
          previewUrl = effective.url;
          previewError = null;
        })
        .catch((e) => {
          previewUrl = null;
          previewError = e instanceof Error ? e.message : String(e);
        });
    }, 300);
    return () => clearTimeout(handle);
  });

  function onEditorChange(text: string): void {
    if (api) {
      ui.buffers[api.id] = text;
    }
  }

  async function handleSave(): Promise<void> {
    if (!api || bufferText === undefined || !dirty) return;
    const text = bufferText;
    saving = true;
    saveError = null;
    try {
      const result = await saveApi(api.id, text);
      diagnostics = result;
      const hasError = result.some((d) => d.severity === "error");
      if (!hasError) {
        ui.buffers[api.id] = text;
        cleanSnapshot[api.id] = text;
      }
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="request-panel-inner">
  {#if !api || !endpoint}
    <p class="placeholder">Select an endpoint</p>
  {:else}
    <header class="summary">
      <div class="summary-row">
        <span class="method method-{endpoint.method.toLowerCase()}"
          >{endpoint.method}</span
        >
        {#if previewUrl}
          <span class="url">{previewUrl}</span>
        {:else if previewError}
          <span class="url url-error">{previewError}</span>
        {:else}
          <span class="url url-pending">resolving…</span>
        {/if}
      </div>
      <div class="summary-row">
        <span class="auth-kind">auth: {endpoint.authKind}</span>
        <button
          type="button"
          class="save-button"
          disabled={!dirty || saving}
          onclick={handleSave}
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
      {#if saveError}
        <div class="save-error">{saveError}</div>
      {/if}
    </header>

    <div class="editor-wrap">
      <Editor
        value={bufferText ?? api.text}
        onChange={onEditorChange}
        {diagnostics}
      />
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

  .editor-wrap {
    flex: 1;
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
