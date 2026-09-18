<script lang="ts">
  /**
   * The app's whole icon set, as one inline-SVG component.
   *
   * Inline (not an image or a font) so every glyph takes its color from
   * `currentColor` and stays legible in both themes, and so nothing has to
   * be fetched — `tauri.conf.json` ships a `default-src 'self'` CSP.
   *
   * All paths are drawn on a 24×24 box with a 2.2 stroke, which is what
   * keeps them looking like one family at the 11–15px sizes used here.
   */
  export type IconName =
    | "chain"
    | "search"
    | "chevron-down"
    | "chevron-right"
    | "plus"
    | "close"
    | "refresh"
    | "send"
    | "copy"
    | "clock"
    | "warning"
    | "info"
    | "key"
    | "system"
    | "sun"
    | "moon"
    | "arrow-right"
    | "trash"
    | "up"
    | "down";

  interface Props {
    name: IconName;
    size?: number;
    /** Decorative by default; pass a label when the icon is the only content. */
    label?: string;
  }

  const { name, size = 13, label }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2.2"
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? "img" : "presentation"}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
  style="flex-shrink: 0; display: block;"
>
  {#if name === "chain"}
    <path d="M10.5 13.5a4.6 4.6 0 0 0 6.9.5l2.6-2.6a4.6 4.6 0 0 0-6.5-6.5l-1.5 1.5" />
    <path d="M13.5 10.5a4.6 4.6 0 0 0-6.9-.5L4 12.6a4.6 4.6 0 0 0 6.5 6.5l1.5-1.5" />
  {:else if name === "search"}
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.6-3.6" />
  {:else if name === "chevron-down"}
    <path d="m6 9 6 6 6-6" />
  {:else if name === "chevron-right"}
    <path d="m9 6 6 6-6 6" />
  {:else if name === "plus"}
    <path d="M12 5v14M5 12h14" />
  {:else if name === "close"}
    <path d="M6 6l12 12M18 6 6 18" />
  {:else if name === "refresh"}
    <path d="M20 11A8 8 0 0 0 6.3 6.3L3 9.5" />
    <path d="M3 4.5v5h5" />
    <path d="M4 13a8 8 0 0 0 13.7 4.7L21 14.5" />
    <path d="M21 19.5v-5h-5" />
  {:else if name === "send"}
    <path d="M6 4.5 19 12 6 19.5z" fill="currentColor" stroke-width="1.5" />
  {:else if name === "copy"}
    <rect x="9" y="9" width="11" height="11" rx="2.5" stroke-width="2" />
    <path d="M5 15V6a2 2 0 0 1 2-2h8" stroke-width="2" />
  {:else if name === "clock"}
    <circle cx="12" cy="12" r="8.5" />
    <path d="M12 7.5V12l3 2" />
  {:else if name === "warning"}
    <path d="M12 4.5 21 19.5H3z" />
    <path d="M12 10v4" />
    <path d="M12 17h.01" />
  {:else if name === "info"}
    <circle cx="12" cy="12" r="9" stroke-width="2" />
    <path d="M12 11v5.5" stroke-width="2" />
    <path d="M12 7.6h.01" stroke-width="2" />
  {:else if name === "key"}
    <circle cx="8" cy="15" r="4" />
    <path d="m11 12 8-8" />
    <path d="m17 6 2.5 2.5" />
    <path d="m14.5 8.5 2.5 2.5" />
  {:else if name === "system"}
    <rect x="3" y="4" width="18" height="12" rx="2" stroke-width="2" />
    <path d="M9 20h6" stroke-width="2" />
  {:else if name === "sun"}
    <circle cx="12" cy="12" r="4" stroke-width="2" />
    <path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6 7 7M17 17l1.4 1.4M18.4 5.6 17 7M7 17l-1.4 1.4" stroke-width="2" />
  {:else if name === "moon"}
    <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z" stroke-width="2" />
  {:else if name === "arrow-right"}
    <path d="M4 12h15" />
    <path d="m13 6 6 6-6 6" />
  {:else if name === "trash"}
    <path d="M4 7h16" />
    <path d="M9 7V5h6v2" />
    <path d="M6.5 7 7.5 20h9L17.5 7" />
  {:else if name === "up"}
    <path d="m6 15 6-6 6 6" />
  {:else if name === "down"}
    <path d="m6 9 6 6 6-6" />
  {/if}
</svg>
