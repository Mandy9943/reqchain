<script lang="ts">
  import { untrack } from "svelte";
  import { basicSetup, EditorView } from "codemirror";
  import { json } from "@codemirror/lang-json";
  import type { DiagnosticDto } from "./ipc";

  let {
    value,
    onChange,
    diagnostics = [],
  }: {
    value: string;
    onChange: (v: string) => void;
    diagnostics?: DiagnosticDto[];
  } = $props();

  let container: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  // Text last reported to (or accepted from) the parent, so the "external
  // value changed" effect below can tell a hot-reload/buffer-swap apart from
  // the echo of our own keystrokes. Set once the view is created (see
  // below) — read untracked there so it isn't just a snapshot of the value
  // at module init.
  let lastKnown: string;

  const hasError = $derived(diagnostics.some((d) => d.severity === "error"));

  // Create the view once. `value` is read untracked so later edits to the
  // `value` prop (including the echo of our own onChange calls) don't tear
  // down and rebuild the editor.
  $effect(() => {
    if (!container) return;

    const initialDoc = untrack(() => value);
    lastKnown = initialDoc;
    view = new EditorView({
      doc: initialDoc,
      extensions: [
        basicSetup,
        json(),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const text = update.state.doc.toString();
            lastKnown = text;
            onChange(text);
          }
        }),
      ],
      parent: container,
    });

    return () => {
      view?.destroy();
      view = undefined;
    };
  });

  // Replace the document when `value` changes from outside (hot reload,
  // switching to a different API's buffer) — but not when the change is
  // just the echo of an edit this editor itself just produced.
  $effect(() => {
    const external = value;
    if (view && external !== lastKnown) {
      lastKnown = external;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: external },
        selection: { anchor: 0 },
      });
    }
  });
</script>

<div class="editor" class:has-error={hasError} bind:this={container}></div>

<style>
  .editor {
    height: 100%;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    font-size: 0.85rem;
  }

  .editor.has-error {
    border-color: var(--color-error-text);
  }

  .editor :global(.cm-editor) {
    height: 100%;
  }

  .editor :global(.cm-scroller) {
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }
</style>
