import { describe, expect, it } from "vitest";
import { applyAccent, detectAccent, isAccent, rememberAccent } from "./accent";

describe("detectAccent", () => {
  const storage = (value: string | null) => ({
    getItem: () => value,
  });

  it("uses a persisted choice", () => {
    expect(detectAccent(storage("magenta"))).toBe("magenta");
  });

  it("accepts every supported accent", () => {
    for (const accent of ["cyan", "electric-blue", "lime", "magenta", "orange"] as const) {
      expect(detectAccent(storage(accent))).toBe(accent);
    }
  });

  it("falls back to lime without a choice", () => {
    expect(detectAccent(storage(null))).toBe("lime");
  });

  it("ignores unsupported persisted values", () => {
    expect(detectAccent(storage("purple"))).toBe("lime");
  });

  it("survives unavailable storage", () => {
    const blocked = {
      getItem: () => {
        throw new Error("blocked");
      },
    };
    expect(detectAccent(blocked)).toBe("lime");
  });
});

describe("isAccent", () => {
  it("rejects null and unknown names", () => {
    expect(isAccent(null)).toBe(false);
    expect(isAccent("green")).toBe(false);
  });
});

describe("rememberAccent", () => {
  it("stores the chosen accent", () => {
    const values = new Map<string, string>();
    rememberAccent("cyan", { setItem: (key, value) => values.set(key, value) });
    expect(values.get("nico.accent")).toBe("cyan");
  });

  it("does not throw when storage rejects the write", () => {
    expect(() =>
      rememberAccent("orange", {
        setItem: () => {
          throw new Error("blocked");
        },
      }),
    ).not.toThrow();
  });
});

describe("applyAccent", () => {
  it("writes the palette attribute on the root element", () => {
    const root = { dataset: {} as { accent?: string } };
    applyAccent("electric-blue", root);
    expect(root.dataset.accent).toBe("electric-blue");
  });
});
