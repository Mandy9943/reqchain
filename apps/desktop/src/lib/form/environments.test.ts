import { describe, expect, it } from "vitest";
import {
  asEnvironments,
  emptyEnvironment,
  nextEnvSelection,
  validateEnvironmentName,
} from "./environments";

describe("asEnvironments", () => {
  it("treats non-array runtime input as empty instead of throwing", () => {
    expect(asEnvironments(undefined)).toEqual([]);
    expect(asEnvironments(null)).toEqual([]);
    expect(asEnvironments({})).toEqual([]);
    expect(asEnvironments("prod")).toEqual([]);
  });

  it("passes through well-shaped entries", () => {
    expect(
      asEnvironments([{ name: "prod", variables: { host: "p.example.com" } }]),
    ).toEqual([{ name: "prod", variables: { host: "p.example.com" } }]);
  });

  it("coerces a malformed entry instead of throwing", () => {
    expect(asEnvironments([null])).toEqual([{ name: "", variables: {} }]);
    expect(asEnvironments(["prod"])).toEqual([{ name: "", variables: {} }]);
    expect(asEnvironments([{ variables: {} }])).toEqual([
      { name: "", variables: {} },
    ]);
  });

  it("coerces a non-object variables field on an otherwise well-shaped entry", () => {
    expect(asEnvironments([{ name: "prod", variables: null }])).toEqual([
      { name: "prod", variables: {} },
    ]);
    expect(asEnvironments([{ name: "prod", variables: [] }])).toEqual([
      { name: "prod", variables: {} },
    ]);
  });

  it("coerces a missing variables field to an empty object", () => {
    expect(asEnvironments([{ name: "prod" }])).toEqual([
      { name: "prod", variables: {} },
    ]);
  });
});

describe("emptyEnvironment", () => {
  it("has field order name, then variables", () => {
    expect(Object.keys(emptyEnvironment("prod"))).toEqual(["name", "variables"]);
  });

  it("starts with no variables", () => {
    expect(emptyEnvironment("prod")).toEqual({ name: "prod", variables: {} });
  });
});

describe("validateEnvironmentName", () => {
  const envs = [
    { name: "prod", variables: {} },
    { name: "staging", variables: {} },
  ];

  it("rejects an empty or whitespace-only name", () => {
    expect(validateEnvironmentName(envs, "", null)).toEqual({
      ok: false,
      error: "Enter a name.",
    });
    expect(validateEnvironmentName(envs, "   ", null)).toEqual({
      ok: false,
      error: "Enter a name.",
    });
  });

  it("rejects a name that collides with an existing environment", () => {
    const result = validateEnvironmentName(envs, "prod", null);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toMatch(/prod.*already exists/);
  });

  it("trims whitespace before checking for a collision", () => {
    expect(validateEnvironmentName(envs, " prod ", null).ok).toBe(false);
  });

  it("accepts a new, unique name and returns it trimmed", () => {
    expect(validateEnvironmentName(envs, " qa ", null)).toEqual({
      ok: true,
      name: "qa",
    });
  });

  it("does not collide an environment with itself when renaming (ignoreIndex)", () => {
    // Renaming envs[0] ("prod") to the same name it already has must not be
    // refused as a collision with itself.
    expect(validateEnvironmentName(envs, "prod", 0)).toEqual({
      ok: true,
      name: "prod",
    });
  });

  it("still refuses renaming to a name used by a DIFFERENT environment", () => {
    // Renaming envs[0] ("prod") to "staging" collides with envs[1].
    expect(validateEnvironmentName(envs, "staging", 0).ok).toBe(false);
  });
});

describe("nextEnvSelection", () => {
  const envs = [
    { name: "prod", variables: {} },
    { name: "staging", variables: {} },
  ];

  it("keeps the current selection when it still names a real environment", () => {
    expect(nextEnvSelection(envs, "staging")).toBe("staging");
  });

  it("falls back to the first environment when the current selection is stale", () => {
    expect(nextEnvSelection(envs, "removed")).toBe("prod");
  });

  it("falls back to null when there is no environment left at all", () => {
    expect(nextEnvSelection([], "prod")).toBe(null);
  });

  it("leaves a null selection null when there is no environment", () => {
    expect(nextEnvSelection([], null)).toBe(null);
  });

  it("picks the first environment for a null selection when one now exists", () => {
    expect(nextEnvSelection(envs, null)).toBe("prod");
  });
});
