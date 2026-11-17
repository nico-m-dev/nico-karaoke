import { ACCENTS, type Accent } from "./accent";
import type { UpdateStatus } from "./api";
import { applyTranslations, t, type Language, type MessageKey } from "./i18n";

const ACCENT_MESSAGES: Record<Accent, MessageKey> = {
  cyan: "accent.cyan",
  "electric-blue": "accent.electric_blue",
  lime: "accent.lime",
  magenta: "accent.magenta",
  orange: "accent.orange",
  red: "accent.red",
};

export type SettingsPanel = {
  open: (opener: HTMLElement | null) => void;
  close: () => void;
  isOpen: () => boolean;
  setLanguage: (language: Language) => void;
  setAccent: (accent: Accent) => void;
  setVersion: (version: string) => void;
  setUpdate: (status: UpdateStatus) => void;
};

type SettingsOptions = {
  language: Language;
  accent: Accent;
  /** Elements that must not be reachable while the panel is open. */
  background: () => HTMLElement[];
  onAccent: (accent: Accent) => void;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
};

/** One shared appearance panel for the player, idle screen, and Studio. */
export function createSettingsPanel(options: SettingsOptions): SettingsPanel {
  let language = options.language;
  let accent = options.accent;
  let version = "—";
  let update: UpdateStatus | null = null;
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
      <section class="settings-about">
        <p class="settings-label" data-i18n="settings.about"></p>
        <p class="settings-version"><span data-i18n="settings.version"></span> <span data-settings-version>—</span></p>
        <p class="settings-update-status" data-settings-update-status></p>
        <p class="sr-only" role="status" data-settings-update-live></p>
        <p class="settings-update-detail" data-settings-update-detail hidden></p>
        <div class="settings-update-actions">
          <button class="ghost" type="button" data-settings-check data-i18n="update.check"></button>
          <button class="ghost primary" type="button" data-settings-install data-i18n="update.install" hidden></button>
        </div>
      </section>
    </aside>
  `;
  document.body.append(overlay);

  const card = overlay.querySelector<HTMLElement>(".settings-card");
  const closeButton = overlay.querySelector<HTMLButtonElement>("[data-settings-close]");
  const group = overlay.querySelector<HTMLElement>(".settings-accents");
  const versionValue = overlay.querySelector<HTMLElement>("[data-settings-version]");
  const updateStatus = overlay.querySelector<HTMLElement>("[data-settings-update-status]");
  const updateDetail = overlay.querySelector<HTMLElement>("[data-settings-update-detail]");
  const updateLive = overlay.querySelector<HTMLElement>("[data-settings-update-live]");
  const checkButton = overlay.querySelector<HTMLButtonElement>("[data-settings-check]");
  const installButton = overlay.querySelector<HTMLButtonElement>("[data-settings-install]");
  if (
    !card ||
    !closeButton ||
    !group ||
    !versionValue ||
    !updateStatus ||
    !updateDetail ||
    !updateLive ||
    !checkButton ||
    !installButton
  ) {
    throw new Error("Settings panel markup is incomplete");
  }
  const panel = card;
  const closeControl = closeButton;
  const versionEl = versionValue;
  const statusEl = updateStatus;
  const detailEl = updateDetail;
  const liveEl = updateLive;
  const checkControl = checkButton;
  const installControl = installButton;
  let lastAnnouncedPhase: string | null = null;

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
    lastAnnouncedPhase = null;
    renderUpdate();
  }

  function setVersion(next: string): void {
    version = next;
    versionEl.textContent = version;
  }

  function setUpdate(next: UpdateStatus): void {
    update = next;
    renderUpdate();
  }

  function renderUpdate(): void {
    versionEl.textContent = version;
    const status = update;
    if (!status || !status.supported) {
      statusEl.textContent = "";
      detailEl.textContent = "";
      detailEl.hidden = true;
      liveEl.textContent = "";
      lastAnnouncedPhase = null;
      checkControl.hidden = true;
      checkControl.disabled = false;
      checkControl.textContent = t(language, "update.check");
      installControl.hidden = true;
      installControl.disabled = false;
      return;
    }

    const busy =
      status.phase === "checking" ||
      status.phase === "downloading" ||
      status.phase === "installing";
    checkControl.hidden = status.phase === "downloading" || status.phase === "installing";
    checkControl.disabled = busy;
    checkControl.textContent = t(
      language,
      status.phase === "checking" ? "update.checking" : "update.check",
    );
    installControl.hidden = status.phase !== "available";
    installControl.disabled = status.phase !== "available";

    switch (status.phase) {
      case "checking":
        statusEl.textContent = t(language, "update.checking");
        break;
      case "available":
        statusEl.textContent = t(language, "update.available", {
          version: status.version ?? "",
        });
        break;
      case "up_to_date":
        statusEl.textContent = t(language, "update.up_to_date");
        break;
      case "downloading": {
        const percent = status.progress === null ? null : Math.round(status.progress * 100);
        statusEl.textContent =
          percent === null
            ? t(language, "update.downloading_unknown", { version: status.version ?? "" })
            : t(language, "update.downloading", {
                version: status.version ?? "",
                percent,
              });
        break;
      }
      case "installing":
        statusEl.textContent = t(language, "update.installing");
        break;
      case "error":
        statusEl.textContent = t(
          language,
          status.version ? "update.install_failed" : "update.error",
        );
        break;
      default:
        statusEl.textContent = "";
        break;
    }

    const detail = status.phase === "error" ? (status.error ?? "") : "";
    detailEl.textContent = detail;
    detailEl.hidden = detail === "";
    if (lastAnnouncedPhase !== status.phase) {
      lastAnnouncedPhase = status.phase;
      liveEl.textContent = statusEl.textContent;
    }
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
  checkControl.addEventListener("click", () => options.onCheckUpdate());
  installControl.addEventListener("click", () => options.onInstallUpdate());
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

  return {
    open,
    close,
    isOpen: () => !overlay.hidden,
    setLanguage,
    setAccent,
    setVersion,
    setUpdate,
  };
}
