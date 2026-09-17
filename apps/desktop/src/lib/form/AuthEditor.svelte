<script lang="ts">
  import type { Api, Auth } from "../model";
  import KeyValueRows from "./KeyValueRows.svelte";
  import { asRecord } from "./keyValueRows";
  import {
    authTypeOf,
    COMPUTED_FUNCTIONS,
    defaultAuthForType,
    inheritLabel,
    type AuthType,
  } from "./authEditor";
  import { validSourceEndpoints } from "./chainedAuthBuilder";
  import ChainedAuthBuilder from "./ChainedAuthBuilder.svelte";

  // `endpointId` is `null` when this is editing the API-level `auth`
  // (task 6's `ApiForm`, reusing this component unmodified) — the chained
  // builder needs it to know which endpoint it must not offer as its own
  // source, and `allowInherit` controls whether `inherit` is offered at
  // all: it resolves to something at endpoint level (the API's own auth),
  // but there is nothing above the API to inherit from, so spec §17.1 says
  // not to offer it there.
  let {
    auth,
    onChange,
    api,
    endpointId,
    allowInherit,
  }: {
    auth: Auth;
    onChange: (a: Auth) => void;
    api: Api;
    endpointId: string | null;
    allowInherit: boolean;
  } = $props();

  const ALL_TYPES: { value: AuthType; label: string }[] = [
    { value: "inherit", label: "" }, // label filled in below, once `api` is known
    { value: "none", label: "None" },
    { value: "basic", label: "Basic" },
    { value: "bearer", label: "Bearer" },
    { value: "header", label: "Header" },
    { value: "computed", label: "Computed" },
    { value: "chained", label: "Chained" },
  ];

  const types = $derived(
    ALL_TYPES.filter((t) => allowInherit || t.value !== "inherit").map((t) =>
      t.value === "inherit" ? { ...t, label: inheritLabel(api) } : t,
    ),
  );

  // `model.rs`'s own serde defaults, per level — see `authTypeOf`'s doc
  // comment. An endpoint with no `auth` key is `inherit`; an API with no
  // `auth` key is `none`. `endpointId === null` is exactly how this
  // component already distinguishes "editing the API's own auth" (task 6's
  // `ApiForm`) from "editing one endpoint's auth".
  const levelFallback = $derived<AuthType>(endpointId === null ? "none" : "inherit");
  const currentType = $derived(authTypeOf(auth, levelFallback));

  // Unique per component instance, so two `AuthEditor`s mounted at once
  // (task 6's `ApiForm` alongside `EndpointForm`) never collide on a
  // hardcoded DOM id/`for` pair.
  const uid = $props.id();

  function onTypeChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as AuthType;
    // Prefill a fresh `chained` auth's source with the first endpoint this
    // API actually allows — never an id that would be a self-reference or
    // a cycle (see `validSourceEndpoints`), and never empty when a valid
    // choice exists.
    const firstValidSource = validSourceEndpoints(api, endpointId)[0]?.id;
    onChange(defaultAuthForType(value, firstValidSource));
  }

  function onBasicUsernameChange(e: Event): void {
    if (auth.type !== "basic") return;
    onChange({ ...auth, username: (e.currentTarget as HTMLInputElement).value });
  }

  function onBasicPasswordChange(e: Event): void {
    if (auth.type !== "basic") return;
    onChange({ ...auth, password: (e.currentTarget as HTMLInputElement).value });
  }

  function onBearerTokenChange(e: Event): void {
    if (auth.type !== "bearer") return;
    onChange({ ...auth, token: (e.currentTarget as HTMLInputElement).value });
  }

  function onHeaderAuthChange(headers: Record<string, string>): void {
    onChange({ type: "header", headers });
  }

  function onComputedNameChange(e: Event): void {
    if (auth.type !== "computed") return;
    onChange({ ...auth, name: (e.currentTarget as HTMLInputElement).value });
  }

  function onComputedExpressionChange(e: Event): void {
    if (auth.type !== "computed") return;
    onChange({ ...auth, expression: (e.currentTarget as HTMLInputElement).value });
  }
</script>

<div class="auth-editor">
  <div class="field-row">
    <label for="{uid}-auth-type">Auth</label>
    <select id="{uid}-auth-type" value={currentType} onchange={onTypeChange}>
      {#each types as t (t.value)}
        <option value={t.value}>{t.label}</option>
      {/each}
    </select>
  </div>

  {#if currentType === "basic"}
    <div class="field-row">
      <label for="{uid}-auth-basic-username">Username</label>
      <input
        id="{uid}-auth-basic-username"
        type="text"
        value={auth.type === "basic" ? auth.username : ""}
        oninput={onBasicUsernameChange}
      />
    </div>
    <div class="field-row">
      <label for="{uid}-auth-basic-password">Password</label>
      <input
        id="{uid}-auth-basic-password"
        type="text"
        placeholder={"{{secret:NAME}}"}
        value={auth.type === "basic" ? auth.password : ""}
        oninput={onBasicPasswordChange}
      />
    </div>
  {:else if currentType === "bearer"}
    <div class="field-row">
      <label for="{uid}-auth-bearer-token">Token</label>
      <input
        id="{uid}-auth-bearer-token"
        type="text"
        placeholder={"{{secret:NAME}}"}
        value={auth.type === "bearer" ? auth.token : ""}
        oninput={onBearerTokenChange}
      />
    </div>
  {:else if currentType === "header"}
    <section class="fields-section">
      <KeyValueRows
        rows={auth.type === "header" ? asRecord(auth.headers) : {}}
        onChange={onHeaderAuthChange}
        keyLabel="Header"
        valueLabel="Value"
      />
    </section>
  {:else if currentType === "computed"}
    <div class="field-row">
      <label for="{uid}-auth-computed-name">Header name</label>
      <input
        id="{uid}-auth-computed-name"
        type="text"
        value={auth.type === "computed" ? auth.name : ""}
        oninput={onComputedNameChange}
      />
    </div>
    <div class="field-row">
      <label for="{uid}-auth-computed-expr">Expression</label>
      <input
        id="{uid}-auth-computed-expr"
        type="text"
        class="mono"
        value={auth.type === "computed" ? auth.expression : ""}
        oninput={onComputedExpressionChange}
      />
    </div>
    <p class="hint">
      Functions: {COMPUTED_FUNCTIONS.join(", ")}. <code>{"{{name}}"}</code>
      references a variable. An unknown function name is a validation error.
    </p>
  {:else if currentType === "chained"}
    <ChainedAuthBuilder {auth} {onChange} {api} {endpointId} />
  {/if}
</div>

<style>
  .auth-editor {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .field-row label {
    flex-shrink: 0;
    width: 7rem;
    font-size: 0.8rem;
    color: var(--color-text-muted);
  }

  select,
  input {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.4rem;
    font-size: 0.85rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-text);
  }

  input.mono {
    font-family:
      ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
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
</style>
