import en from "./en.json";
import nl from "./nl.json";

export type Language = "en" | "nl";
export type MessageKey = keyof typeof en;

const catalogs: Record<Language, Record<MessageKey, string>> = { en, nl };
const STORAGE_KEY = "nico.lang";

type ReadableStorage = Pick<Storage, "getItem">;
type WritableStorage = Pick<Storage, "setItem">;

export function t(
  language: Language,
  key: MessageKey,
  vars: Record<string, string | number> = {},
): string {
  const template = catalogs[language][key];
  return template.replace(/\{(\w+)\}/g, (_match: string, name: string) => {
    const value = vars[name];
    return value === undefined ? `{${name}}` : String(value);
  });
}

export function detectLanguage(
  storage: ReadableStorage = localStorage,
  browserLanguage: string = navigator.language,
): Language {
  try {
    const stored = storage.getItem(STORAGE_KEY);
    if (stored === "en" || stored === "nl") {
      return stored;
    }
  } catch {
    // Storage can be unavailable in private or embedded contexts; fall back to the browser.
  }

  return browserLanguage.toLowerCase().startsWith("nl") ? "nl" : "en";
}

export function rememberLanguage(
  language: Language,
  storage: WritableStorage = localStorage,
): void {
  try {
    storage.setItem(STORAGE_KEY, language);
  } catch {
    // A persisted choice is nice, not required.
  }
}

export function applyTranslations(root: ParentNode, language: Language): void {
  setText(root, "data-i18n", language);
  setAttribute(root, "data-i18n-title", "title", language);
  setAttribute(root, "data-i18n-aria-label", "aria-label", language);
  setAttribute(root, "data-i18n-placeholder", "placeholder", language);
}

function setText(root: ParentNode, selector: string, language: Language): void {
  root.querySelectorAll<HTMLElement>(`[${selector}]`).forEach((element) => {
    const key = element.getAttribute(selector);
    if (key && isMessageKey(key)) {
      element.textContent = t(language, key);
    }
  });
}

function setAttribute(
  root: ParentNode,
  selector: string,
  attribute: string,
  language: Language,
): void {
  root.querySelectorAll<HTMLElement>(`[${selector}]`).forEach((element) => {
    const key = element.getAttribute(selector);
    if (key && isMessageKey(key)) {
      element.setAttribute(attribute, t(language, key));
    }
  });
}

function isMessageKey(value: string): value is MessageKey {
  return value in en;
}
