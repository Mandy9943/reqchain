<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import RequestPanel from "./lib/RequestPanel.svelte";
  import ResponsePanel from "./lib/ResponsePanel.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import { reload, ui } from "./lib/state.svelte";

  let requestPanelRef: RequestPanel | undefined = $state();
  let responsePanelRef: ResponsePanel | undefined = $state();

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
  <Sidebar />

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
