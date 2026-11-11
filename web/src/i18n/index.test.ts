import { describe, expect, it } from "vitest";
import en from "./en.json";
import nl from "./nl.json";
import { detectLanguage, t } from "./index";

describe("translation catalogs", () => {
  it("keeps English and Dutch keys identical", () => {
    expect(Object.keys(nl).sort()).toEqual(Object.keys(en).sort());
  });

  it("interpolates variables", () => {
    expect(
      t("en", "status.version", {
        version: "1.2.3",
        platform: "linux/x86_64",
      }),
    ).toBe("Version 1.2.3 · linux/x86_64");
  });

  it("leaves unknown variables visible", () => {
    expect(t("en", "status.version", { version: "1.2.3" })).toContain("{platform}");
  });
});

describe("detectLanguage", () => {
  const storage = (value: string | null) => ({
    getItem: () => value,
  });

  it("uses a persisted choice", () => {
    expect(detectLanguage(storage("nl"), "en-US")).toBe("nl");
  });

  it("uses Dutch for Dutch browsers without a choice", () => {
    expect(detectLanguage(storage(null), "nl-BE")).toBe("nl");
  });

  it("defaults to English for other browsers", () => {
    expect(detectLanguage(storage(null), "de-DE")).toBe("en");
  });

  it("ignores unsupported persisted values", () => {
    expect(detectLanguage(storage("fr"), "en-US")).toBe("en");
  });
});
