<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import RequestPanel from "./lib/RequestPanel.svelte";
  import ResponsePanel from "./lib/ResponsePanel.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import SecretsScreen from "./lib/SecretsScreen.svelte";
  import { reload, ui } from "./lib/state.svelte";

  let requestPanelRef: RequestPanel | undefined = $state();
  let responsePanelRef: ResponsePanel | undefined = $state();
  // Owns whether the secrets screen is shown. Mounting/unmounting the
  // component (rather than just hiding it) is what makes "leaving the screen
  // re-masks" hold even if a bug skipped the toggle's own re-mask path —
  // `SecretsScreen`'s `onDestroy` runs whenever this flips back to false.
  let showSecrets = $state(false);

  onMount(() => {
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
      if (responsePanelRef?.sendCurrent()) {
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
</script>

<main class="layout">
  <Sidebar onOpenSecrets={() => (showSecrets = true)} />

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
          <ResponsePanel bind:this={responsePanelRef} />
        </div>
        <HistoryPanel />
      </div>
    </section>
  {/if}
</main>

{#if ui.error}
  <div class="global-error">{ui.error}</div>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: 280px 1fr 1fr;
    height: 100vh;
    min-height: 0;
  }

  .panel {
    min-width: 0;
    min-height: 0;
    overflow: auto;
    padding: 0.75rem;
  }

  .response-stack {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 0.5rem;
  }

  .response-main {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .request-panel {
    border-right: 1px solid var(--color-border);
  }

  .secrets-panel {
    grid-column: 2 / 4;
    padding: 0;
  }

  .global-error {
    position: fixed;
    bottom: 0.75rem;
    right: 0.75rem;
    max-width: 28rem;
    padding: 0.5rem 0.75rem;
    background: var(--color-error-bg);
    color: var(--color-error-text);
    border: 1px solid var(--color-error-text);
    border-radius: 4px;
    font-size: 0.85rem;
  }
</style>
