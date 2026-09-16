<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import { reload, ui } from "./lib/state.svelte";

  onMount(() => {
    void reload();
  });
</script>

<main class="layout">
  <Sidebar />

  <section class="panel request-panel">
    <!-- Task 8 fills this in: request editor for the selected endpoint. -->
    <p class="placeholder">Request panel (task 8)</p>
  </section>

  <section class="panel response-panel">
    <!-- Task 9 fills this in: response viewer. -->
    <p class="placeholder">Response panel (task 9)</p>
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
    overflow: auto;
    padding: 0.75rem;
  }

  .request-panel {
    border-right: 1px solid var(--color-border);
  }

  .placeholder {
    color: var(--color-text-muted);
    font-style: italic;
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
