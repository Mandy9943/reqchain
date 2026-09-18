<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import RequestPanel from "./lib/RequestPanel.svelte";
  import ResponsePanel from "./lib/ResponsePanel.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import SecretsScreen from "./lib/SecretsScreen.svelte";
  import Icon from "./lib/ui/Icon.svelte";
  import { canSendSelected, reload, sendSelected, ui } from "./lib/state.svelte";
  import { applyTheme, nextTheme, setTheme, theme, themeLabel } from "./lib/theme.svelte";

  let requestPanelRef: RequestPanel | undefined = $state();
  // Owns whether the secrets screen is shown. Mounting/unmounting the
  // component (rather than just hiding it) is what makes "leaving the screen
  // re-masks" hold even if a bug skipped the toggle's own re-mask path —
  // `SecretsScreen`'s `onDestroy` runs whenever this flips back to false.
  let showSecrets = $state(false);
  let reloading = $state(false);

  const endpointCount = $derived(
    ui.workspace.apis.reduce((n, api) => n + api.endpoints.length, 0),
  );

  onMount(() => {
    // The stored choice has to reach <html> before the first paint of a
    // themed pixel; app.css's media query covers the "system" case on its
    // own, so this only ever stamps an explicit override.
    applyTheme(theme.choice);
    void reload();
  });

  /** Focuses and selects the sidebar search input. Returns whether it found one. */
  function focusSidebarSearch(): boolean {
    const el = document.querySelector<HTMLInputElement>(
      'input[aria-label="Search endpoints"]',
    );
    if (!el) return false;
    el.focus();
    el.select();
    return true;
  }

  // Single window-level keydown listener for the app's shortcuts:
  //   Ctrl+Enter — send the selected endpoint
  //   Ctrl+K     — focus and select the sidebar search
  //   Ctrl+S     — save the current buffer
  // Each only calls preventDefault when it actually did something, so the
  // browser/webview default (e.g. its own save dialog) still runs on a
  // no-op press, and normal typing inside the CodeMirror editor is never
  // hijacked — none of these three combos are part of its own keymap.
  function handleKeydown(event: KeyboardEvent): void {
    if (!event.ctrlKey) return;

    if (event.key === "Enter") {
      // Sends through the shared state directly rather than through a panel
      // ref: the Send control lives on the request's URL bar now, and the
      // shortcut must keep working even while the secrets screen has the
      // request/response panels unmounted.
      if (canSendSelected()) {
        void sendSelected();
        event.preventDefault();
      }
    } else if (event.key === "k" || event.key === "K") {
      if (focusSidebarSearch()) {
        event.preventDefault();
      }
    } else if (event.key === "s" || event.key === "S") {
      if (requestPanelRef?.saveCurrent()) {
        event.preventDefault();
      }
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });

  async function handleReload(): Promise<void> {
    reloading = true;
    try {
      await reload();
    } finally {
      reloading = false;
    }
  }
</script>

<div class="window">
  <header class="titlebar">
    <span class="wordmark">
      <span class="wordmark-glyph"><Icon name="chain" size={15} /></span>
      reqchain
    </span>

    <span class="workspace-count">
      {ui.workspace.apis.length}
      {ui.workspace.apis.length === 1 ? "API" : "APIs"} · {endpointCount}
      {endpointCount === 1 ? "endpoint" : "endpoints"}
    </span>

    <span class="spacer"></span>

    <button
      type="button"
      class="tool"
      onclick={handleReload}
      disabled={reloading}
      title="Reload the workspace from disk"
    >
      <Icon name="refresh" size={13} />
      {reloading ? "Reloading…" : "Reload"}
    </button>

    <button
      type="button"
      class="tool"
      class:tool-active={showSecrets}
      onclick={() => (showSecrets = !showSecrets)}
      aria-pressed={showSecrets}
    >
      <Icon name="key" size={13} />
      Secrets
    </button>

    <button
      type="button"
      class="tool tool-icon"
      onclick={() => setTheme(nextTheme(theme.choice))}
      title={`Theme: ${themeLabel(theme.choice)} — click to change`}
      aria-label={`Theme: ${themeLabel(theme.choice)}`}
    >
      <Icon
        name={theme.choice === "light"
          ? "sun"
          : theme.choice === "dark"
            ? "moon"
            : "system"}
        size={13}
      />
    </button>
  </header>

  <main class="layout">
    <Sidebar />

    {#if showSecrets}
      <section class="panel secrets-panel">
        <SecretsScreen onClose={() => (showSecrets = false)} />
      </section>
    {:else}
      <section class="panel request-panel">
        <RequestPanel bind:this={requestPanelRef} />
      </section>

      <section class="panel response-panel">
        <div class="response-stack">
          <div class="response-main">
            <ResponsePanel />
          </div>
          <HistoryPanel />
        </div>
      </section>
    {/if}
  </main>
</div>

{#if ui.error}
  <div class="global-error" role="alert">
    <Icon name="warning" size={14} />
    <span>{ui.error}</span>
  </div>
{/if}

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    min-height: 0;
    background: var(--color-bg);
  }

  .titlebar {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 3rem;
    padding: 0 var(--space-5);
    background: var(--color-panel);
    border-bottom: 1px solid var(--color-border);
  }

  .wordmark {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-family: var(--font-display);
    font-weight: 700;
    font-size: var(--text-md);
    letter-spacing: -0.015em;
    color: var(--color-text-strong);
  }

  .wordmark-glyph {
    display: flex;
    color: var(--color-accent);
  }

  .workspace-count {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .spacer {
    flex-grow: 1;
  }

  .tool {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 1.875rem;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-inset);
    color: var(--color-text-soft);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .tool:hover:not(:disabled) {
    color: var(--color-text-strong);
    border-color: var(--color-text-faint);
  }

  .tool:disabled {
    color: var(--color-text-faint);
    cursor: default;
  }

  .tool-active {
    color: var(--color-accent);
    border-color: var(--color-accent-line);
    background: var(--color-accent-tint);
  }

  .tool-icon {
    width: 1.875rem;
    padding: 0;
    justify-content: center;
  }

  .layout {
    flex-grow: 1;
    display: grid;
    grid-template-columns: 300px minmax(0, 1fr) minmax(0, 1fr);
    min-height: 0;
  }

  .panel {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-bg);
  }

  .response-stack {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .response-main {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .request-panel {
    border-right: 1px solid var(--color-border);
  }

  .secrets-panel {
    grid-column: 2 / 4;
    overflow: auto;
  }

  .global-error {
    position: fixed;
    bottom: var(--space-5);
    right: var(--space-5);
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    max-width: 28rem;
    padding: var(--space-3) var(--space-4);
    background: var(--color-panel);
    color: var(--color-err);
    border: 1px solid var(--color-err-line);
    border-left: 3px solid var(--color-err);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-pop);
    font-size: var(--text-sm);
    line-height: 1.5;
  }
</style>
