import "./styles.css";
import { applyTranslations, detectLanguage, rememberLanguage, t, type Language } from "./i18n";

// Keep this shape in sync with `HealthResponse` in `crates/server/src/api.rs` until shared
// API types are generated.
type Health = {
  status: "ok";
  name: string;
  version: string;
  platform: string;
  supported: boolean;
};

const appElement = document.querySelector<HTMLDivElement>("#app");
if (!appElement) {
  throw new Error("Missing #app element");
}
const app: HTMLDivElement = appElement;

let language: Language = detectLanguage();
let health: Health | null = null;
let healthError = false;

function renderShell(): void {
  app.innerHTML = `
    <main class="shell">
      <header class="masthead">
        <div>
          <p class="eyebrow" data-i18n="app.title"></p>
          <h1 data-i18n="app.tagline"></h1>
        </div>
        <div class="languages" role="group" data-i18n-aria-label="language.label">
          <button type="button" data-language="en" data-i18n-title="language.en">EN</button>
          <button type="button" data-language="nl" data-i18n-title="language.nl">NL</button>
        </div>
      </header>
      <section class="card" aria-live="polite">
        <p id="status" data-i18n="status.checking"></p>
        <p id="meta" class="meta"></p>
      </section>
    </main>
  `;

  app.querySelectorAll<HTMLButtonElement>("[data-language]").forEach((button) => {
    button.addEventListener("click", () => {
      const next = button.dataset.language;
      if (next === "en" || next === "nl") {
        setLanguage(next);
      }
    });
  });
}

function setLanguage(next: Language, persist = true): void {
  language = next;
  if (persist) {
    rememberLanguage(next);
  }
  document.documentElement.lang = next;
  document.title = t(next, "app.title");
  applyTranslations(app, next);
  updateLanguageButtons();
  renderStatus();
}

function updateLanguageButtons(): void {
  app.querySelectorAll<HTMLButtonElement>("[data-language]").forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.language === language));
  });
}

function renderStatus(): void {
  const status = app.querySelector<HTMLParagraphElement>("#status");
  const meta = app.querySelector<HTMLParagraphElement>("#meta");
  if (!status || !meta) {
    return;
  }

  if (health) {
    status.textContent = t(language, "status.ok");
    meta.textContent = t(language, "status.version", {
      version: health.version,
      platform: health.platform,
    });
  } else if (healthError) {
    status.textContent = t(language, "status.error");
    meta.textContent = "";
  } else {
    status.textContent = t(language, "status.checking");
    meta.textContent = "";
  }
}

async function checkHealth(): Promise<void> {
  healthError = false;
  try {
    const response = await fetch("/api/health", {
      cache: "no-store",
      headers: { Accept: "application/json" },
    });
    if (!response.ok) {
      throw new Error(`health request failed with ${response.status}`);
    }
    const payload: unknown = await response.json();
    if (!isHealth(payload)) {
      throw new Error("health response has an unexpected shape");
    }
    health = payload;
  } catch {
    health = null;
    healthError = true;
  }
  renderStatus();
}

function isHealth(value: unknown): value is Health {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    record.status === "ok" &&
    typeof record.name === "string" &&
    typeof record.version === "string" &&
    typeof record.platform === "string" &&
    typeof record.supported === "boolean"
  );
}

renderShell();
setLanguage(language, false);
void checkHealth();
