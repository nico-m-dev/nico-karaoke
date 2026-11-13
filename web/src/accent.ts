export type Accent = "cyan" | "electric-blue" | "lime" | "magenta" | "orange";

export const ACCENTS: readonly Accent[] = ["cyan", "electric-blue", "lime", "magenta", "orange"];

export const DEFAULT_ACCENT: Accent = "lime";

export const ACCENT_STORAGE_KEY = "nico.accent";

type ReadableStorage = Pick<Storage, "getItem">;
type WritableStorage = Pick<Storage, "setItem">;

type AccentRoot = {
  dataset: { accent?: string };
};

export function isAccent(value: string | null): value is Accent {
  return value !== null && (ACCENTS as readonly string[]).includes(value);
}

export function detectAccent(storage: ReadableStorage = localStorage): Accent {
  try {
    const stored = storage.getItem(ACCENT_STORAGE_KEY);
    if (isAccent(stored)) {
      return stored;
    }
  } catch {
    // Storage can be unavailable in private or embedded contexts; use the default.
  }
  return DEFAULT_ACCENT;
}

export function rememberAccent(accent: Accent, storage: WritableStorage = localStorage): void {
  try {
    storage.setItem(ACCENT_STORAGE_KEY, accent);
  } catch {
    // A persisted choice is nice, not required.
  }
}

/** The stylesheet keys every palette on this attribute, so the whole app follows along. */
export function applyAccent(accent: Accent, root: AccentRoot = document.documentElement): void {
  root.dataset.accent = accent;
}
