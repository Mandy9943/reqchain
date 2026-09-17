import { describe, expect, it } from "vitest";
import { nextEnvSelection } from "./envSelection";

describe("nextEnvSelection", () => {
  const names = ["prod", "staging"];

  it("keeps the current selection when it still names a real environment", () => {
    expect(nextEnvSelection(names, "staging")).toBe("staging");
  });

  it("falls back to the first environment when the current selection is stale", () => {
    expect(nextEnvSelection(names, "removed")).toBe("prod");
  });

  it("treats an undefined selection (never set) the same as null", () => {
    expect(nextEnvSelection(names, undefined)).toBe("prod");
    expect(nextEnvSelection(names, null)).toBe("prod");
  });

  it("falls back to null when there is no environment left at all", () => {
    expect(nextEnvSelection([], "prod")).toBe(null);
    expect(nextEnvSelection([], null)).toBe(null);
    expect(nextEnvSelection([], undefined)).toBe(null);
  });

  it("picks the first environment for a null/undefined selection when one now exists", () => {
    expect(nextEnvSelection(names, null)).toBe("prod");
    expect(nextEnvSelection(names, undefined)).toBe("prod");
  });
});
