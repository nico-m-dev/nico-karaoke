import { ACCENTS, type Accent } from "./accent";
import { applyTranslations, type Language, type MessageKey } from "./i18n";

const ACCENT_MESSAGES: Record<Accent, MessageKey> = {
  cyan: "accent.cyan",
  "electric-blue": "accent.electric_blue",
  lime: "accent.lime",
  magenta: "accent.magenta",
  orange: "accent.orange",
};

export type SettingsPanel = {
  open: (opener: HTMLElement | null) => void;
  close: () => void;
  isOpen: () => boolean;
  setLanguage: (language: Language) => void;
  setAccent: (accent: Accent) => void;
};

type SettingsOptions = {
  language: Language;
  accent: Accent;
  /** Elements that must not be reachable while the panel is open. */
  background: () => HTMLElement[];
  onAccent: (accent: Accent) => void;
};

/** One shared appearance panel for the player, idle screen, and Studio. */
export function createSettingsPanel(options: SettingsOptions): SettingsPanel {
  let language = options.language;
  let accent = options.accent;
  let opener: HTMLElement | null = null;
  let inerted: HTMLElement[] = [];

  const overlay = document.createElement("div");
  overlay.className = "settings";
  overlay.hidden = true;
  overlay.innerHTML = `
    <aside class="settings-card" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <header class="settings-head">
        <div>
          <h2 id="settings-title" data-i18n="settings.title"></h2>
          <p class="hint" data-i18n="settings.hint"></p>
        </div>
        <button class="ghost icon" type="button" data-settings-close data-i18n-title="settings.close" data-i18n-aria-label="settings.close">✕</button>
      </header>
      <p class="settings-label" data-i18n="settings.accent"></p>
      <div class="settings-accents" role="group" data-i18n-aria-label="settings.accent"></div>
    </aside>
  `;
  document.body.append(overlay);

  const card = overlay.querySelector<HTMLElement>(".settings-card");
  const closeButton = overlay.querySelector<HTMLButtonElement>("[data-settings-close]");
  const group = overlay.querySelector<HTMLElement>(".settings-accents");
  if (!card || !closeButton || !group) {
    throw new Error("Settings panel markup is incomplete");
  }
  const panel = card;
  const closeControl = closeButton;

  const swatches = ACCENTS.map((value) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "accent-swatch";
    button.dataset.accent = value;
    button.setAttribute("aria-pressed", "false");

    const dot = document.createElement("span");
    dot.className = "accent-swatch-dot";
    dot.setAttribute("aria-hidden", "true");

    const label = document.createElement("span");
    label.dataset.i18n = ACCENT_MESSAGES[value];

    button.append(dot, label);
    button.addEventListener("click", () => {
      options.onAccent(value);
      setAccent(value);
    });
    group.append(button);
    return button;
  });

  function setAccent(next: Accent): void {
    accent = next;
    for (const swatch of swatches) {
      const selected = swatch.dataset.accent === next;
      swatch.setAttribute("aria-pressed", String(selected));
    }
  }

  function setLanguage(next: Language): void {
    language = next;
    applyTranslations(overlay, next);
  }

  function open(nextOpener: HTMLElement | null): void {
    if (!overlay.hidden) {
      return;
    }
    opener =
      nextOpener ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    inerted = options.background().filter((node) => !node.inert);
    for (const node of inerted) {
      node.inert = true;
    }
    setLanguage(language);
    setAccent(accent);
    overlay.hidden = false;
    closeControl.focus();
  }

  function close(): void {
    if (overlay.hidden) {
      return;
    }
    overlay.hidden = true;
    for (const node of inerted) {
      node.inert = false;
    }
    inerted = [];
    const target = opener;
    opener = null;
    if (target && canRestoreFocus(target)) {
      target.focus();
      return;
    }
    // The opener can disappear or be painted over while the panel is open, for example when a
    // song starts or ends on the idle screen. Move focus to a visible settings toggle instead
    // of dropping it behind another surface.
    for (const candidate of document.querySelectorAll<HTMLElement>(".settings-toggle")) {
      if (canRestoreFocus(candidate)) {
        candidate.focus();
        return;
      }
    }
  }

  /** Focusable and either on screen or scrolled out of the viewport rather than covered. */
  function canRestoreFocus(element: HTMLElement): boolean {
    if (!element.isConnected) {
      return false;
    }
    const box = element.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) {
      return false;
    }
    const x = box.left + box.width / 2;
    const y = box.top + box.height / 2;
    if (x < 0 || y < 0 || x > window.innerWidth || y > window.innerHeight) {
      return true;
    }
    const hit = document.elementFromPoint(x, y);
    return hit === element || element.contains(hit);
  }

  closeControl.addEventListener("click", close);
  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) {
      close();
    }
  });
  overlay.addEventListener("keydown", (event) => {
    if (event.key === "Tab") {
      trapFocus(event);
    }
  });

  // Escape must work even after a click on static panel content moved focus to the document.
  document.addEventListener("keydown", (event) => {
    if (overlay.hidden || event.key !== "Escape") {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    close();
  });

  function trapFocus(event: KeyboardEvent): void {
    const focusables = Array.from(
      panel.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((element) => !element.hasAttribute("disabled") && element.getClientRects().length > 0);
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (!first || !last) {
      return;
    }
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  setLanguage(language);
  setAccent(accent);

  return { open, close, isOpen: () => !overlay.hidden, setLanguage, setAccent };
}
