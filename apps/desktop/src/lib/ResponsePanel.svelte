<script lang="ts">
  import { onDestroy } from "svelte";
  // NOT `navigator.clipboard`: Tauri's `WebviewAttributes.clipboard` defaults
  // to false and wry only enables WebKitGTK clipboard access when it is set,
  // so `navigator.clipboard.writeText` rejects in the packaged app on the only
  // platform this ships to (it works in a plain browser, which is why a
  // browser-pane check cannot catch this). The plugin writes through the
  // system clipboard directly.
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { curlCommand, type AuthStepDto } from "./ipc";
  import { detectBodyKind, prettyPrint, type BodyKind } from "./pretty";
  import { formatSize, statusClass } from "./format";
  import { isSelectionLive, selectedEndpoint, ui } from "./state.svelte";
  import Icon from "./ui/Icon.svelte";
  import MethodChip from "./ui/MethodChip.svelte";

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

  // Spec §8 asks for pretty-print on JSON, XML, HTML and plain text. The kind
  // comes from the Content-Type first and the body's own shape second, so a
  // server that sends XML as `text/plain` still gets indented.
  const bodyKind = $derived.by(() => {
    if (!response || response.bodyIsBinary) return "text" as BodyKind;
    return detectBodyKind(contentType(response.headers), response.body);
  });

  const canPrettyPrint = $derived(bodyKind !== "text");

  const displayedBody = $derived.by(() => {
    if (!response || response.bodyIsBinary) return "";
    if (!prettyBody || !canPrettyPrint) return response.body;
    return prettyPrint(response.body, bodyKind);
  });

  /**
   * The body, split into plain runs and matched runs. Each match carries its
   * ordinal so the find bar can say "3 of 17" and step between them — a
   * plain highlight-everything pass leaves you scrolling a 1 MB body by
   * hand looking for the next yellow mark.
   */
  const bodySegments = $derived.by(() => {
    const text = displayedBody;
    const q = findQuery.trim();
    if (!q) return [{ text, matchIndex: -1 }];
    const lower = text.toLowerCase();
    const needle = q.toLowerCase();
    const segments: { text: string; matchIndex: number }[] = [];
    let i = 0;
    let found = 0;
    while (i < text.length) {
      const idx = lower.indexOf(needle, i);
      if (idx === -1) {
        segments.push({ text: text.slice(i), matchIndex: -1 });
        break;
      }
      if (idx > i) segments.push({ text: text.slice(i, idx), matchIndex: -1 });
      segments.push({
        text: text.slice(idx, idx + needle.length),
        matchIndex: found++,
      });
      i = idx + needle.length;
    }
    return segments;
  });

  const matchCount = $derived(
    bodySegments.reduce((n, s) => (s.matchIndex >= 0 ? n + 1 : n), 0),
  );

  /** Which match the find bar is parked on. Always in range, or 0 for none. */
  let activeMatch = $state(0);
  let bodyEl = $state<HTMLPreElement | undefined>();

  // A new query (or a new body under the same query) starts again at the
  // first hit rather than keeping an index that now points somewhere else.
  $effect(() => {
    void findQuery;
    void displayedBody;
    activeMatch = 0;
  });

  // Bring the current match into view. Scrolling is the whole point of the
  // counter — without it, "12 of 40" tells you a number and nothing else.
  $effect(() => {
    const index = activeMatch;
    void matchCount;
    const container = bodyEl;
    if (!container) return;
    const mark = container.querySelector<HTMLElement>(
      `[data-match="${index}"]`,
    );
    mark?.scrollIntoView({ block: "center", behavior: "auto" });
  });

  /** Steps the find bar by `delta`, wrapping at both ends. */
  function stepMatch(delta: number): void {
    if (matchCount === 0) return;
    activeMatch = (activeMatch + delta + matchCount) % matchCount;
  }

  function onFindKeydown(event: KeyboardEvent): void {
    if (event.key !== "Enter") return;
    event.preventDefault();
    stepMatch(event.shiftKey ? -1 : 1);
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
        // "(masked)" is not decoration: secrets and chain tokens come out as
        // `***` (spec §5.3) and §5.3's "unless the user explicitly reveals
        // them" is not built, so the exported command cannot be run as-is.
        // Deferred to phase 3 — see SPEC.md.
        return "Copy as curl (masked)";
    }
  }

  async function handleCopyCurl(): Promise<void> {
    // `isSelectionLive()` guarantees `selectedEndpoint()` resolves, i.e. the
    // selection is not an API-only one (`endpointId: null`) — read the id
    // off the resolved endpoint rather than widening `ui.selected`'s type.
    if (!isSelectionLive()) return;
    const sel = ui.selected;
    const endpoint = selectedEndpoint();
    if (!sel || !endpoint) return;
    const apiId = sel.apiId;
    const endpointId = endpoint.id;
    const env = ui.env[apiId] ?? null;

    copyState = "copying";
    copyError = null;
    try {
      const text = await curlCommand(apiId, endpointId, env);
      await writeText(text);
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
  <header class="status-band">
    {#if response && !runError}
      <span class="status-figure status-{statusClass(response.status)}">
        {response.status}
      </span>
      <span class="divider"></span>
      <span class="meta">{response.elapsedMs} ms</span>
      <span class="meta meta-quiet">{formatSize(response.sizeBytes)}</span>
    {:else if runError}
      <span class="status-figure status-err">
        <Icon name="warning" size={18} />
      </span>
      <span class="meta meta-quiet">request failed</span>
    {:else}
      <span class="status-idle">No response yet</span>
    {/if}

    <span class="spacer"></span>

    <button
      type="button"
      class="ghost-button"
      disabled={!isSelectionLive()}
      onclick={handleCopyCurl}
    >
      <Icon name="copy" size={12} />
      {copyLabel()}
    </button>
  </header>

  {#if copyError}
    <div class="banner banner-error">
      <Icon name="warning" size={13} />
      <span>{copyError}</span>
    </div>
  {/if}

  {#if runError}
    <div class="run-error">
      <h3>The request did not complete</h3>
      <pre>{runError}</pre>
    </div>
  {:else if !response}
    <div class="placeholder-pane">
      <span class="placeholder-glyph"><Icon name="send" size={20} /></span>
      <p>
        Send the request to see its response — <kbd class="keycap">Ctrl Enter</kbd>,
        or the Send button on the URL bar.
      </p>
    </div>
  {:else}
    <div class="tab-strip" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "body"}
        class="tab"
        class:tab-active={activeTab === "body"}
        onclick={() => (activeTab = "body")}
      >
        Body
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "headers"}
        class="tab"
        class:tab-active={activeTab === "headers"}
        onclick={() => (activeTab = "headers")}
      >
        Headers
        <span class="tab-count">{response.headers.length}</span>
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "effective"}
        class="tab"
        class:tab-active={activeTab === "effective"}
        onclick={() => (activeTab = "effective")}
      >
        Effective request
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "auth"}
        class="tab"
        class:tab-active={activeTab === "auth"}
        onclick={() => (activeTab = "auth")}
      >
        <Icon name="chain" size={11} />
        Auth chain
        {#if response.authTrace.length > 0}
          <span class="tab-count">{response.authTrace.length}</span>
        {/if}
      </button>
    </div>

    <div class="tab-content">
      {#if activeTab === "body"}
        {#if response.bodyIsBinary}
          <div class="notice">
            Binary response body ({contentType(response.headers)}, {formatSize(
              response.sizeBytes,
            )}) — binary bodies are not displayed.
          </div>
        {:else}
          <div class="body-controls">
            {#if canPrettyPrint}
              <div class="segmented">
                <button
                  type="button"
                  class="segment"
                  class:segment-active={prettyBody}
                  onclick={() => (prettyBody = true)}
                >
                  Pretty
                </button>
                <button
                  type="button"
                  class="segment"
                  class:segment-active={!prettyBody}
                  onclick={() => (prettyBody = false)}
                >
                  Raw
                </button>
              </div>
            {/if}

            <div class="find-box">
              <span class="find-icon"><Icon name="search" size={12} /></span>
              <input
                type="text"
                class="find-input"
                placeholder="Find in body"
                aria-label="Find in body"
                bind:value={findQuery}
                onkeydown={onFindKeydown}
              />
              {#if findQuery.trim()}
                <span class="find-count" class:find-count-none={matchCount === 0}>
                  {matchCount === 0 ? "no matches" : `${activeMatch + 1} / ${matchCount}`}
                </span>
                <button
                  type="button"
                  class="find-step"
                  disabled={matchCount === 0}
                  onclick={() => stepMatch(-1)}
                  aria-label="Previous match"
                  title="Previous match (Shift+Enter)"
                >
                  <Icon name="up" size={12} />
                </button>
                <button
                  type="button"
                  class="find-step"
                  disabled={matchCount === 0}
                  onclick={() => stepMatch(1)}
                  aria-label="Next match"
                  title="Next match (Enter)"
                >
                  <Icon name="down" size={12} />
                </button>
              {/if}
            </div>
          </div>

          {#if response.bodyTruncated}
            <p class="truncated-note">
              <Icon name="warning" size={12} />
              Showing the first 1 MB of a {formatSize(response.sizeBytes)} response.
            </p>
          {/if}

          <pre class="body-text" bind:this={bodyEl}>{#each bodySegments as seg, i (i)}{#if seg.matchIndex >= 0}<mark
                  data-match={seg.matchIndex}
                  class:match-active={seg.matchIndex === activeMatch}>{seg.text}</mark
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
          <MethodChip method={response.effective.method} />
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
          <p class="notice">No request body.</p>
        {/if}
      {:else if activeTab === "auth"}
        {#if response.authTrace.length === 0}
          <p class="notice">
            This endpoint resolved without an auth chain — nothing was fetched
            on its behalf.
          </p>
        {:else}
          <!-- The trace is drawn as a chain, not a list: each step is a link,
               and the connector between two links is where the token the
               previous step produced gets handed to the next one. That
               hand-off is the thing reqchain does that a flat log hides. -->
          <ol class="chain">
            {#each response.authTrace as step, i (i)}
              <li class="link">
                <div class="link-rail">
                  <span class="link-number" class:link-number-cached={step.fromCache}>
                    {i + 1}
                  </span>
                  <span class="link-line"></span>
                </div>

                <div class="link-card">
                  <details open={!step.fromCache}>
                    <summary class="link-head">
                      <span class="link-id">{step.endpointId}</span>
                      {#if step.request}
                        <MethodChip method={step.request.method} />
                        <span class="url url-quiet">{step.request.url}</span>
                      {:else}
                        <span class="url-note">no request recorded</span>
                      {/if}
                      <span class="spacer"></span>
                      {#if step.fromCache}
                        <span class="cache-pill">
                          <Icon name="clock" size={11} />
                          from cache
                        </span>
                      {:else}
                        <span
                          class="step-status status-{statusClass(step.status ?? 0)}"
                        >
                          {authStatusLabel(step)}
                        </span>
                      {/if}
                    </summary>

                    <div class="link-body">
                      {#if step.request}
                        <div class="link-section">
                          <span class="section-label">Sent</span>
                          <table class="kv-table kv-table-compact">
                            <tbody>
                              {#each step.request.headers as [name, value], j (j)}
                                <tr><td>{name}</td><td>{value}</td></tr>
                              {/each}
                            </tbody>
                          </table>
                          {#if step.request.body !== null}
                            <pre class="body-text body-text-inset">{step.request.body}</pre>
                          {/if}
                        </div>
                      {:else}
                        <p class="notice">
                          Nothing was sent for this step — the cached token was
                          still valid.
                        </p>
                      {/if}

                      <div class="link-section">
                        <span class="section-label">Received</span>
                        <pre class="body-text body-text-inset">{step.body}</pre>
                      </div>
                    </div>
                  </details>
                </div>
              </li>

              <li class="handoff" aria-hidden="true">
                <div class="link-rail"><span class="link-line"></span></div>
                <span class="handoff-pill">
                  <Icon name="chain" size={11} />
                  token
                  <Icon name="arrow-right" size={12} />
                  {i === response.authTrace.length - 1
                    ? "your request"
                    : "next step"}
                </span>
              </li>
            {/each}

            <li class="link link-final">
              <div class="link-rail">
                <span class="link-number link-number-final">
                  <Icon name="send" size={11} />
                </span>
              </div>
              <div class="link-card link-card-final">
                <div class="link-head">
                  <span class="section-label">Your request</span>
                  <MethodChip method={response.effective.method} />
                  <span class="url url-quiet">{response.effective.url}</span>
                  <span class="spacer"></span>
                  <span class="step-status status-{statusClass(response.status)}">
                    {response.status}
                  </span>
                </div>
              </div>
            </li>
          </ol>

          <p class="chain-footnote">
            <Icon name="info" size={13} />
            Tokens live in memory for this run only — nothing is written to the
            workspace files.
          </p>
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
    background: var(--color-bg);
  }

  .spacer {
    flex-grow: 1;
  }

  /* --- Status band ------------------------------------------------------ */

  .status-band {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--color-border-soft);
  }

  /* The status carries the color for the whole row; nothing else in it
     competes. */
  .status-figure {
    display: flex;
    align-items: center;
    font-size: var(--text-status);
    font-weight: 600;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
    line-height: 1;
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

  .status-idle {
    font-size: var(--text-sm);
    color: var(--color-text-faint);
  }

  .divider {
    width: 1px;
    height: 1.125rem;
    background: var(--color-border);
  }

  .meta {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-text-soft);
    font-variant-numeric: tabular-nums;
  }

  .meta-quiet {
    color: var(--color-text-muted);
  }

  .ghost-button {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 1.75rem;
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-inset);
    color: var(--color-text);
    cursor: pointer;
  }

  .ghost-button:hover:not(:disabled) {
    border-color: var(--color-text-faint);
  }

  .ghost-button:disabled {
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
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3);
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

  .tab-count {
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
    font-variant-numeric: tabular-nums;
  }

  .tab-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }

  /* --- Body ------------------------------------------------------------- */

  .body-controls {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-5);
  }

  .segmented {
    display: flex;
    gap: 2px;
    padding: 2px;
    background: var(--color-inset);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .segment {
    padding: var(--space-1) var(--space-4);
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .segment-active {
    background: var(--color-panel);
    color: var(--color-text-strong);
    font-weight: 600;
  }

  .find-box {
    flex-grow: 1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 1.75rem;
    padding: 0 var(--space-2) 0 var(--space-3);
    background: var(--color-field);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .find-box:focus-within {
    border-color: var(--color-accent-line);
  }

  .find-icon {
    display: flex;
    color: var(--color-text-faint);
  }

  .find-input {
    flex-grow: 1;
    min-width: 0;
    border: none;
    background: transparent;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .find-input:focus {
    outline: none;
  }

  .find-count {
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .find-count-none {
    color: var(--color-err);
  }

  .find-step {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .find-step:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-text-strong);
  }

  .find-step:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .body-text {
    margin: 0;
    padding: 0 var(--space-5) var(--space-5);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: 1.75;
    color: var(--color-text-soft);
    white-space: pre-wrap;
    word-break: break-word;
    overflow-x: auto;
  }

  .body-text-inset {
    padding: var(--space-3) var(--space-4);
    margin-top: var(--space-2);
    background: var(--color-field);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
    line-height: 1.6;
    max-height: 14rem;
    overflow: auto;
  }

  mark {
    background: var(--color-warn-tint);
    color: inherit;
    border-radius: 2px;
  }

  /* The one match you are parked on gets the accent; the rest stay a quiet
     tint, so "3 of 17" has something to point at. */
  .match-active {
    background: var(--color-accent);
    color: var(--color-accent-on);
  }

  .truncated-note {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0 var(--space-5) var(--space-3);
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-xs);
    color: var(--color-warn);
    background: var(--color-warn-tint);
    border-radius: var(--radius-sm);
  }

  /* --- Tables ----------------------------------------------------------- */

  .kv-table {
    width: 100%;
    border-collapse: collapse;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    table-layout: fixed;
  }

  .kv-table th {
    text-align: left;
    font-family: var(--font-condensed);
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--color-text-faint);
    padding: var(--space-2) var(--space-5);
    border-bottom: 1px solid var(--color-border);
  }

  .kv-table th:first-child,
  .kv-table td:first-child {
    width: 14rem;
    color: var(--color-text-soft);
  }

  .kv-table td {
    padding: var(--space-2) var(--space-5);
    border-bottom: 1px solid var(--color-border-soft);
    color: var(--color-text-muted);
    word-break: break-word;
    vertical-align: top;
  }

  .kv-table-compact td {
    padding: var(--space-1) 0;
    border-bottom: none;
  }

  .kv-table-compact td:first-child {
    width: 11rem;
    padding-right: var(--space-3);
  }

  /* --- Effective request ------------------------------------------------ */

  .effective-line {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-5);
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

  .url-quiet {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .url-note {
    font-size: var(--text-xs);
    color: var(--color-text-faint);
  }

  /* --- The auth chain --------------------------------------------------- */

  .chain {
    list-style: none;
    margin: 0;
    padding: var(--space-5) var(--space-5) 0;
    display: flex;
    flex-direction: column;
  }

  .link,
  .handoff {
    display: flex;
    gap: var(--space-4);
    align-items: stretch;
  }

  .link-rail {
    flex-shrink: 0;
    width: 1.625rem;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .link-number {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.625rem;
    height: 1.625rem;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--color-ok-tint);
    color: var(--color-ok);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .link-number-cached {
    background: var(--color-inset);
    color: var(--color-text-muted);
  }

  .link-number-final {
    background: var(--color-accent);
    color: var(--color-accent-on);
  }

  /* The dashed rail is what makes consecutive steps read as one chain
     rather than as unrelated cards. */
  .link-line {
    flex-grow: 1;
    width: 1px;
    background: repeating-linear-gradient(
      to bottom,
      var(--color-border) 0 4px,
      transparent 4px 8px
    );
  }

  .link-card {
    flex-grow: 1;
    min-width: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-panel);
    overflow: hidden;
  }

  .link-card-final {
    border-color: var(--color-accent-line);
    background: var(--color-accent-tint);
  }

  .link-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4);
    cursor: pointer;
    list-style: none;
  }

  .link-head::-webkit-details-marker {
    display: none;
  }

  .link-id {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--color-text-strong);
  }

  .step-status {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .cache-pill {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px var(--space-3);
    border-radius: var(--radius-pill);
    background: var(--color-info-tint);
    color: var(--color-info);
    font-size: var(--text-xs);
  }

  .link-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: 0 var(--space-4) var(--space-4);
    border-top: 1px solid var(--color-border-soft);
    padding-top: var(--space-4);
  }

  .link-section {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .section-label {
    font-family: var(--font-condensed);
    font-weight: 600;
    font-size: 0.625rem;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--color-text-faint);
  }

  .handoff {
    min-height: 2.75rem;
    align-items: center;
  }

  .handoff .link-rail {
    align-self: stretch;
  }

  .handoff-pill {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border: 1px dashed var(--color-accent-line);
    border-radius: var(--radius-pill);
    background: var(--color-accent-tint);
    color: var(--color-accent);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .chain-footnote {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-4) var(--space-5) var(--space-5);
    padding: var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  /* --- Notices, errors, empty ------------------------------------------- */

  .notice {
    margin: 0;
    padding: var(--space-4) var(--space-5);
    font-size: var(--text-sm);
    line-height: 1.6;
    color: var(--color-text-muted);
  }

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

  .run-error {
    margin: var(--space-5);
    padding: var(--space-4);
    border: 1px solid var(--color-err-line);
    border-left: 3px solid var(--color-err);
    border-radius: var(--radius-lg);
    background: var(--color-err-tint);
  }

  .run-error h3 {
    margin: 0 0 var(--space-3);
    font-size: var(--text-base);
    color: var(--color-err);
  }

  .run-error pre {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.6;
    color: var(--color-text-soft);
    white-space: pre-wrap;
    word-break: break-word;
  }

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
    max-width: 34ch;
    font-size: var(--text-base);
    line-height: 1.7;
    color: var(--color-text-muted);
  }

  .keycap {
    font-family: var(--font-mono);
    font-size: 0.625rem;
    color: var(--color-text-faint);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 1px 4px;
    white-space: nowrap;
  }
</style>
