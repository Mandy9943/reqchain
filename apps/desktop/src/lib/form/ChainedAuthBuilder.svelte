<script lang="ts">
  import type { Api, Auth } from "../model";
  import {
    asChained,
    asRetryOn,
    buildExtract,
    buildInject,
    buildTtl,
    currentSourceProblem,
    DEFAULT_EXTRACT_JSON_PATH,
    DEFAULT_TTL_JSON_PATH,
    excludedSourceCandidates,
    extraRetryStatuses,
    extractChoiceOf,
    injectChoiceOf,
    RETRY_STATUS_CHOICES,
    sourceProblemLabel,
    toggleRetryStatus,
    ttlChoiceOf,
    validSourceEndpoints,
    XPATH_UNSUPPORTED_MESSAGE,
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

  // Unique per component instance, so two `ChainedAuthBuilder`s mounted at
  // once (task 6's `ApiForm` will mount one at API level alongside this
  // one at endpoint level) never collide on a hardcoded DOM id/`for` pair.
  const uid = $props.id();

  // Defensive coercion, not just a display convenience: a JSON-tab edit can
  // hand `AuthEditor` a `type: "chained"` value missing `source`/`inject` —
  // `parseApi`'s `validateAuthShape` should make that unreachable, but this
  // is the second, independent layer (see chainedAuthBuilder.ts's header
  // comment).
  const chained = $derived(asChained(auth));

  // The ONLY endpoints question 1 may offer — self-reference, any cycle
  // and anything that would exceed the engine's depth limit are already
  // excluded here, never merely reported after the fact.
  const sources = $derived(validSourceEndpoints(api, endpointId));

  // The current `source.endpoint` may already be bad — a cycle, a
  // dangling reference, or too deep — if this file was hand-written, was
  // valid before a sibling endpoint's auth changed, or was written before
  // this builder existed. `sources` alone can't show that: the select
  // would just silently fail to match anything and fall back to "Select an
  // endpoint…", telling the user there is no source when the file says
  // otherwise (tests/fixtures/chain-cycle.json, chain-missing.json,
  // chain-too-deep.json are exactly this). This computes whether that's
  // happening, so the template can render the actual value as an extra,
  // disabled, named option instead of hiding it.
  const currentSourceId = $derived(chained.source.endpoint);
  const currentProblem = $derived(
    currentSourceProblem(api, endpointId, currentSourceId),
  );
  // Reason every OTHER endpoint was excluded, named — used only for the
  // "nothing available" explanation below (an empty `sources` list on its
  // own doesn't say why, or which endpoints were even considered).
  const excludedCandidates = $derived(excludedSourceCandidates(api, endpointId));

  const extractChoice = $derived(extractChoiceOf(chained.extract));
  const ttlChoice = $derived(ttlChoiceOf(chained.ttl));
  const injectChoice = $derived(injectChoiceOf(chained.inject));
  const retryOn = $derived(asRetryOn(chained.retryOn));
  const extraRetry = $derived(extraRetryStatuses(chained.retryOn));

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

  // A freshly-selected "Body — Regex" choice seeds an empty pattern (see
  // `buildExtract`'s doc comment) — an empty regex compiles and matches
  // empty, so the token would silently become `""` with nothing flagging
  // it. Surfaced here rather than seeding a guessed "usable" pattern that
  // could just as easily mislead the user into leaving it unedited.
  const extractRegexMissing = $derived(
    extractChoice === "body-regex" &&
      chained.extract?.from === "body" &&
      (chained.extract.regex ?? "") === "",
  );
</script>

<div class="chained-builder">
  <fieldset class="question">
    <legend>1. Which endpoint provides the token?</legend>
    <select id="{uid}-source" value={currentSourceId} onchange={onSourceChange}>
      <option value="" disabled>Select an endpoint…</option>
      {#each sources as ep (ep.id)}
        <option value={ep.id}>{ep.name}</option>
      {/each}
      {#if currentProblem !== null}
        <!-- The file's ACTUAL current value, shown for what it is instead
             of silently falling back to "Select an endpoint…" (which would
             tell the user there is no source when the file says
             otherwise) — disabled, so it can never be re-selected as-is;
             fixing it means picking one of the real options above. -->
        <option value={currentSourceId} disabled>
          {sourceProblemLabel(currentSourceId, currentProblem)}
        </option>
      {/if}
    </select>
    {#if sources.length === 0}
      <p class="hint">
        {#if endpointId !== null}
          No other endpoint in this API can be used here — every one would
          either be this endpoint itself or would form a cycle or too-deep
          chain back to it.
        {:else}
          No endpoint in this API can be used as the API-level source —
          every one would form a cycle or too-deep chain once every
          endpoint that inherits this auth is taken into account.
        {/if}
        {#if excludedCandidates.length > 0}
          Specifically:
          {excludedCandidates
            .map((c) => sourceProblemLabel(c.id, c.reason))
            .join("; ")}.
        {/if}
      </p>
    {/if}
  </fieldset>

  <fieldset class="question">
    <legend>2. Where in the response is the value?</legend>
    <select id="{uid}-extract-choice" value={extractChoice} onchange={onExtractChoiceChange}>
      <option value="default">
        Documented default — body, JSONPath {DEFAULT_EXTRACT_JSON_PATH}
      </option>
      <option value="body-jsonpath">Body — JSONPath</option>
      <option value="body-regex">Body — Regex</option>
      <option value="header">Response header</option>
      <option value="status">Status code</option>
      {#if extractChoice === "body-xpath"}
        <!-- Never a normal, selectable choice — see extractChoiceOf's doc
             comment. Shown only so an ALREADY-PRESENT xpath extract
             (tests/fixtures/chain.json's `xpath-business` endpoint) is
             represented honestly instead of silently misreported as
             "Body — JSONPath" with an empty field. -->
        <option value="body-xpath" disabled>XPath (unsupported)</option>
      {/if}
    </select>

    {#if extractChoice === "default"}
      <p class="hint">
        Prefilled default: <code>{DEFAULT_EXTRACT_JSON_PATH}</code> in the
        response body. Leaving <code>extract</code> out of the file this way
        makes <code>reqchain validate</code> emit a warning — pick
        "Body — JSONPath" and keep the same path below to silence it while
        keeping identical behavior.
      </p>
    {:else if extractChoice === "body-xpath" && chained.extract?.from === "body"}
      <p class="hint error-hint">{XPATH_UNSUPPORTED_MESSAGE}</p>
      <div class="field-row">
        <label for="{uid}-extract-xpath">XPath (read-only)</label>
        <input
          id="{uid}-extract-xpath"
          type="text"
          class="mono"
          value={chained.extract.xpath ?? ""}
          disabled
        />
      </div>
      <p class="hint">Pick "Body — JSONPath" or "Body — Regex" instead.</p>
    {:else if chained.extract?.from === "body" && chained.extract.regex === undefined}
      <div class="field-row">
        <label for="{uid}-extract-jsonpath">JSONPath</label>
        <input
          id="{uid}-extract-jsonpath"
          type="text"
          class="mono"
          value={chained.extract.jsonPath ?? ""}
          oninput={setExtractJsonPath}
        />
      </div>
    {:else if chained.extract?.from === "body"}
      <div class="field-row">
        <label for="{uid}-extract-regex">Regex</label>
        <input
          id="{uid}-extract-regex"
          type="text"
          class="mono"
          value={chained.extract.regex ?? ""}
          oninput={setExtractBodyRegex}
        />
      </div>
      <p class="hint">Capture group 1 is the token, else the whole match.</p>
      {#if extractRegexMissing}
        <p class="hint error-hint">
          A regex is required — an empty pattern matches an empty string,
          silently producing an empty token.
        </p>
      {/if}
    {:else if chained.extract?.from === "header"}
      <div class="field-row">
        <label for="{uid}-extract-header-name">Header name</label>
        <input
          id="{uid}-extract-header-name"
          type="text"
          value={chained.extract.name}
          oninput={setExtractHeaderName}
        />
      </div>
      <div class="field-row">
        <label for="{uid}-extract-header-regex">Regex (optional)</label>
        <input
          id="{uid}-extract-header-regex"
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
        <label for="{uid}-ttl-jsonpath">JSONPath</label>
        <input
          id="{uid}-ttl-jsonpath"
          type="text"
          class="mono"
          value={chained.ttl.jsonPath}
          oninput={setTtlBodyJsonPath}
        />
      </div>
      <div class="field-row">
        <label for="{uid}-ttl-unit">Unit</label>
        <select id="{uid}-ttl-unit" value={chained.ttl.unit} onchange={setTtlUnit}>
          <option value="seconds">Seconds</option>
          <option value="milliseconds">Milliseconds</option>
        </select>
      </div>
    {:else if chained.ttl?.from === "fixed"}
      <div class="field-row">
        <label for="{uid}-ttl-fixed-seconds">Seconds</label>
        <input
          id="{uid}-ttl-fixed-seconds"
          type="number"
          min="0"
          value={chained.ttl.seconds}
          oninput={setTtlFixedSeconds}
        />
      </div>
    {:else if chained.ttl?.from === "absolute"}
      <div class="field-row">
        <label for="{uid}-ttl-absolute-jsonpath">JSONPath</label>
        <input
          id="{uid}-ttl-absolute-jsonpath"
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
        <label for="{uid}-inject-header-name">Header name</label>
        <input
          id="{uid}-inject-header-name"
          type="text"
          value={chained.inject.name}
          oninput={setInjectHeaderName}
        />
      </div>
      <div class="field-row">
        <label for="{uid}-inject-header-template">Template</label>
        <input
          id="{uid}-inject-header-template"
          type="text"
          class="mono"
          value={chained.inject.template}
          oninput={setInjectHeaderTemplate}
        />
      </div>
    {:else if chained.inject.into === "query"}
      <div class="field-row">
        <label for="{uid}-inject-query-name">Param name</label>
        <input
          id="{uid}-inject-query-name"
          type="text"
          value={chained.inject.name}
          oninput={setInjectQueryName}
        />
      </div>
      <div class="field-row">
        <label for="{uid}-inject-query-template">Template</label>
        <input
          id="{uid}-inject-query-template"
          type="text"
          class="mono"
          value={chained.inject.template}
          oninput={setInjectQueryTemplate}
        />
      </div>
    {:else if chained.inject.into === "body"}
      <div class="field-row">
        <label for="{uid}-inject-body-pointer">JSON pointer</label>
        <input
          id="{uid}-inject-body-pointer"
          type="text"
          class="mono"
          placeholder="/auth/token"
          value={chained.inject.pointer}
          oninput={setInjectBodyPointer}
        />
      </div>
      <div class="field-row">
        <label for="{uid}-inject-body-template">Template</label>
        <input
          id="{uid}-inject-body-template"
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
      {#each extraRetry as status (status)}
        <!-- A status outside the curated chip set above (e.g. a JSON-tab
             edit added 504) — still real, still preserved on every edit
             here, just with no chip of its own. Shown read-only rather
             than silently hidden. -->
        <span class="chip chip-readonly" title="Set outside this builder — edit via the JSON tab to remove it">
          {status}
        </span>
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
    border-radius: var(--radius-md);
    background-color: var(--color-bg);
    color: var(--color-text);
    box-sizing: border-box;
  }

  input.mono {
    font-family: var(--font-mono);
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
    font-family: var(--font-mono);
  }

  .error-hint {
    color: var(--color-error-text);
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

  .chip-readonly {
    cursor: default;
    font-style: italic;
    opacity: 0.75;
  }
</style>
