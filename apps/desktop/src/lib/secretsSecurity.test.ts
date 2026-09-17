// Grep-style test pinning the security contract that a runtime test cannot
// easily observe on its own: "reveal_secret is the ONLY function in the
// entire application that returns a stored credential to the webview, and
// no component other than the secrets screen may call it." A refactor that
// quietly adds a second call site (e.g. some other screen "just needs to
// check" a value) would compile and pass every functional test while
// reopening exactly the leak spec §17.3 calls out — this is the test that
// catches it.
//
// Uses Vite's `import.meta.glob` (works under vitest, which shares Vite's
// module pipeline) to read every source file as raw text, rather than
// Node's `fs`/`path` — this project has no `@types/node` dependency, and
// this is a source-inspection test, not something that needs real
// filesystem access.
import { describe, expect, it } from "vitest";

const sourceFiles = import.meta.glob("../**/*.{ts,svelte}", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

describe("revealSecret call-site containment", () => {
  it("is imported by exactly one file besides its own definition (ipc.ts)", () => {
    const importers = Object.entries(sourceFiles)
      .filter(([path]) => !path.endsWith("/ipc.ts"))
      .filter(([path]) => !path.endsWith("/secretsSecurity.test.ts"))
      .filter(([, text]) => /\brevealSecret\b/.test(text))
      .map(([path]) => path);

    expect(importers).toHaveLength(1);
    expect(importers[0]).toMatch(/\/SecretsScreen\.svelte$/);
  });
});
