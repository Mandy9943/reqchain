<script lang="ts">
  import type { Api, Auth } from "../model";
  import {
    asChained,
    asRetryOn,
    buildExtract,
    buildInject,
    buildTtl,
    DEFAULT_EXTRACT_JSON_PATH,
    DEFAULT_TTL_JSON_PATH,
    extractChoiceOf,
    injectChoiceOf,
    RETRY_STATUS_CHOICES,
    toggleRetryStatus,
    ttlChoiceOf,
    validSourceEndpoints,
    type ExtractChoice,
    type InjectChoice,
    type TtlChoice,
  } from "./chainedAuthBuilder";

  let {
    auth,
    onChange,
    api,
    endpointId,
  }: {
    auth: Auth;
    onChange: (a: Auth) => void;
    api: Api;
    endpointId: string | null;
  } = $props();

  // Defensive coercion, not just a display convenience: a JSON-tab edit can
  // hand `AuthEditor` a `type: "chained"` value missing `source`/`inject` —
  // `parseApi`'s `validateAuthShape` should make that unreachable, but this
  // is the second, independent layer (see chainedAuthBuilder.ts's header
  // comment).
  const chained = $derived(asChained(auth));

  // The ONLY endpoints question 1 may offer — self-reference and any cycle
  // are already excluded here, never merely reported after the fact.
  const sources = $derived(validSourceEndpoints(api, endpointId));

  const extractChoice = $derived(extractChoiceOf(chained.extract));
  const ttlChoice = $derived(ttlChoiceOf(chained.ttl));
  const injectChoice = $derived(injectChoiceOf(chained.inject));
  const retryOn = $derived(asRetryOn(chained.retryOn));

  function onSourceChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value;
    onChange({ ...chained, source: { endpoint: value } });
  }

  // --- extract ---

  function onExtractChoiceChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as ExtractChoice;
    onChange({ ...chained, extract: buildExtract(value) });
  }

  function setExtractJsonPath(e: Event): void {
    if (chained.extract?.from !== "body") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, extract: { from: "body", jsonPath: value } });
  }

  function setExtractBodyRegex(e: Event): void {
    if (chained.extract?.from !== "body") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, extract: { from: "body", regex: value } });
  }

  function setExtractHeaderName(e: Event): void {
    if (chained.extract?.from !== "header") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({
      ...chained,
      extract: { from: "header", name: value, regex: chained.extract.regex },
    });
  }

  function setExtractHeaderRegex(e: Event): void {
    if (chained.extract?.from !== "header") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({
      ...chained,
      extract: {
        from: "header",
        name: chained.extract.name,
        regex: value === "" ? undefined : value,
      },
    });
  }

  // --- ttl ---

  function onTtlChoiceChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as TtlChoice;
    onChange({ ...chained, ttl: buildTtl(value) });
  }

  function setTtlBodyJsonPath(e: Event): void {
    if (chained.ttl?.from !== "body") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, ttl: { from: "body", jsonPath: value, unit: chained.ttl.unit } });
  }

  function setTtlUnit(e: Event): void {
    if (chained.ttl?.from !== "body") return;
    const value = (e.currentTarget as HTMLSelectElement).value as "seconds" | "milliseconds";
    onChange({ ...chained, ttl: { from: "body", jsonPath: chained.ttl.jsonPath, unit: value } });
  }

  function setTtlFixedSeconds(e: Event): void {
    if (chained.ttl?.from !== "fixed") return;
    const raw = Number((e.currentTarget as HTMLInputElement).value);
    const seconds = Number.isFinite(raw) ? Math.max(0, Math.trunc(raw)) : 0;
    onChange({ ...chained, ttl: { from: "fixed", seconds } });
  }

  function setTtlAbsoluteJsonPath(e: Event): void {
    if (chained.ttl?.from !== "absolute") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, ttl: { from: "absolute", jsonPath: value } });
  }

  // --- inject ---

  function onInjectChoiceChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as InjectChoice;
    onChange({ ...chained, inject: buildInject(value) });
  }

  function setInjectHeaderName(e: Event): void {
    if (chained.inject.into !== "header") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, inject: { into: "header", name: value, template: chained.inject.template } });
  }

  function setInjectHeaderTemplate(e: Event): void {
    if (chained.inject.into !== "header") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, inject: { into: "header", name: chained.inject.name, template: value } });
  }

  function setInjectQueryName(e: Event): void {
    if (chained.inject.into !== "query") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, inject: { into: "query", name: value, template: chained.inject.template } });
  }

  function setInjectQueryTemplate(e: Event): void {
    if (chained.inject.into !== "query") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, inject: { into: "query", name: chained.inject.name, template: value } });
  }

  function setInjectBodyPointer(e: Event): void {
    if (chained.inject.into !== "body") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, inject: { into: "body", pointer: value, template: chained.inject.template } });
  }

  function setInjectBodyTemplate(e: Event): void {
    if (chained.inject.into !== "body") return;
    const value = (e.currentTarget as HTMLInputElement).value;
    onChange({ ...chained, inject: { into: "body", pointer: chained.inject.pointer, template: value } });
  }

  // --- retryOn ---

  function onRetryToggle(status: number): void {
    onChange({ ...chained, retryOn: toggleRetryStatus(chained.retryOn, status) });
  }
</script>

<div class="chained-builder">
  <fieldset class="question">
    <legend>1. Which endpoint provides the token?</legend>
    <select value={chained.source.endpoint} onchange={onSourceChange}>
      <option value="" disabled>Select an endpoint…</option>
      {#each sources as ep (ep.id)}
        <option value={ep.id}>{ep.name}</option>
      {/each}
    </select>
    {#if sources.length === 0}
      <p class="hint">
        No other endpoint in this API can be used here — every one would
        either be this endpoint itself or would form a cycle back to it.
      </p>
    {/if}
  </fieldset>

  <fieldset class="question">
    <legend>2. Where in the response is the value?</legend>
    <select value={extractChoice} onchange={onExtractChoiceChange}>
      <option value="default">
        Documented default — body, JSONPath {DEFAULT_EXTRACT_JSON_PATH}
      </option>
      <option value="body-jsonpath">Body — JSONPath</option>
      <option value="body-regex">Body — Regex</option>
      <option value="header">Response header</option>
      <option value="status">Status code</option>
    </select>

    {#if extractChoice === "default"}
      <p class="hint">
        Prefilled default: <code>{DEFAULT_EXTRACT_JSON_PATH}</code> in the
        response body. Leaving <code>extract</code> out of the file this way
        makes <code>reqchain validate</code> emit a warning — pick
        "Body — JSONPath" and keep the same path below to silence it while
        keeping identical behavior.
      </p>
    {:else if chained.extract?.from === "body" && chained.extract.regex === undefined}
      <div class="field-row">
        <label for="extract-jsonpath">JSONPath</label>
        <input
          id="extract-jsonpath"
          type="text"
          class="mono"
          value={chained.extract.jsonPath ?? ""}
          oninput={setExtractJsonPath}
        />
      </div>
    {:else if chained.extract?.from === "body"}
      <div class="field-row">
        <label for="extract-regex">Regex</label>
        <input
          id="extract-regex"
          type="text"
          class="mono"
          value={chained.extract.regex ?? ""}
          oninput={setExtractBodyRegex}
        />
      </div>
      <p class="hint">Capture group 1 is the token, else the whole match.</p>
    {:else if chained.extract?.from === "header"}
      <div class="field-row">
        <label for="extract-header-name">Header name</label>
        <input
          id="extract-header-name"
          type="text"
          value={chained.extract.name}
          oninput={setExtractHeaderName}
        />
      </div>
      <div class="field-row">
        <label for="extract-header-regex">Regex (optional)</label>
        <input
          id="extract-header-regex"
          type="text"
          class="mono"
          value={chained.extract.regex ?? ""}
          oninput={setExtractHeaderRegex}
        />
      </div>
    {:else if chained.extract?.from === "status"}
      <p class="hint">The response's HTTP status, as a string, is the token.</p>
    {/if}
  </fieldset>

  <fieldset class="question">
    <legend>3. How long is it good for?</legend>
    <select value={ttlChoice} onchange={onTtlChoiceChange}>
      <option value="default">
        Documented default — body, JSONPath {DEFAULT_TTL_JSON_PATH} (seconds)
      </option>
      <option value="body">Body field</option>
      <option value="fixed">Fixed number of seconds</option>
      <option value="absolute">Absolute date field</option>
    </select>

    {#if ttlChoice === "default"}
      <p class="hint">
        Prefilled default: <code>{DEFAULT_TTL_JSON_PATH}</code> seconds, in
        the response body. Leaving <code>ttl</code> out of the file this way
        makes <code>reqchain validate</code> emit a warning — pick "Body
        field" and keep the same path below to silence it while keeping
        identical behavior.
      </p>
    {:else if chained.ttl?.from === "body"}
      <div class="field-row">
        <label for="ttl-jsonpath">JSONPath</label>
        <input
          id="ttl-jsonpath"
          type="text"
          class="mono"
          value={chained.ttl.jsonPath}
          oninput={setTtlBodyJsonPath}
        />
      </div>
      <div class="field-row">
        <label for="ttl-unit">Unit</label>
        <select id="ttl-unit" value={chained.ttl.unit} onchange={setTtlUnit}>
          <option value="seconds">Seconds</option>
          <option value="milliseconds">Milliseconds</option>
        </select>
      </div>
    {:else if chained.ttl?.from === "fixed"}
      <div class="field-row">
        <label for="ttl-fixed-seconds">Seconds</label>
        <input
          id="ttl-fixed-seconds"
          type="number"
          min="0"
          value={chained.ttl.seconds}
          oninput={setTtlFixedSeconds}
        />
      </div>
    {:else if chained.ttl?.from === "absolute"}
      <div class="field-row">
        <label for="ttl-absolute-jsonpath">JSONPath</label>
        <input
          id="ttl-absolute-jsonpath"
          type="text"
          class="mono"
          value={chained.ttl.jsonPath}
          oninput={setTtlAbsoluteJsonPath}
        />
      </div>
      <p class="hint">Parsed as an RFC 3339 timestamp.</p>
    {/if}
  </fieldset>

  <fieldset class="question">
    <legend>4. How is it injected?</legend>
    <select value={injectChoice} onchange={onInjectChoiceChange}>
      <option value="header">Header</option>
      <option value="query">Query parameter</option>
      <option value="body">JSON body pointer</option>
    </select>

    {#if chained.inject.into === "header"}
      <div class="field-row">
        <label for="inject-header-name">Header name</label>
        <input
          id="inject-header-name"
          type="text"
          value={chained.inject.name}
          oninput={setInjectHeaderName}
        />
      </div>
      <div class="field-row">
        <label for="inject-header-template">Template</label>
        <input
          id="inject-header-template"
          type="text"
          class="mono"
          value={chained.inject.template}
          oninput={setInjectHeaderTemplate}
        />
      </div>
    {:else if chained.inject.into === "query"}
      <div class="field-row">
        <label for="inject-query-name">Param name</label>
        <input
          id="inject-query-name"
          type="text"
          value={chained.inject.name}
          oninput={setInjectQueryName}
        />
      </div>
      <div class="field-row">
        <label for="inject-query-template">Template</label>
        <input
          id="inject-query-template"
          type="text"
          class="mono"
          value={chained.inject.template}
          oninput={setInjectQueryTemplate}
        />
      </div>
    {:else if chained.inject.into === "body"}
      <div class="field-row">
        <label for="inject-body-pointer">JSON pointer</label>
        <input
          id="inject-body-pointer"
          type="text"
          class="mono"
          placeholder="/auth/token"
          value={chained.inject.pointer}
          oninput={setInjectBodyPointer}
        />
      </div>
      <div class="field-row">
        <label for="inject-body-template">Template</label>
        <input
          id="inject-body-template"
          type="text"
          class="mono"
          value={chained.inject.template}
          oninput={setInjectBodyTemplate}
        />
      </div>
    {/if}
    <p class="hint"><code>{"{{value}}"}</code> is replaced by the token.</p>
  </fieldset>

  <fieldset class="question">
    <legend>Retry on</legend>
    <div class="retry-chips">
      {#each RETRY_STATUS_CHOICES as status (status)}
        <label class="chip" class:chip-active={retryOn.includes(status)}>
          <input
            type="checkbox"
            checked={retryOn.includes(status)}
            onchange={() => onRetryToggle(status)}
          />
          {status}
        </label>
      {/each}
    </div>
    <p class="hint">
      On these statuses, the cached token is invalidated and the chain is
      re-run exactly once before the real request is repeated.
    </p>
  </fieldset>
</div>

<style>
  .chained-builder {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }

  .question {
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 0.6rem 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .question legend {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-muted);
    padding: 0 0.2rem;
  }

  select,
  input {
    width: 100%;
    padding: 0.3rem 0.4rem;
    font-size: 0.85rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
    box-sizing: border-box;
  }

  input.mono {
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .field-row label {
    flex-shrink: 0;
    width: 8rem;
    font-size: 0.8rem;
    color: var(--color-text-muted);
  }

  .hint {
    margin: 0;
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .hint code {
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  }

  .retry-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.2rem 0.5rem;
    font-size: 0.8rem;
    border: 1px solid var(--color-border);
    border-radius: 999px;
    background: var(--color-surface);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .chip-active {
    border-color: var(--color-accent);
    color: var(--color-text);
    background: var(--color-selected);
  }

  .chip input {
    width: auto;
    margin: 0;
  }
</style>
