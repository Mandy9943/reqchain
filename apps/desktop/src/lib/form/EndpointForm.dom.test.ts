// @vitest-environment jsdom
//
// Component-level regression tests for the remount discipline this branch
// relies on: `EndpointForm.svelte` wraps its `KeyValueRows`/`AuthEditor`/
// `BodyEditor` mounts in `{#key endpointId}` specifically because those
// children keep local `$state` scoped to whatever endpoint they were last
// handed, and their own echo-vs-external checks compare CONTENT, not
// endpoint identity. A pure-module unit test (keyValueRows.test.ts,
// bodyEditor.test.ts, authEditor.test.ts) structurally cannot see this —
// the bug lives in whether SVELTE remounts the component, which only a
// real render can exercise. Kept deliberately narrow, per the coordinator's
// instruction: this is not broad component coverage, just the specific
// blind spot (switch endpoints mid-edit; the edit must land on the
// endpoint it was typed into, and nowhere else).
import { describe, expect, it } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { emptyApi, emptyEndpoint, parseApi, serializeApi, type Api } from "../model";
import { reactiveProps } from "./testSupport.svelte";

// `EndpointForm` mutates through `updateDoc` (doc.svelte.ts), which reads
// the global `ui` state (state.svelte.ts). That module registers a
// `listen(...)` side effect at load time reaching `window.__TAURI_INTERNALS__`.
// jsdom provides a real `window`, so (unlike state.test.ts's node-environment
// stub, which replaces `window` wholesale) this only adds the two members
// that side effect actually touches — set before the dynamic imports below
// pull state.svelte.ts in, so nothing observes it missing.
(
  window as unknown as {
    __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<unknown>; transformCallback: () => number };
  }
).__TAURI_INTERNALS__ = {
  invoke: async (cmd: string) => (cmd === "plugin:event|listen" ? 1 : null),
  transformCallback: () => 0,
};

function buildApi(headersA: Record<string, string>, headersB: Record<string, string>): Api {
  const a = { ...emptyEndpoint("a", "Endpoint A"), headers: headersA };
  const b = { ...emptyEndpoint("b", "Endpoint B"), headers: headersB };
  return { ...emptyApi("gw", "Gateway"), baseUrl: "https://x", endpoints: [a, b] };
}

/** Wires the global `ui` store (state.svelte.ts) so `updateDoc`/`currentDoc`
 * resolve against a single in-memory API document, the way `RequestPanel`
 * would set it up for a real selection. Returns the buffer accessor so
 * tests can inspect what actually landed after each interaction. */
async function seedWorkspace(api: Api) {
  const { ui } = await import("../state.svelte");
  const text = serializeApi(api);
  ui.workspace = {
    apis: [
      {
        id: api.id,
        name: api.name,
        baseUrl: api.baseUrl,
        environments: [],
        endpoints: api.endpoints.map((e) => ({
          id: e.id,
          name: e.name,
          method: e.method,
          path: e.path,
          authKind: e.auth.type,
        })),
        text,
        path: `/tmp/${api.id}.json`,
      },
    ],
    errors: [],
  };
  ui.selected = { apiId: api.id, endpointId: api.endpoints[0].id };
  ui.buffers = { [api.id]: text };
  return {
    currentHeaders(endpointId: string): Record<string, string> {
      const parsed = parseApi(ui.buffers[api.id]);
      if (!parsed.ok) throw new Error(`buffer no longer parses: ${parsed.error}`);
      const ep = parsed.api.endpoints.find((e) => e.id === endpointId);
      if (!ep) throw new Error(`endpoint ${endpointId} missing from buffer`);
      return ep.headers;
    },
  };
}

function dispatchInput(el: HTMLInputElement, value: string): void {
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}

describe("EndpointForm remount discipline (headers)", () => {
  it("does not leak a stale duplicate-key error from one endpoint onto another with coincidentally-identical headers", async () => {
    // Both endpoints start with EXACTLY the same headers — the one
    // condition under which `KeyValueRows`' own content-only echo check
    // cannot tell "a different endpoint" apart from "an echo of my own
    // last edit" on its own, and the only situation where `{#key
    // endpointId}` (rather than KeyValueRows' internal logic) is what
    // actually prevents the bleed.
    const api = buildApi({ Accept: "json" }, { Accept: "json" });
    const doc = await seedWorkspace(api);
    const { default: EndpointForm } = await import("./EndpointForm.svelte");

    const target = document.createElement("div");
    document.body.appendChild(target);
    const props = reactiveProps<{ api: Api; endpointId: string }>({ api, endpointId: "a" });
    const instance = mount(EndpointForm, { target, props });
    flushSync();

    // On endpoint A: type "Accept" into the blank row, colliding with the
    // existing "Accept" row — refused (never committed), leaving A's
    // `KeyValueRows` instance holding an uncommitted duplicate-key error.
    const headerKeyInputs = () =>
      Array.from(target.querySelectorAll<HTMLInputElement>(".fields-section"))[0]!
        .querySelectorAll<HTMLInputElement>(".row-key");
    let keyInputs = headerKeyInputs();
    expect(keyInputs.length).toBe(2); // "Accept" row + the trailing blank
    dispatchInput(keyInputs[1]!, "Accept");
    flushSync();
    expect(target.textContent).toMatch(/Duplicate/);
    // The buffer is untouched by the refused edit — still just the
    // original single "Accept" header on A.
    expect(doc.currentHeaders("a")).toEqual({ Accept: "json" });

    // Switch to endpoint B — same props object, same component instance,
    // exactly what `RequestPanel` does when the sidebar selection changes.
    // Endpoint B's real headers are IDENTICAL to what `KeyValueRows` last
    // successfully committed for A (`{ Accept: "json" }`), so if
    // `{#key endpointId}` were removed, KeyValueRows' own echo-vs-external
    // check would see no difference and would NOT resync — leaving A's
    // stray duplicate "Accept" row rendered as if it belonged to B.
    props.endpointId = "b";
    flushSync();

    // With the `{#key endpointId}` fix in place, the component fully
    // remounted: no duplicate-error banner, and exactly one "Accept" row
    // (B's real header) plus a fresh trailing blank — not the stale,
    // still-colliding two-"Accept"-rows state from A.
    expect(target.textContent).not.toMatch(/Duplicate/);
    keyInputs = headerKeyInputs();
    expect(keyInputs.length).toBe(2);
    expect(keyInputs[0]!.value).toBe("Accept");
    expect(keyInputs[1]!.value).toBe("");

    // Finish "fixing" what would have been the leaked duplicate, using B's
    // fresh blank row this time.
    dispatchInput(keyInputs[1]!, "X-Only-On-B");
    flushSync();

    // B gets exactly the header actually typed into it — nothing bled over
    // from A's earlier, refused edit.
    expect(doc.currentHeaders("b")).toEqual({ Accept: "json", "X-Only-On-B": "" });
    // A is completely unaffected by anything that happened on B.
    expect(doc.currentHeaders("a")).toEqual({ Accept: "json" });

    unmount(instance);
    target.remove();
  });

  it("shows and commits into the newly-selected endpoint's own headers immediately after a switch, not the previous endpoint's", async () => {
    const api = buildApi({ Accept: "json" }, {});
    const doc = await seedWorkspace(api);
    const { default: EndpointForm } = await import("./EndpointForm.svelte");

    const target = document.createElement("div");
    document.body.appendChild(target);
    const props = reactiveProps<{ api: Api; endpointId: string }>({ api, endpointId: "a" });
    const instance = mount(EndpointForm, { target, props });
    flushSync();

    props.endpointId = "b";
    flushSync();

    const headerKeyInputs = target
      .querySelectorAll<HTMLInputElement>(".fields-section")[0]!
      .querySelectorAll<HTMLInputElement>(".row-key");
    // B started with NO headers at all — a fresh remount shows only the
    // permanent blank row, not A's "Accept".
    expect(headerKeyInputs.length).toBe(1);
    expect(headerKeyInputs[0]!.value).toBe("");

    dispatchInput(headerKeyInputs[0]!, "X-B");
    flushSync();

    expect(doc.currentHeaders("b")).toEqual({ "X-B": "" });
    expect(doc.currentHeaders("a")).toEqual({ Accept: "json" });

    unmount(instance);
    target.remove();
  });
});
