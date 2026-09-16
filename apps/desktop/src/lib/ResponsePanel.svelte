<script lang="ts">
  import { onDestroy } from "svelte";
  import { curlCommand, type AuthStepDto } from "./ipc";
  import {
    canSendSelected,
    isSelectionLive,
    sendSelected,
    ui,
  } from "./state.svelte";

  type Tab = "body" | "headers" | "effective" | "auth";

  let activeTab = $state<Tab>("body");
  let prettyBody = $state(true);
  let findQuery = $state("");
  let copyState = $state<"idle" | "copying" | "copied" | "error">("idle");
  let copyError = $state<string | null>(null);
  let copyResetHandle: ReturnType<typeof setTimeout> | undefined;

  const response = $derived(ui.response);
  const runError = $derived(ui.runError);

  // Selection changed: drop feedback that belonged to whatever was
  // previously on screen (the in-flight run itself is invalidated by
  // `select()`'s own sequence bump in state.svelte.ts).
  $effect(() => {
    void ui.selected;
    copyState = "idle";
    copyError = null;
  });

  const parsedBody = $derived.by(() => {
    if (!response || response.bodyIsBinary) return undefined;
    try {
      return JSON.parse(response.body) as unknown;
    } catch {
      return undefined;
    }
  });

  const bodyIsJson = $derived(parsedBody !== undefined);

  const displayedBody = $derived.by(() => {
    if (!response || response.bodyIsBinary) return "";
    if (bodyIsJson && prettyBody) {
      return JSON.stringify(parsedBody, null, 2);
    }
    return response.body;
  });

  const bodySegments = $derived.by(() => {
    const text = displayedBody;
    const q = findQuery.trim();
    if (!q) return [{ text, match: false }];
    const lower = text.toLowerCase();
    const needle = q.toLowerCase();
    const segments: { text: string; match: boolean }[] = [];
    let i = 0;
    while (i < text.length) {
      const idx = lower.indexOf(needle, i);
      if (idx === -1) {
        segments.push({ text: text.slice(i), match: false });
        break;
      }
      if (idx > i) segments.push({ text: text.slice(i, idx), match: false });
      segments.push({
        text: text.slice(idx, idx + needle.length),
        match: true,
      });
      i = idx + needle.length;
    }
    return segments;
  });

  const tabs = $derived([
    { id: "body" as const, label: "Body" },
    { id: "headers" as const, label: "Headers" },
    { id: "effective" as const, label: "Effective" },
    {
      id: "auth" as const,
      label: response ? `Auth (${response.authTrace.length})` : "Auth",
    },
  ]);

  function statusClass(status: number): string {
    const leading = Math.floor(status / 100);
    if (leading === 2) return "ok";
    if (leading === 3) return "redirect";
    return "err";
  }

  function formatSize(bytes: number): string {
    return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(2)} KB`;
  }

  function contentType(headers: [string, string][]): string {
    const h = headers.find(([name]) => name.toLowerCase() === "content-type");
    return h ? h[1] : "unknown";
  }

  function authStatusLabel(step: AuthStepDto): string {
    return step.status === null ? "from cache" : String(step.status);
  }

  function copyLabel(): string {
    switch (copyState) {
      case "copying":
        return "Copying…";
      case "copied":
        return "Copied!";
      default:
        return "Copy as curl";
    }
  }

  function handleSend(): void {
    void sendSelected();
  }

  /**
   * Imperative entry point for the Ctrl+Enter shortcut (App.svelte, via
   * `bind:this`). Returns whether a send was actually triggered, so the
   * caller only calls `preventDefault` when the shortcut did something.
   */
  export function sendCurrent(): boolean {
    if (!canSendSelected()) return false;
    void sendSelected();
    return true;
  }

  async function handleCopyCurl(): Promise<void> {
    if (!isSelectionLive()) return;
    const sel = ui.selected;
    if (!sel) return;
    const apiId = sel.apiId;
    const endpointId = sel.endpointId;
    const env = ui.env[apiId] ?? null;

    copyState = "copying";
    copyError = null;
    try {
      const text = await curlCommand(apiId, endpointId, env);
      await navigator.clipboard.writeText(text);
      if (ui.selected?.apiId === apiId && ui.selected?.endpointId === endpointId) {
        copyState = "copied";
        if (copyResetHandle) clearTimeout(copyResetHandle);
        copyResetHandle = setTimeout(() => {
          copyState = "idle";
        }, 1500);
      }
    } catch (e) {
      if (ui.selected?.apiId === apiId && ui.selected?.endpointId === endpointId) {
        copyState = "error";
        copyError = e instanceof Error ? e.message : String(e);
      }
    }
  }

  onDestroy(() => {
    if (copyResetHandle) clearTimeout(copyResetHandle);
  });
</script>

<div class="response-panel-inner">
  <header class="topbar">
    <button
      type="button"
      class="send-button"
      disabled={!canSendSelected()}
      onclick={handleSend}
    >
      {ui.running ? "Sending…" : "Send"}
    </button>

    {#if response && !runError}
      <span class="status status-{statusClass(response.status)}"
        >{response.status}</span
      >
      <span class="meta">{response.elapsedMs} ms</span>
      <span class="meta">{formatSize(response.sizeBytes)}</span>
    {/if}

    <button
      type="button"
      class="copy-button"
      disabled={!isSelectionLive()}
      onclick={handleCopyCurl}
    >
      {copyLabel()}
    </button>
  </header>

  {#if copyError}
    <div class="copy-error">{copyError}</div>
  {/if}

  {#if runError}
    <div class="run-error">{runError}</div>
  {:else if !response}
    <p class="placeholder">Send a request to see the response.</p>
  {:else}
    <nav class="tabs">
      {#each tabs as t (t.id)}
        <button
          type="button"
          class="tab"
          class:active={activeTab === t.id}
          onclick={() => (activeTab = t.id)}
        >
          {t.label}
        </button>
      {/each}
    </nav>

    <div class="tab-content">
      {#if activeTab === "body"}
        {#if response.bodyIsBinary}
          <p class="placeholder">
            Binary response body ({contentType(response.headers)}, {formatSize(
              response.sizeBytes,
            )}) — binary bodies are not displayed.
          </p>
        {:else}
          <div class="body-controls">
            {#if bodyIsJson}
              <button type="button" onclick={() => (prettyBody = !prettyBody)}>
                {prettyBody ? "Raw" : "Pretty"}
              </button>
            {/if}
            <input
              type="text"
              class="find-input"
              placeholder="Find in body"
              aria-label="Find in body"
              bind:value={findQuery}
            />
          </div>
          <pre class="body-text">{#each bodySegments as seg, i (i)}{#if seg.match}<mark
                  >{seg.text}</mark
                >{:else}{seg.text}{/if}{/each}</pre>
        {/if}
      {:else if activeTab === "headers"}
        <table class="kv-table">
          <thead>
            <tr><th>Name</th><th>Value</th></tr>
          </thead>
          <tbody>
            {#each response.headers as [name, value], i (i)}
              <tr><td>{name}</td><td>{value}</td></tr>
            {/each}
          </tbody>
        </table>
      {:else if activeTab === "effective"}
        <div class="effective-line">
          <span class="method method-{response.effective.method.toLowerCase()}"
            >{response.effective.method}</span
          >
          <span class="url">{response.effective.url}</span>
        </div>
        <table class="kv-table">
          <thead>
            <tr><th>Name</th><th>Value</th></tr>
          </thead>
          <tbody>
            {#each response.effective.headers as [name, value], i (i)}
              <tr><td>{name}</td><td>{value}</td></tr>
            {/each}
          </tbody>
        </table>
        {#if response.effective.body !== null}
          <pre class="body-text">{response.effective.body}</pre>
        {:else}
          <p class="placeholder">No request body.</p>
        {/if}
      {:else if activeTab === "auth"}
        {#if response.authTrace.length === 0}
          <p class="placeholder">No auth trace for this run.</p>
        {:else}
          <div class="auth-trace">
            {#each response.authTrace as step, i (i)}
              <details class="auth-step" open>
                <summary>
                  <span class="auth-endpoint">{step.endpointId}</span>
                  {#if step.fromCache}
                    <span class="badge badge-cache">from cache</span>
                  {/if}
                  <span class="auth-status">{authStatusLabel(step)}</span>
                </summary>
                <div class="auth-step-body">
                  <h4>Request</h4>
                  {#if step.request}
                    <div class="effective-line">
                      <span
                        class="method method-{step.request.method.toLowerCase()}"
                        >{step.request.method}</span
                      >
                      <span class="url">{step.request.url}</span>
                    </div>
                    <table class="kv-table">
                      <tbody>
                        {#each step.request.headers as [name, value], j (j)}
                          <tr><td>{name}</td><td>{value}</td></tr>
                        {/each}
                      </tbody>
                    </table>
                    {#if step.request.body !== null}
                      <pre class="body-text">{step.request.body}</pre>
                    {/if}
                  {:else}
                    <p class="placeholder">
                      No request recorded (served from cache).
                    </p>
                  {/if}
                  <h4>Response body</h4>
                  <pre class="body-text">{step.body}</pre>
                </div>
              </details>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .response-panel-inner {
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

  .topbar {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-shrink: 0;
  }

  .send-button,
  .copy-button {
    padding: 0.3rem 0.75rem;
    font-size: 0.8rem;
    border: 1px solid var(--color-accent);
    border-radius: 4px;
    background: var(--color-accent);
    color: #fff;
    cursor: pointer;
  }

  .copy-button {
    margin-left: auto;
    background: var(--color-surface);
    color: var(--color-text);
    border-color: var(--color-border);
  }

  .send-button:disabled,
  .copy-button:disabled {
    background: var(--color-border);
    border-color: var(--color-border);
    color: var(--color-text-muted);
    cursor: default;
  }

  .status {
    font-size: 0.75rem;
    font-weight: 700;
    padding: 0.15rem 0.4rem;
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

  .meta {
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .run-error,
  .copy-error {
    font-size: 0.8rem;
    color: var(--color-error-text);
    background: var(--color-error-bg);
    border: 1px solid var(--color-error-text);
    border-radius: 4px;
    padding: 0.4rem 0.6rem;
    white-space: pre-wrap;
  }

  .tabs {
    display: flex;
    gap: 0.25rem;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .tab {
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    border: none;
    background: none;
    cursor: pointer;
    color: var(--color-text-muted);
    border-bottom: 2px solid transparent;
  }

  .tab.active {
    color: var(--color-text);
    border-bottom-color: var(--color-accent);
  }

  .tab-content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .body-controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .body-controls button {
    padding: 0.2rem 0.5rem;
    font-size: 0.75rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-surface);
    cursor: pointer;
  }

  .find-input {
    flex: 1;
    min-width: 0;
    padding: 0.2rem 0.4rem;
    font-size: 0.75rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
  }

  .body-text {
    margin: 0;
    padding: 0.5rem;
    white-space: pre-wrap;
    word-break: break-word;
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.8rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-surface);
  }

  .body-text mark {
    background: #fff2a8;
    color: inherit;
  }

  .kv-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  .kv-table th,
  .kv-table td {
    text-align: left;
    padding: 0.25rem 0.4rem;
    border-bottom: 1px solid var(--color-border);
    word-break: break-word;
  }

  .kv-table th {
    color: var(--color-text-muted);
    font-weight: 600;
  }

  .effective-line {
    display: flex;
    align-items: center;
    gap: 0.5rem;
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
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.8rem;
    overflow-wrap: anywhere;
  }

  .auth-trace {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .auth-step {
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0.4rem 0.6rem;
  }

  .auth-step summary {
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
  }

  .auth-endpoint {
    font-weight: 600;
  }

  .auth-status {
    margin-left: auto;
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .badge {
    font-size: 0.7rem;
    padding: 0.1rem 0.35rem;
    border-radius: 3px;
  }

  .badge-cache {
    background: var(--color-selected);
    color: var(--color-text);
  }

  .auth-step-body {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-top: 0.5rem;
  }

  .auth-step-body h4 {
    margin: 0;
    font-size: 0.75rem;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
</style>
