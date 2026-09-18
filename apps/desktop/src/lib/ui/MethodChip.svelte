<script lang="ts">
  /**
   * The HTTP method, as one chip. Every list, summary line and trace step
   * renders the method through this, so a method reads the same everywhere.
   *
   * A method the app doesn't have a color for (a server-specific verb, or
   * junk typed into the JSON) falls back to the neutral pair rather than
   * rendering an uncolored, differently-sized chip.
   */
  const KNOWN = ["get", "post", "put", "patch", "delete", "head", "options"];

  interface Props {
    method: string;
    /** "sm" in dense lists, "md" on the request's own URL bar. */
    size?: "sm" | "md";
  }

  const { method, size = "sm" }: Props = $props();

  const key = $derived(method.toLowerCase());
  const known = $derived(KNOWN.includes(key));
  // DELETE is the one verb too wide for the chip at this size; it is also
  // the one whose first three letters are unambiguous.
  const label = $derived(key === "delete" ? "DEL" : method.toUpperCase());
</script>

<span
  class="chip chip-{size}"
  class:known
  style={known ? `--chip-fg: var(--color-method-${key}); --chip-bg: var(--color-method-${key}-tint);` : ""}
  title={method.toUpperCase()}
>
  {label}
</span>

<style>
  .chip {
    flex-shrink: 0;
    text-align: center;
    font-family: var(--font-condensed);
    font-weight: 600;
    letter-spacing: 0.05em;
    border-radius: var(--radius-sm);
    color: var(--color-method-other);
    background: var(--color-method-other-tint);
  }

  .known {
    color: var(--chip-fg);
    background: var(--chip-bg);
  }

  .chip-sm {
    width: 2.875rem;
    font-size: 0.594rem;
    padding: 0.188rem 0;
  }

  .chip-md {
    width: 3.625rem;
    font-size: 0.688rem;
    letter-spacing: 0.06em;
    padding: 0.5rem 0;
    border-radius: var(--radius-md);
  }
</style>
