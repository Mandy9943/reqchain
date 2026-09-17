import { describe, expect, it, vi } from "vitest";

// `state.svelte.ts` registers a `listen(WORKSPACE_CHANGED_EVENT, ...)`
// side effect at module load, which reaches `window.__TAURI_INTERNALS__` —
// absent in vitest's default (node, no DOM) environment. Rather than pull
// in jsdom/happy-dom just to satisfy that side effect (this project has no
// DOM test dependency, and canWriteApiDoc itself touches no DOM at all), a
// minimal global stub of exactly the two members that side effect calls
// lets the module load cleanly, and a dynamic import (not a static one)
// delays evaluation until after the stub is installed — a static import
// would run before this test body, since ES module imports are hoisted.
vi.stubGlobal("window", {
  __TAURI_INTERNALS__: {
    invoke: async () => 0,
    transformCallback: () => 0,
  },
});

describe("canWriteApiDoc", () => {
  it("is true for an api with no save or delete in flight", async () => {
    const { canWriteApiDoc } = await import("./state.svelte");
    expect(canWriteApiDoc("untouched-api")).toBe(true);
  });

  it("is false while a save is in flight for that api", async () => {
    const { ui, canWriteApiDoc } = await import("./state.svelte");
    ui.savingIds["demo"] = true;
    expect(canWriteApiDoc("demo")).toBe(false);
    delete ui.savingIds["demo"];
    expect(canWriteApiDoc("demo")).toBe(true);
  });

  it("is false while the whole api is being deleted", async () => {
    const { ui, canWriteApiDoc } = await import("./state.svelte");
    ui.deletingIds["demo"] = true;
    expect(canWriteApiDoc("demo")).toBe(false);
    delete ui.deletingIds["demo"];
    expect(canWriteApiDoc("demo")).toBe(true);
  });

  it("does not confuse one api id's in-flight state with another's", async () => {
    const { ui, canWriteApiDoc } = await import("./state.svelte");
    ui.savingIds["a"] = true;
    expect(canWriteApiDoc("a")).toBe(false);
    expect(canWriteApiDoc("b")).toBe(true);
    delete ui.savingIds["a"];
  });
});
