// `$state` is only usable inside `.svelte`/`.svelte.js`/`.svelte.ts` files —
// a plain `*.test.ts` file cannot declare it directly (Svelte's compiler
// only rune-transforms files with those extensions; used from anywhere
// else it fails at runtime with `rune_outside_svelte`). This tiny helper
// exists solely so `EndpointForm.dom.test.ts` can create genuinely-reactive
// props for `mount(...)` and then mutate them from ordinary test code —
// the returned object is a real Svelte reactive proxy, and plain property
// assignment on it (even from a file with no rune syntax of its own)
// correctly triggers reactivity, the same way any `.ts` file in this
// codebase already mutates `state.svelte.ts`'s exported `ui` object.
export function reactiveProps<T extends object>(initial: T): T {
  let props = $state(initial);
  return props;
}
