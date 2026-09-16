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
});
