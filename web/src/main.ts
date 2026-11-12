import "./styles.css";
import {
  ApiError,
  choosePlayer,
  deleteLibraryItem,
  getLibrary,
  getLibraryItem,
  getNow,
  togglePlayback,
  type LibraryItemResponse,
  type LibraryResponse,
  type LibrarySummary,
  type Lyrics,
  type Now,
  type PlayerInfo,
  type Track,
} from "./api";
import { applyTranslations, detectLanguage, rememberLanguage, t, type Language } from "./i18n";
import { errorMessage, sourceLabelKey } from "./messages";
import {
  fillProgress,
  findActiveLine,
  formatClock,
  formatTime,
  isMusicLine,
  positionAt,
  targetPosition,
  updateAnchor,
  type Anchor,
} from "./player";
import headLogo from "./assets/nico-head.png";
import { matchesLibraryQuery, spotifySearchUrl, youtubeSearchUrl } from "./library";

const appElement = document.querySelector<HTMLDivElement>("#app");
if (!appElement) {
  throw new Error("Missing #app element");
}
const app: HTMLDivElement = appElement;

app.innerHTML = `
  <main class="stage" id="stage">
    <section class="lyrics" id="lyrics" aria-live="off">
      <div class="viewport" id="viewport">
        <div class="track" id="track"></div>
      </div>
      <div class="centerline" aria-hidden="true"></div>
    </section>

    <header class="bar top" id="topbar">
      <span class="dot" aria-hidden="true"></span>
      <div class="titles">
        <span class="song-title" id="song-title" role="heading" aria-level="1"></span>
        <span class="song-artist" id="song-artist"></span>
      </div>
      <span class="offline" id="offline" role="status" data-i18n="player.offline_short" hidden></span>
      <label class="player-pick">
        <span class="sr-only" data-i18n="player.players.aria"></span>
        <select id="players" hidden></select>
      </label>
      <button class="ghost" id="library-btn" type="button" data-i18n="player.library"></button>
      <button class="ghost icon" id="fullscreen-btn" type="button" data-i18n-title="player.fullscreen" data-i18n-aria-label="player.fullscreen">⛶</button>
    </header>

    <footer class="bar bottom" id="controls" hidden>
      <button class="play" id="play" type="button" data-i18n-title="player.playpause" data-i18n-aria-label="player.playpause">▶</button>
      <span class="time" id="time">0:00</span>
    </footer>

    <section class="idle" id="idle">
      <div class="idle-inner">
        <div class="mark-wrap" aria-hidden="true">
          <img class="mark" src="${headLogo}" alt="" />
        </div>
        <p class="eyebrow" data-i18n="app.title"></p>
        <h1 data-i18n="player.idle.title"></h1>
        <p class="hint" data-i18n="player.idle.lead"></p>
        <p class="hint players-hint" id="idle-hint"></p>
        <div class="idle-actions">
          <button class="ghost" id="library-idle" type="button" data-i18n="player.library"></button>
          <div class="languages" role="group" data-i18n-aria-label="lang.label">
            <button type="button" data-language="nl" data-i18n-title="language.nl">NL</button>
            <button type="button" data-language="en" data-i18n-title="language.en">EN</button>
          </div>
        </div>
      </div>
    </section>
  </main>

  <div class="library" id="library" hidden>
    <aside class="library-card" role="dialog" aria-modal="true" aria-labelledby="library-title">
      <header class="library-head">
        <div>
          <h2 id="library-title" data-i18n="player.library"></h2>
          <p class="hint" id="library-count"></p>
        </div>
        <div class="library-head-actions">
          <button class="ghost icon" id="library-refresh" type="button" data-i18n-title="player.library.refresh" data-i18n-aria-label="player.library.refresh">↻</button>
          <button class="ghost icon" id="library-close" type="button" data-i18n-title="player.library.close" data-i18n-aria-label="player.library.close">✕</button>
        </div>
      </header>
      <input id="library-search" class="library-search" type="search" data-i18n-placeholder="player.library.search_placeholder" data-i18n-aria-label="player.library.search_aria" autocomplete="off">
      <div class="library-list" id="library-list"></div>
      <section class="library-detail" id="library-detail" hidden></section>
    </aside>
  </div>

  <div class="toast" id="toast" role="status" aria-live="polite"></div>
`;

function query<T extends Element>(selector: string): T {
  const node = document.querySelector<T>(selector);
  if (!node) {
    throw new Error(`Missing element: ${selector}`);
  }
  return node;
}

const els = {
  stage: query<HTMLElement>("#stage"),
  viewport: query<HTMLElement>("#viewport"),
  track: query<HTMLElement>("#track"),
  songTitle: query<HTMLElement>("#song-title"),
  songArtist: query<HTMLElement>("#song-artist"),
  players: query<HTMLSelectElement>("#players"),
  controls: query<HTMLElement>("#controls"),
  play: query<HTMLButtonElement>("#play"),
  time: query<HTMLElement>("#time"),
  idle: query<HTMLElement>("#idle"),
  idleHint: query<HTMLElement>("#idle-hint"),
  library: query<HTMLElement>("#library"),
  libraryList: query<HTMLElement>("#library-list"),
  libraryCount: query<HTMLElement>("#library-count"),
  librarySearch: query<HTMLInputElement>("#library-search"),
  libraryDetail: query<HTMLElement>("#library-detail"),
  libraryRefresh: query<HTMLButtonElement>("#library-refresh"),
  libraryClose: query<HTMLButtonElement>("#library-close"),
  offline: query<HTMLElement>("#offline"),
  toast: query<HTMLElement>("#toast"),
};

const emptyLyrics: Lyrics = {
  state: "none",
  signature: null,
  record_key: null,
  synced: false,
  offset: 0,
  source: null,
  title: "",
  artist: "",
  album: "",
  duration: null,
  text: "",
  lines: [],
  metadata: [],
  extras: [],
  lrclib_id: null,
  fetched_at: 0,
  updated_at: 0,
  original: null,
  error: null,
};

let language: Language = detectLanguage();
let data: Now = { track: null, players: [], lyrics: emptyLyrics };
let anchor: Anchor | null = null;
let lines: Lyrics["lines"] = [];
let lineEls: HTMLElement[] = [];
let activeIndex = -2;
let activeDone = false;
let plainMode = false;
let offsetSeconds = 0;
let lyricsKey = "";
let idleTimer = 0;
let chromeTimer = 0;
let toastTimer = 0;
let lastTimeText = "";
let tickScheduled = false;
let pollBusy = false;
let controlBusy = false;
let selectionBusy = false;
let offlineShown = false;
let libraryData: LibraryResponse | null = null;
let libraryOpener: HTMLElement | null = null;
let libraryRequest = 0;
let selectedKey: string | null = null;
let detailItem: LibraryItemResponse | null = null;
let detailBusy = false;

function trackKey(track: Track | null): string {
  return track ? `${track.player}|${track.track_id}|${track.title}|${track.artist}` : "";
}

/** Library refreshes only need the metadata identity, not a provider-specific track id. */
function trackMetadataKey(track: Track | null): string {
  return track ? `${track.player}|${track.title}|${track.artist}` : "";
}

function lineCountLabel(count: number): string {
  return count === 1 ? t(language, "library.lines_one") : t(language, "library.lines", { count });
}

function lyricsCacheKey(lyrics: Lyrics): string {
  return [
    lyrics.state,
    lyrics.record_key ?? "",
    lyrics.offset,
    lyrics.updated_at,
    lyrics.lines.length,
    lyrics.synced,
    lyrics.error ?? "",
  ].join("|");
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
  updateTrackTexts(data.track);
  updateIdleHint(data.players);
  updatePlayButton(data.track);
  updateTime(livePosition());
  rebuildLyrics(data.lyrics);
  if (selectedKey && detailItem) {
    renderDetail();
  } else if (libraryData) {
    renderLibrary(libraryData);
  }
}

function updateLanguageButtons(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-language]").forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.language === language));
  });
}

function updateTrackTexts(track: Track | null): void {
  els.songTitle.setAttribute("aria-level", track ? "1" : "2");
  if (!track) {
    els.songTitle.textContent = t(language, "app.title");
    els.songArtist.textContent = t(language, "player.waiting");
    return;
  }
  els.songTitle.textContent = track.title || t(language, "player.unknown_track");
  els.songArtist.textContent = [track.artist, track.label].filter(Boolean).join(" · ");
}

function updateIdleHint(players: PlayerInfo[]): void {
  const names = players.map((player) => player.label).filter(Boolean);
  els.idleHint.textContent = names.length
    ? t(language, "player.idle.players", { names: names.join(", ") })
    : "";
}

function updatePlayers(players: PlayerInfo[], track: Track | null): void {
  const current = track?.player ?? "";
  const existing = Array.from(els.players.options).map((option) => option.value);
  const next = players.map((player) => player.player);
  const changed =
    existing.length !== next.length || existing.some((value, index) => value !== next[index]);

  if (changed) {
    const options = players.map((player) => {
      const option = document.createElement("option");
      option.value = player.player;
      option.textContent = player.label || player.player;
      return option;
    });
    els.players.replaceChildren(...options);
  }

  const selected = current || players[0]?.player || "";
  if (selected && els.players.value !== selected) {
    els.players.value = selected;
  }
  els.players.hidden = players.length < 2;
  updateIdleHint(players);
}

function updatePlayButton(track: Track | null): void {
  els.play.disabled = !track;
  els.play.textContent = track?.playing ? "❚❚" : "▶";
}

function updateTime(position: number | null): void {
  const text = formatClock(position, data.track?.length ?? null);
  if (text !== lastTimeText) {
    lastTimeText = text;
    els.time.textContent = text;
  }
}

function rebuildLyrics(lyrics: Lyrics): void {
  lines = lyrics.lines;
  lineEls = [];
  activeIndex = -2;
  activeDone = false;
  offsetSeconds = Number.isFinite(lyrics.offset) ? lyrics.offset : 0;
  els.track.style.transform = "none";
  els.track.classList.remove("instant");

  if (lyrics.state !== "found" || lines.length === 0) {
    lineEls = [];
    const placeholder = document.createElement("p");
    placeholder.className = "placeholder";
    placeholder.textContent = placeholderText(lyrics);
    els.track.replaceChildren(placeholder);
    return;
  }

  const nodes = lines.map((line) => {
    const element = document.createElement("p");
    element.className = "line";
    const instrumental = isMusicLine(line.text);

    if (instrumental) {
      element.classList.add("instrumental");
    }

    const base = document.createElement("span");
    base.className = "text";
    base.textContent = line.text || "♪";
    element.append(base);

    if (lyrics.synced && !instrumental && !plainMode) {
      const fill = document.createElement("span");
      fill.className = "fill";
      fill.setAttribute("aria-hidden", "true");
      fill.textContent = line.text;
      element.append(fill);
    }

    lineEls.push(element);
    return element;
  });

  els.track.replaceChildren(...nodes);
  if (!plainMode) {
    const time = lyricTime();
    const index = time === null ? 0 : findActiveLine(lines, time);
    markLines(index, false);
    requestAnimationFrame(() => centerLine(index >= 0 ? index : 0, true));
  }
}

function placeholderText(lyrics: Lyrics): string {
  switch (lyrics.state) {
    case "loading":
      return t(language, "player.lyrics.loading");
    case "missing":
      return t(language, "player.lyrics.missing");
    case "error":
      return lyrics.error === "lyrics_pending"
        ? t(language, "player.lyrics.loading")
        : t(language, "player.lyrics.error");
    default:
      return t(language, "player.lyrics.idle");
  }
}

function lyricTime(): number | null {
  const position = livePosition();
  return position === null ? null : position + offsetSeconds;
}

function livePosition(): number | null {
  const position = positionAt(anchor, performance.now());
  const length = data.track?.length;
  if (position === null) {
    return null;
  }
  return typeof length === "number" && Number.isFinite(length) && length > 0
    ? Math.min(position, length)
    : position;
}

function ensureTick(): void {
  if (tickScheduled) {
    return;
  }
  tickScheduled = true;
  requestAnimationFrame(tick);
}

function updateLyricsFrame(): void {
  if (plainMode || !data.lyrics.synced || lines.length === 0) {
    return;
  }
  const time = lyricTime();
  if (time === null) {
    return;
  }

  const index = findActiveLine(lines, time);
  const line = index >= 0 ? lines[index] : null;
  const done = Boolean(
    line && typeof line.end === "number" && Number.isFinite(line.end) && time >= line.end,
  );

  if (index !== activeIndex || done !== activeDone) {
    activeIndex = index;
    activeDone = done;
    markLines(index, done);
    centerLine(index >= 0 ? index : 0);
  }

  if (index >= 0 && !done) {
    setFill(index, fillProgress(lines, index, data.track?.length ?? null, time));
  }
}

function tick(): void {
  tickScheduled = false;
  const position = livePosition();
  if (position !== null) {
    updateTime(position);
  }
  updateLyricsFrame();

  if (data.track?.playing && !document.hidden) {
    ensureTick();
  }
}

function markLines(index: number, done: boolean): void {
  lineEls.forEach((element, lineIndex) => {
    const past = lineIndex < index || (lineIndex === index && done);
    element.classList.toggle("active", lineIndex === index && !done);
    element.classList.toggle("past", past);
    if (past) {
      element.style.setProperty("--p", "100%");
    } else if (lineIndex !== index) {
      element.style.setProperty("--p", "0%");
    }
  });
}

function setFill(index: number, progress: number): void {
  lineEls[index]?.style.setProperty("--p", `${(progress * 100).toFixed(2)}%`);
}

function centerLine(index: number, instant = false): void {
  if (plainMode) {
    return;
  }
  const line = lineEls[Math.max(0, index)];
  if (!line) {
    return;
  }
  if (instant) {
    els.track.classList.add("instant");
    requestAnimationFrame(() => els.track.classList.remove("instant"));
  }
  const y = line.offsetTop + line.offsetHeight / 2;
  const center = els.viewport.clientHeight / 2;
  els.track.style.transform = `translateY(${center - y}px)`;
}

function applyNow(next: Now): void {
  const previous = data;
  data = next;
  const track = next.track;
  const now = performance.now();

  if (!track) {
    if (document.body.dataset.state === "playing") {
      renderPlayers(next.players, null);
      if (!idleTimer) {
        idleTimer = window.setTimeout(() => {
          idleTimer = 0;
          goIdle(data);
        }, 2000);
      }
      return;
    }
    goIdle(next);
    return;
  }

  clearTimeout(idleTimer);
  idleTimer = 0;
  document.body.dataset.playing = String(track.playing);
  if (document.body.dataset.state !== "playing") {
    document.body.dataset.state = "playing";
    els.controls.hidden = false;
    showChrome();
  }

  const sameTrack =
    previous.track !== null &&
    trackKey(previous.track) === trackKey(track) &&
    previous.track.playing === track.playing &&
    previous.track.rate === track.rate;
  const target = targetPosition({
    position: track.position,
    age: track.age,
    playing: track.playing,
    rate: track.rate,
    length: track.length,
  });

  if (target !== null) {
    anchor = updateAnchor(anchor, sameTrack, target, track.playing, track.rate, now);
  } else if (sameTrack && anchor) {
    anchor = { ...anchor, playing: track.playing, rate: track.rate };
  } else {
    anchor = { pos: 0, at: now, playing: false, rate: 1, ok: false };
  }

  const nextPlain = !next.lyrics.synced || track.position === null;
  if (nextPlain !== plainMode) {
    plainMode = nextPlain;
    els.stage.classList.toggle("plain", plainMode);
    lyricsKey = "";
  }

  updateTrackTexts(track);
  updatePlayers(next.players, track);
  updatePlayButton(track);
  updateTime(livePosition());

  if (trackMetadataKey(previous.track) !== trackMetadataKey(track) && !els.library.hidden) {
    if (selectedKey && !detailBusy) {
      void openDetail(selectedKey, false);
    } else if (!selectedKey) {
      void loadLibrary();
    }
  }

  const nextLyricsKey = lyricsCacheKey(next.lyrics);
  if (nextLyricsKey !== lyricsKey) {
    lyricsKey = nextLyricsKey;
    rebuildLyrics(next.lyrics);
  }
  updateLyricsFrame();

  if (track.playing) {
    ensureTick();
  }
}

function setOffline(value: boolean): void {
  els.offline.hidden = !value;
}

function goIdle(next: Now): void {
  clearTimeout(idleTimer);
  clearTimeout(chromeTimer);
  idleTimer = 0;
  anchor = null;
  plainMode = false;
  lyricsKey = "";
  lines = [];
  lineEls = [];
  els.stage.classList.remove("plain", "chrome-hidden");
  document.body.dataset.state = "idle";
  document.body.dataset.playing = "false";
  els.controls.hidden = true;
  updateTrackTexts(null);
  els.track.replaceChildren();
  els.track.style.transform = "none";
  renderPlayers(next.players, null);
}

function renderPlayers(players: PlayerInfo[], track: Track | null): void {
  updatePlayers(players, track);
}

function chromeHasFocus(): boolean {
  return (
    document.activeElement instanceof Element && document.activeElement.closest(".bar") !== null
  );
}

function showChrome(): void {
  els.stage.classList.remove("chrome-hidden");
  clearTimeout(chromeTimer);
  if (document.body.dataset.playing === "true" && els.library.hidden) {
    chromeTimer = window.setTimeout(() => {
      // Never hide the bar while one of its controls has keyboard focus.
      if (!chromeHasFocus()) {
        els.stage.classList.add("chrome-hidden");
      }
    }, 3000);
  }
}

async function poll(): Promise<void> {
  if (pollBusy) {
    return;
  }
  pollBusy = true;
  try {
    const next = await getNow();
    offlineShown = false;
    setOffline(false);
    applyNow(next);
  } catch (error) {
    setOffline(true);
    if (!offlineShown) {
      offlineShown = true;
      showToast(t(language, "player.offline"));
    }
    if (error instanceof ApiError) {
      // Keep the last view; the next poll may recover.
    }
  } finally {
    pollBusy = false;
  }
}

async function onPlay(): Promise<void> {
  if (controlBusy || !data.track) {
    return;
  }
  controlBusy = true;
  els.play.disabled = true;
  try {
    await togglePlayback();
    await poll();
  } catch (error) {
    showToast(errorMessage(language, error));
  } finally {
    controlBusy = false;
    updatePlayButton(data.track);
  }
}

async function onPlayerChange(): Promise<void> {
  if (selectionBusy) {
    return;
  }
  const player = els.players.value;
  if (!player) {
    return;
  }
  selectionBusy = true;
  els.players.disabled = true;
  try {
    await choosePlayer(player);
    await poll();
  } catch (error) {
    showToast(errorMessage(language, error));
  } finally {
    selectionBusy = false;
    els.players.disabled = false;
  }
}

async function toggleFullscreen(): Promise<void> {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await document.documentElement.requestFullscreen();
    }
  } catch {
    // Browsers may reject fullscreen without a direct user gesture; nothing to do.
  }
}

function showToast(message: string): void {
  els.toast.textContent = message;
  els.toast.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    els.toast.classList.remove("show");
    els.toast.textContent = "";
  }, 4200);
}

async function openLibrary(): Promise<void> {
  if (detailBusy) {
    return;
  }
  libraryOpener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  selectedKey = null;
  detailItem = null;
  els.librarySearch.value = "";
  clearTimeout(chromeTimer);
  els.library.hidden = false;
  els.stage.inert = true;
  showLibraryList();
  els.libraryList.scrollTop = 0;
  els.libraryDetail.scrollTop = 0;
  els.libraryClose.focus();
  await loadLibrary();
}

function closeLibrary(): void {
  if (els.library.hidden || detailBusy) {
    return;
  }
  els.library.hidden = true;
  els.stage.inert = false;
  selectedKey = null;
  detailItem = null;
  showChrome();
  const opener = libraryOpener;
  libraryOpener = null;
  opener?.focus();
}

function showLibraryList(): void {
  els.librarySearch.hidden = false;
  els.libraryList.hidden = false;
  els.libraryDetail.hidden = true;
}

async function refreshLibrary(): Promise<void> {
  if (detailBusy) {
    return;
  }
  await loadLibrary();
  if (selectedKey) {
    await openDetail(selectedKey);
  }
}

async function loadLibrary(): Promise<void> {
  const request = ++libraryRequest;
  els.libraryCount.textContent = t(language, "player.library.loading");
  try {
    const data = await getLibrary();
    if (request !== libraryRequest) {
      return;
    }
    libraryData = data;
    if (selectedKey) {
      els.libraryCount.textContent = t(language, "player.library.count", {
        count: data.items.length,
      });
    } else {
      renderLibrary(data);
    }
  } catch (error) {
    if (request !== libraryRequest) {
      return;
    }
    // A stale list must not reappear when the search box is used after a failed refresh.
    libraryData = null;
    els.libraryCount.textContent = errorMessage(language, error);
    const hint = document.createElement("p");
    hint.className = "hint library-empty";
    hint.textContent = errorMessage(language, error);
    els.libraryList.replaceChildren(hint);
  }
}

function renderLibrary(library: LibraryResponse): void {
  showLibraryList();
  els.libraryCount.textContent = t(language, "player.library.count", {
    count: library.items.length,
  });

  const query = els.librarySearch.value.trim().toLocaleLowerCase();
  const items = library.items.filter((item) => matchesLibraryQuery(item, query));

  els.libraryCount.textContent = t(language, "player.library.count", {
    count: items.length,
  });

  if (items.length === 0) {
    const empty = document.createElement("p");
    empty.className = "hint library-empty";
    empty.textContent = t(
      language,
      query && library.items.length > 0 ? "player.library.no_results" : "player.library.empty",
    );
    els.libraryList.replaceChildren(empty);
    return;
  }

  const nodes = items.map((item) => libraryItem(item));
  els.libraryList.replaceChildren(...nodes);
}

function libraryItem(item: LibrarySummary): HTMLElement {
  const card = document.createElement("button");
  card.type = "button";
  card.className = "library-item";
  card.dataset.key = item.key;
  if (item.current) {
    card.classList.add("current");
  }
  card.addEventListener("click", () => {
    void openDetail(item.key);
  });

  const title = document.createElement("span");
  title.className = "library-item-title";
  title.textContent = item.title || t(language, "player.unknown_track");

  const artist = document.createElement("span");
  artist.className = "library-item-artist";
  artist.textContent = [item.artist, item.album].filter(Boolean).join(" · ");

  const meta = document.createElement("span");
  meta.className = "library-item-meta";
  const parts = [
    t(language, sourceLabelKey(item.source)),
    item.synced ? t(language, "library.synced") : t(language, "library.plain"),
    lineCountLabel(item.line_count),
    formatDate(item.updated_at),
  ];
  if (item.has_original) {
    parts.push(t(language, "player.library.original_available"));
  }
  if (item.current) {
    parts.unshift(t(language, "player.library.current"));
  }
  meta.textContent = parts.filter(Boolean).join(" · ");

  card.append(title, artist, meta);
  return card;
}

async function openDetail(key: string, focusTitle = true): Promise<void> {
  if (detailBusy) {
    return;
  }
  selectedKey = key;
  detailItem = null;
  els.librarySearch.hidden = true;
  els.libraryList.hidden = true;
  els.libraryDetail.hidden = false;
  els.libraryDetail.replaceChildren(hintNode(t(language, "player.library.loading")));

  try {
    const item = await getLibraryItem(key);
    if (selectedKey !== key) {
      return;
    }
    detailItem = item;
    renderDetail(focusTitle);
  } catch (error) {
    if (selectedKey === key) {
      renderDetailError(errorMessage(language, error));
    }
  }
}

function renderDetail(focusTitle = true): void {
  if (!selectedKey || !detailItem) {
    return;
  }
  const item = detailItem;
  const lyrics = item.lyrics;
  els.libraryDetail.replaceChildren();
  els.librarySearch.hidden = true;
  els.libraryList.hidden = true;
  els.libraryDetail.hidden = false;

  const back = actionButton(t(language, "player.library.back"), () => {
    void backToLibrary();
  });
  back.classList.add("detail-back");
  els.libraryDetail.append(back);

  const title = document.createElement("h3");
  title.className = "detail-title";
  title.tabIndex = -1;
  title.textContent = lyrics.title || t(language, "player.unknown_track");
  els.libraryDetail.append(title);

  const artist = document.createElement("p");
  artist.className = "detail-artist";
  artist.textContent = [lyrics.artist, lyrics.album].filter(Boolean).join(" · ");
  els.libraryDetail.append(artist);

  const meta = document.createElement("p");
  meta.className = "detail-meta";
  meta.textContent = [
    item.current ? t(language, "player.library.current") : "",
    t(language, sourceLabelKey(lyrics.source ?? "manual")),
    lyrics.synced ? t(language, "library.synced") : t(language, "library.plain"),
    lineCountLabel(lyrics.lines.length),
    lyrics.duration ? formatTime(lyrics.duration) : "",
    formatDate(lyrics.updated_at),
    lyrics.offset ? t(language, "player.library.offset", { value: lyrics.offset }) : "",
    lyrics.original ? t(language, "player.library.original_available") : "",
  ]
    .filter(Boolean)
    .join(" · ");
  els.libraryDetail.append(meta);

  const preview = document.createElement("pre");
  preview.className = "detail-preview";
  preview.textContent = lyrics.text;
  els.libraryDetail.append(preview);

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  const studio = document.createElement("a");
  studio.className = "ghost primary detail-link";
  studio.href = `/editor?key=${encodeURIComponent(item.lyrics.record_key ?? selectedKey ?? "")}`;
  studio.textContent = t(language, "player.library.studio");
  actions.append(
    studio,
    externalLink(t(language, "library.youtube"), youtubeSearchUrl(lyrics.title, lyrics.artist)),
    externalLink(t(language, "library.spotify"), spotifySearchUrl(lyrics.title, lyrics.artist)),
  );

  const remove = actionButton(t(language, "player.library.delete"), () => {
    void deleteDetail();
  });
  remove.classList.add("danger");
  actions.append(remove);
  els.libraryDetail.append(actions);

  // A background refresh must not steal focus from the control the user is using.
  if (focusTitle) {
    title.focus();
  }
}

function setDetailBusy(busy: boolean): void {
  detailBusy = busy;
  els.libraryDetail.classList.toggle("busy", busy);
}

async function backToLibrary(): Promise<void> {
  if (detailBusy) {
    return;
  }
  const key = selectedKey;
  selectedKey = null;
  detailItem = null;
  showLibraryList();
  if (libraryData) {
    renderLibrary(libraryData);
  }
  const card = key
    ? Array.from(els.libraryList.querySelectorAll<HTMLElement>(".library-item")).find(
        (element) => element.dataset.key === key,
      )
    : undefined;
  (card ?? els.libraryClose).focus();
}

async function deleteDetail(): Promise<void> {
  if (!selectedKey || !detailItem || detailBusy) {
    return;
  }
  const key = selectedKey;
  const title = detailItem.lyrics.title || t(language, "player.unknown_track");
  if (!window.confirm(t(language, "player.library.delete_confirm", { title }))) {
    return;
  }
  setDetailBusy(true);
  try {
    await deleteLibraryItem(key);
    if (selectedKey === key) {
      selectedKey = null;
      detailItem = null;
      showLibraryList();
    }
    await loadLibrary();
    if (!selectedKey) {
      const first = els.libraryList.querySelector<HTMLButtonElement>(".library-item");
      (first ?? els.libraryClose).focus();
    }
    showToast(t(language, "player.library.deleted"));
  } catch (error) {
    showToast(errorMessage(language, error));
  } finally {
    setDetailBusy(false);
  }
}

function renderDetailError(message: string): void {
  els.libraryDetail.replaceChildren();
  const back = actionButton(t(language, "player.library.back"), () => {
    void backToLibrary();
  });
  back.classList.add("detail-back");
  els.libraryDetail.append(back, hintNode(message));
  back.focus();
}

function actionButton(label: string, action: () => void, className = ""): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `ghost ${className}`.trim();
  button.textContent = label;
  button.addEventListener("click", action);
  return button;
}

function externalLink(label: string, href: string): HTMLAnchorElement {
  const link = document.createElement("a");
  link.className = "ghost detail-link";
  link.href = href;
  link.target = "_blank";
  link.rel = "noreferrer";
  link.textContent = label;
  return link;
}

function hintNode(text: string): HTMLParagraphElement {
  const hint = document.createElement("p");
  hint.className = "hint";
  hint.textContent = text;
  return hint;
}

function formatDate(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return "";
  }
  try {
    return new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(
      new Date(seconds * 1000),
    );
  } catch {
    return "";
  }
}

function bind(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-language]").forEach((button) => {
    button.addEventListener("click", () => {
      const next = button.dataset.language;
      if (next === "en" || next === "nl") {
        setLanguage(next);
      }
    });
  });

  document.querySelector<HTMLButtonElement>("#library-btn")?.addEventListener("click", () => {
    void openLibrary();
  });
  document.querySelector<HTMLButtonElement>("#library-idle")?.addEventListener("click", () => {
    void openLibrary();
  });
  els.libraryClose.addEventListener("click", closeLibrary);
  els.librarySearch.addEventListener("input", () => {
    if (libraryData && !selectedKey) {
      renderLibrary(libraryData);
    }
  });
  els.libraryRefresh.addEventListener("click", () => {
    void refreshLibrary();
  });
  els.library.addEventListener("click", (event) => {
    if (event.target === els.library) {
      closeLibrary();
    }
  });
  els.library.addEventListener("keydown", (event) => {
    if (event.key !== "Tab") {
      return;
    }
    const focusables = Array.from(
      els.library.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      ),
    ).filter(
      (element) =>
        !element.hasAttribute("disabled") &&
        !element.closest("[hidden]") &&
        element.getClientRects().length > 0,
    );
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
  });
  document.querySelector<HTMLButtonElement>("#fullscreen-btn")?.addEventListener("click", () => {
    void toggleFullscreen();
  });
  els.play.addEventListener("click", () => {
    void onPlay();
  });
  els.players.addEventListener("change", () => {
    void onPlayerChange();
  });

  const recenter = () => {
    if (plainMode) {
      return;
    }
    if (activeIndex >= 0) {
      centerLine(activeIndex, true);
    } else if (lineEls.length > 0) {
      centerLine(0, true);
    }
  };
  window.addEventListener("resize", recenter);
  window.addEventListener("orientationchange", recenter);
  document.addEventListener("fullscreenchange", recenter);
  window.addEventListener("mousemove", showChrome, { passive: true });
  window.addEventListener("pointerdown", showChrome, { passive: true });
  window.addEventListener("touchstart", showChrome, { passive: true });
  window.addEventListener("keydown", (event) => {
    showChrome();
    if (event.key === "Escape") {
      closeLibrary();
      return;
    }
    if (!els.library.hidden) {
      return;
    }
    const target = event.target;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable)
    ) {
      return;
    }
    if (event.code === "Space") {
      if (target instanceof HTMLButtonElement || target instanceof HTMLSelectElement) {
        return;
      }
      event.preventDefault();
      void onPlay();
    } else if (
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey &&
      event.key.toLowerCase() === "f"
    ) {
      event.preventDefault();
      void toggleFullscreen();
    }
  });

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      void poll();
      showChrome();
    }
  });
  document.addEventListener("focusin", () => {
    showChrome();
  });
  document.addEventListener("focusout", () => {
    showChrome();
  });
}

bind();
setLanguage(language, false);
document.body.dataset.state = "idle";
void poll();
window.setInterval(() => {
  if (!document.hidden) {
    void poll();
  }
}, 400);
