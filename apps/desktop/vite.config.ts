import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // This is a single-page desktop app bundle (no route-based code
  // splitting to do), so the default 500 kB advisory limit just produces
  // noise as legitimate features are added — raised to give some headroom
  // rather than have "zero warnings" get gamed by artificially splitting
  // unrelated code.
  build: { chunkSizeWarningLimit: 700 },
  // Every existing test file is plain TS/pure-logic and runs fine under
  // Vitest's default "node" environment (no DOM). A handful of tests
  // (*.dom.test.ts) mount actual `.svelte` components to test cross-
  // component remount discipline (see EndpointForm.dom.test.ts) — those
  // need a real `document`/`window`, hence jsdom, and Svelte 5's own
  // client runtime needs the "browser" package export condition to
  // resolve (its default/node condition points at server-only exports
  // that don't support `mount`/`unmount`/effects at all). Scoped to only
  // Vitest's own resolution (not the `vite build`/`tauri dev` compile
  // path) via the `VITEST` env var Vitest itself sets, so production
  // bundling is completely unaffected by this.
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  test: {
    environment: "node",
    environmentMatchGlobs: [["**/*.dom.test.ts", "jsdom"]],
  },
});
