<script lang="ts">
  import { updateDoc } from "../doc.svelte";
  import type { Api, Auth, Body, Endpoint, Method } from "../model";
  import AuthEditor from "./AuthEditor.svelte";
  import BodyEditor from "./BodyEditor.svelte";
  import { setEndpointField } from "./fieldOrder";
  import KeyValueRows from "./KeyValueRows.svelte";

  // `api` is the already-parsed document `RequestPanel` computed — passed
  // in rather than re-derived here via `currentDoc()`. `currentDoc()`
  // requires the selection to still be "live" (`selectedApi()`), which is
  // NOT true for a just-removed-but-still-displayed endpoint
  // (`ui.removedSelected`); `RequestPanel`'s own `parsedDoc` is built from
  // its removed-tolerant `api`/`bufferText` fallbacks instead, so it stays
  // renderable in that case (mirroring what the JSON tab's `Editor` already
  // does). Re-deriving via `currentDoc()` here would silently go
  // `undefined` in exactly that case and dead-end the form on "not
  // available" — Form is the default tab, so that dead end would be the
  // first and only thing such a user sees.
  let { api, endpointId }: { api: Api; endpointId: string } = $props();

  const METHODS: Method[] = [
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
  ];

  const endpoint = $derived<Endpoint | undefined>(
    api.endpoints.find((e) => e.id === endpointId),
  );

  /** Runs `fn` against the live endpoint inside the document `updateDoc`
   * gives us — never against the stale `endpoint` snapshot read above,
   * which could be one buffer-generation behind by the time a mutator
   * runs. */
  function mutateEndpoint(fn: (ep: Endpoint) => void): void {
    updateDoc((api) => {
      const ep = api.endpoints.find((e) => e.id === endpointId);
      if (!ep) return;
      fn(ep);
    });
  }

  function onMethodChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as Method;
    mutateEndpoint((ep) => {
      ep.method = value;
    });
  }

  function onPathChange(e: Event): void {
    const value = (e.currentTarget as HTMLInputElement).value;
    mutateEndpoint((ep) => {
      ep.path = value;
    });
  }

  function onHeadersChange(headers: Record<string, string>): void {
    mutateEndpoint((ep) => {
      setEndpointField(ep, "headers", headers);
    });
  }

  function onQueryChange(query: Record<string, string>): void {
    mutateEndpoint((ep) => {
      setEndpointField(ep, "query", query);
    });
  }

  function onBodyChange(body: Body | undefined): void {
    mutateEndpoint((ep) => {
      if (body === undefined) {
        delete ep.body;
      } else {
        ep.body = body;
      }
    });
  }

  function onAuthChange(auth: Auth): void {
    mutateEndpoint((ep) => {
      setEndpointField(ep, "auth", auth);
    });
  }
</script>

{#if !endpoint}
  <p class="placeholder">Endpoint not available in the current document.</p>
{:else}
  <div class="endpoint-form">
    <div class="field-row">
      <label for="endpoint-method">Method</label>
      <select
        id="endpoint-method"
        class="method-select"
        value={endpoint.method}
        onchange={onMethodChange}
      >
        {#each METHODS as m (m)}
          <option value={m}>{m}</option>
        {/each}
      </select>
    </div>

    <div class="field-row">
      <label for="endpoint-path">Path</label>
      <input
        id="endpoint-path"
        class="path-input"
        type="text"
        value={endpoint.path}
        oninput={onPathChange}
      />
    </div>

    <section class="fields-section">
      <h3>Headers</h3>
      {#key endpointId}
        <!-- Same reasoning as `AuthEditor`/`BodyEditor` below: `KeyValueRows`
             keeps local `$state` (its own ordered `Row[]`, including
             `hadKey` identity tracking) scoped to whatever record it was
             handed. Its own echo-vs-external check compares CONTENT only,
             with no endpoint identity in it — two endpoints whose headers
             happen to serialize identically (e.g. two untouched `{}`
             endpoints) would not look like a change to it, so switching
             endpoints without this `{#key}` could leave endpoint A's
             half-typed row mounted over endpoint B's editor, and the next
             keystroke would commit it onto B through `onHeadersChange`'s
             closed-over `endpointId`. Keying on `endpointId` forces a
             remount on every endpoint switch, independent of whether the
             two records collide. -->
        <KeyValueRows
          rows={endpoint.headers}
          onChange={onHeadersChange}
          keyLabel="Header"
          valueLabel="Value"
        />
      {/key}
    </section>

    <section class="fields-section">
      <h3>Query</h3>
      {#key endpointId}
        <!-- Same reasoning as Headers above. -->
        <KeyValueRows
          rows={endpoint.query}
          onChange={onQueryChange}
          keyLabel="Param"
          valueLabel="Value"
        />
      {/key}
    </section>

    <section class="fields-section">
      <h3>Auth</h3>
      {#key endpointId}
        <!-- Same reasoning as `BodyEditor`'s `{#key}` below: nothing inside
             `AuthEditor`/`ChainedAuthBuilder` currently holds local
             `$state` (every field reads straight from `auth` and writes
             straight through `onChange`), but `KeyValueRows` — used for the
             `header` auth type — does, and two endpoints whose auth
             happens to serialize identically (e.g. two untouched
             `{"type":"inherit"}` endpoints) would not look like a change
             to its own echo-vs-external check. Keying on `endpointId`
             forces a remount on every endpoint switch regardless of
             content collisions. -->
        <AuthEditor
          auth={endpoint.auth}
          onChange={onAuthChange}
          {api}
          endpointId={endpoint.id}
          allowInherit={true}
        />
      {/key}
    </section>

    <section class="fields-section">
      <h3>Body</h3>
      {#key endpointId}
        <!-- `BodyEditor` keeps local state (a pending unconfirmed type
             switch, an uncommitted invalid-JSON draft) that is scoped to
             ONE endpoint. Its own internal echo-vs-external check
             (`bodyKey`) compares body CONTENT only, with no endpoint
             identity in it — two endpoints whose bodies happen to
             serialize identically (e.g. both the untouched default
             `{"type":"json","content":""}`) would not look like a change
             to it, so switching endpoints without this `{#key}` could
             leave endpoint A's pending confirmation or draft text mounted
             over endpoint B's editor, and confirming/finishing it would
             mutate B through `onBodyChange`'s closed-over `endpointId`.
             Keying on `endpointId` forces Svelte to destroy and recreate
             the component on every endpoint switch, so no such state can
             ever survive one — independent of whether the two bodies
             collide. -->
        <BodyEditor body={endpoint.body} onChange={onBodyChange} />
      {/key}
    </section>
  </div>
{/if}

<style>
  .placeholder {
    padding: var(--space-5);
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  /* One gutter for the whole centre column, set here rather than on every
     row, and the same one the URL bar above uses — so a label, a field and
     the method chip overhead all line up on the same left edge. */
  .endpoint-form {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    overflow-y: auto;
    min-height: 0;
    padding: var(--space-5);
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }

  .field-row label {
    flex-shrink: 0;
    width: var(--label-col);
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  .method-select,
  .path-input {
    flex: 1;
    min-width: 0;
    height: var(--control-height);
    padding: 0 var(--space-4);
    font-size: var(--text-base);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background-color: var(--color-field);
    color: var(--color-text);
  }

  .method-select:focus,
  .path-input:focus {
    outline: none;
    border-color: var(--color-accent);
  }

  .path-input {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .fields-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  /* The section name and its hairline read as one object: the rule runs to
     the right edge of the column, so the eye can tell where one group of
     fields ends and the next begins without counting gaps. */
  .fields-section h3 {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin: 0;
    font-family: var(--font-condensed);
    font-weight: 600;
    font-size: 0.625rem;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--color-text-faint);
  }

  .fields-section h3::after {
    content: "";
    flex-grow: 1;
    height: 1px;
    background: var(--color-border-soft);
  }
</style>
