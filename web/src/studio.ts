import "./styles.css";
import "./studio.css";

import {
  fetchLyrics,
  getLibraryItem,
  getNow,
  parseLyrics,
  restoreLibraryOriginal,
  saveLibraryItem,
  seekTo as seekPlayer,
  setPlayback,
  togglePlayback,
  type LibraryItemResponse,
  type Now,
} from "./api";
import { applyTranslations, detectLanguage, rememberLanguage, t, type Language } from "./i18n";
import { errorMessage, sourceLabelKey } from "./messages";
import { formatTime, positionAt, targetPosition, updateAnchor, type Anchor } from "./player";
import {
  deleteLine,
  insertLine,
  insertMusicMarker,
  isMusicMarker,
  makeEditorLines,
  nudgeLine,
  parseTimecode,
  popUndo,
  pushUndo,
  roundTime,
  serializeLines,
  sortLines,
  takeTime,
  timelineDuration,
  toggleMusic,
  updateLine,
  type EditorLine,
  type EditorSnapshot,
} from "./studio/model";
import {
  DEFAULT_TAP_CORRECTION,
  backspaceTap,
  correctedTapTime,
  nextTap,
  parseCorrection,
  startTap,
  type TapSession,
} from "./studio/tap";
import {
  DEFAULT_SCALE,
  canvasWidth,
  clampScale,
  fitScale,
  rulerTicks,
  timeAtX,
  xAtTime,
} from "./studio/timeline";
import {
  renderInspector,
  renderLines,
  renderRuler,
  renderTimeline,
  scrollLineIntoView,
  updateInspectorTime,
  updateLineRowSelection,
  updateLineTimeInput,
  updateMarkerSelection,
  updateMarkerTime,
  updatePlayhead,
  type TimelineElements,
} from "./studio/view";

function query<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) {
    throw new Error(`Missing element: ${selector}`);
  }
  return element;
}

const els = {
  studio: query<HTMLElement>("#studio"),
  loading: query<HTMLElement>("#studio-loading"),
  back: query<HTMLAnchorElement>("#studio-back"),
  title: query<HTMLElement>("#studio-title"),
  subtitle: query<HTMLElement>("#studio-subtitle"),
  saveState: query<HTMLElement>("#studio-save-state"),
  undo: query<HTMLButtonElement>("#studio-undo"),
  save: query<HTMLButtonElement>("#studio-save"),
  play: query<HTMLButtonElement>("#studio-play"),
  time: query<HTMLElement>("#studio-time"),
  duration: query<HTMLElement>("#studio-duration"),
  fetch: query<HTMLButtonElement>("#studio-fetch"),
  restore: query<HTMLButtonElement>("#studio-restore"),
  zoom: query<HTMLInputElement>("#timeline-zoom"),
  zoomValue: query<HTMLElement>("#timeline-zoom-value"),
  fit: query<HTMLButtonElement>("#timeline-fit"),
  scroll: query<HTMLElement>("#timeline-scroll"),
  canvas: query<HTMLElement>("#timeline-canvas"),
  ruler: query<HTMLElement>("#timeline-ruler"),
  markers: query<HTMLElement>("#timeline-markers"),
  playhead: query<HTMLElement>("#timeline-playhead"),
  lineCount: query<HTMLElement>("#line-count"),
  lineList: query<HTMLOListElement>("#line-list"),
  linesEmpty: query<HTMLElement>("#lines-empty"),
  addLine: query<HTMLButtonElement>("#add-line"),
  addMusic: query<HTMLButtonElement>("#add-music"),
  inspector: query<HTMLElement>("#inspector-body"),
  tapOverlay: query<HTMLElement>("#tap-overlay"),
  tapHint: query<HTMLElement>("#tap-hint"),
  tapCountdown: query<HTMLElement>("#tap-countdown"),
  tapLine: query<HTMLElement>("#tap-line"),
  tapProgress: query<HTMLElement>("#tap-progress"),
  tapCorrection: query<HTMLInputElement>("#tap-correction"),
  tapStart: query<HTMLButtonElement>("#tap-start"),
  tapTap: query<HTMLButtonElement>("#tap-tap"),
  tapStop: query<HTMLButtonElement>("#tap-stop"),
  rawPanel: query<HTMLDetailsElement>("#raw-panel"),
  rawText: query<HTMLTextAreaElement>("#raw-text"),
  rawApply: query<HTMLButtonElement>("#raw-apply"),
  rawReset: query<HTMLButtonElement>("#raw-reset"),
  notice: query<HTMLElement>("#studio-notice"),
  toast: query<HTMLElement>("#toast"),
};

const timelineElements: TimelineElements = {
  scroll: els.scroll,
  canvas: els.canvas,
  ruler: els.ruler,
  markers: els.markers,
  playhead: els.playhead,
};

let language: Language = detectLanguage();
let booted = false;
let key: string | null = null;
let item: LibraryItemResponse | null = null;
let now: Now | null = null;
let lines: EditorLine[] = [];
let selectedId: string | null = null;
let nextId = 1;
let dirty = false;
let busy = false;
let fetching = false;
let migratedNotice = false;
let undoStack: EditorSnapshot[] = [];
let scale = DEFAULT_SCALE;
let editSnapshot: EditorSnapshot | null = null;
let anchor: Anchor | null = null;
let anchorTrack = "";
let pollBusy = false;
let previewTimer: number | null = null;
let previewing = false;
let toastTimer: number | undefined;
let fatalMessage: string | null = null;

type TapPhase = "ready" | "countdown" | "tapping";

let tapPhase: TapPhase | null = null;
let tapSession: TapSession = startTap();
let tapOriginals: (number | null)[] = [];
let tapSnapshot: EditorSnapshot | null = null;
let tapTimer: number | null = null;

async function boot(): Promise<void> {
  applyLanguage(language, false);
  bindEvents();
  booted = true;
  els.loading.hidden = true;
  els.studio.hidden = false;

  const requested = new URLSearchParams(window.location.search).get("key");
  if (requested) {
    await refreshNow().catch(() => undefined);
    await loadItem(requested);
  } else {
    await loadCurrent();
  }

  window.setInterval(() => {
    void poll();
  }, 1000);
  window.requestAnimationFrame(frame);
}

function applyLanguage(next: Language, persist: boolean): void {
  language = next;
  document.documentElement.lang = next;
  document.title = t(next, "studio.page_title");
  applyTranslations(document, next);
  updateLanguageButtons();
  if (persist) {
    rememberLanguage(next);
  }
  if (booted) {
    renderStructure();
    if (tapPhase !== null) {
      updateTapUI();
    }
  }
}

function updateLanguageButtons(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-language]").forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.language === language));
  });
}

async function loadCurrent(): Promise<void> {
  try {
    const state = await getNow();
    now = state;
    updateAnchorFrom(state);
    if (state.lyrics.record_key) {
      await loadItem(state.lyrics.record_key);
      return;
    }
    key = null;
    item = null;
    lines = [];
    selectedId = null;
    dirty = false;
    migratedNotice = false;
    undoStack = [];
    renderStructure();
    els.playhead.hidden = true;
  } catch (error) {
    showFatal(errorMessage(language, error));
  }
}

async function loadItem(itemKey: string): Promise<void> {
  try {
    const result = await getLibraryItem(itemKey);
    applyItem(result);
    els.playhead.hidden = false;
  } catch (error) {
    showFatal(errorMessage(language, error));
  }
}

function applyItem(result: LibraryItemResponse): void {
  fatalMessage = null;
  const recordKey = result.lyrics.record_key;
  key = recordKey ?? key;
  item = result;
  const adopted = makeEditorLines(result.lyrics.lines);
  lines = adopted.lines;
  nextId = adopted.nextId;
  selectedId = lines[0]?.id ?? null;
  undoStack = [];
  editSnapshot = null;
  dirty = adopted.migrated > 0;
  migratedNotice = adopted.migrated > 0;
  renderStructure();
  syncRawText();
  centerTimelineOnLine(selectedId);
  flushPreview();
}

function showFatal(message: string): void {
  fatalMessage = message;
  key = null;
  item = null;
  lines = [];
  selectedId = null;
  dirty = false;
  renderStructure();
  showNotice(message);
}

function renderStructure(): void {
  renderHeader();
  renderLineList();
  renderTimelineView();
  renderInspectorView();
  updatePlayhead(els.playhead, livePosition(), scale);
  renderTransport();
}

function renderHeader(): void {
  const lyrics = item?.lyrics;
  const track = fatalMessage ? null : now?.track;
  const title = lyrics?.title || track?.title || t(language, "studio.title.unknown");
  els.title.textContent = title;

  const parts = [
    lyrics?.artist || track?.artist || "",
    lyrics?.album || "",
    lyrics ? t(language, sourceLabelKey(lyrics.source ?? "manual")) : "",
    lyrics ? (lyrics.synced ? t(language, "library.synced") : t(language, "library.plain")) : "",
    lyrics?.original ? t(language, "player.library.original_available") : "",
  ].filter(Boolean);
  els.subtitle.textContent = parts.join(" · ");

  const text = !item
    ? ""
    : busy
      ? t(language, "studio.saving")
      : dirty
        ? t(language, "studio.unsaved")
        : t(language, "studio.saved");
  els.saveState.textContent = text;
  els.saveState.classList.toggle("dirty", dirty && !busy);

  els.save.disabled = !key || !dirty || busy;
  els.undo.disabled = undoStack.length === 0 || busy;
  els.fetch.disabled = busy || fetching || (!key && !now?.track);
  els.fetch.textContent = t(language, fetching ? "studio.fetching" : "studio.fetch");
  els.restore.disabled = !item?.lyrics.original || busy;

  updateNotice();
}

function updateNotice(): void {
  if (fatalMessage) {
    els.notice.hidden = false;
    els.notice.textContent = fatalMessage;
    return;
  }
  let message = "";
  if (migratedNotice) {
    message = t(language, "studio.notice.migrated");
  } else if (!now?.track) {
    message = t(language, "studio.notice.no_track");
  } else if (!key) {
    message = t(language, "studio.notice.current_missing");
  } else if (!isCurrent()) {
    message = t(language, "studio.notice.not_current");
  }
  if (message) {
    els.notice.hidden = false;
    els.notice.textContent = message;
  } else {
    els.notice.hidden = true;
    els.notice.textContent = "";
  }
}

function renderTransport(): void {
  const current = isCurrent();
  const track = now?.track;
  const playing = current && Boolean(track?.playing);
  els.play.disabled = !current || !track?.can_play;
  els.play.textContent = playing ? "❚❚" : "▶";
  const label = t(language, playing ? "studio.pause" : "studio.play");
  els.play.title = label;
  els.play.setAttribute("aria-label", label);
  els.time.textContent = current
    ? formatTime(livePosition() ?? 0)
    : t(language, "studio.not_playing");
  const length = duration();
  els.duration.textContent =
    length === null ? t(language, "studio.not_playing") : formatTime(length);
}

function renderLineList(): void {
  const count = lines.length;
  els.lineCount.textContent = t(language, "studio.line_count", { count });
  els.linesEmpty.hidden = count > 0;
  renderLines(els.lineList, lines, selectedId, language, lineHandlers);
}

function renderTimelineView(): void {
  const length = timelineLength();
  const width = canvasWidth(length, scale, els.scroll.clientWidth);
  renderRuler(timelineElements, rulerTicks(length, scale), scale);
  renderTimeline(timelineElements, lines, selectedId, scale, width, language, {
    select: (id) => selectLine(id, true),
    dragStart: startDrag,
  });
  els.zoom.value = String(Math.round(scale));
  els.zoomValue.textContent = t(language, "studio.zoom_value", {
    value: Math.round(scale),
  });
}

function renderInspectorView(): void {
  const selected = lines.find((line) => line.id === selectedId) ?? null;
  renderInspector(
    els.inspector,
    {
      line: selected,
      lines,
      language,
      canTakeTime: isCurrent() && livePosition() !== null,
      canPreview: isCurrent() && Boolean(now?.track?.can_seek),
      previewing,
      readOnly: false,
    },
    inspectorHandlers,
  );
}

function updateDirty(): void {
  dirty = true;
  renderHeader();
}

async function save(): Promise<void> {
  if (!key || busy) {
    return;
  }
  const text = serializeLines(lines);
  if (!text.trim()) {
    showToast(t(language, "player.error.lyrics_empty"));
    return;
  }
  busy = true;
  renderHeader();
  try {
    const updated = await saveLibraryItem(key, text);
    item = { ...updated, current: isCurrent() };
    dirty = false;
    migratedNotice = false;
    undoStack = [];
    renderStructure();
    focusRow(selectedId);
    showToast(t(language, "player.library.saved"));
  } catch (error) {
    showToast(errorMessage(language, error));
  } finally {
    busy = false;
    renderHeader();
  }
}

function undo(): void {
  if (busy) {
    return;
  }
  const popped = popUndo(undoStack);
  if (!popped.snapshot) {
    return;
  }
  undoStack = popped.stack;
  lines = popped.snapshot.lines;
  selectedId = popped.snapshot.selectedId;
  updateDirty();
  renderStructure();
  scrollLineIntoView(els.lineList, selectedId);
  focusRow(selectedId);
}

function selectLine(id: string, seek: boolean): void {
  selectedId = id;
  const line = lines.find((candidate) => candidate.id === id);
  updateLineRowSelection(els.lineList, selectedId);
  updateMarkerSelection(els.markers, selectedId);
  renderInspectorView();
  scrollLineIntoView(els.lineList, selectedId);
  if (seek && line && line.t !== null) {
    centerTimelineOnLine(id);
    void seekToTime(line.t);
  }
}

function startDrag(id: string, event: PointerEvent): void {
  event.preventDefault();
  const line = lines.find((candidate) => candidate.id === id);
  if (!line || line.t === null) {
    return;
  }
  const before = snapshot();
  const startX = event.clientX;
  const startTime = line.t;
  let moved = false;
  document.body.classList.add("dragging");
  selectedId = id;
  updateLineRowSelection(els.lineList, selectedId);
  updateMarkerSelection(els.markers, selectedId);
  renderInspectorView();

  const onMove = (move: PointerEvent) => {
    if (!moved && Math.abs(move.clientX - startX) < 2) {
      return;
    }
    moved = true;
    const nextTime = roundTime(
      clampTime(timeAtX(xAtTime(startTime, scale) + move.clientX - startX, scale)),
    );
    lines = updateLine(lines, id, { t: nextTime }, false);
    updateMarkerTime(els.markers, id, nextTime, scale);
    updateLineTimeInput(els.lineList, id, nextTime);
    updateInspectorTime(els.inspector, nextTime);
    dirty = true;
    renderHeader();
  };

  const onUp = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    document.body.classList.remove("dragging");
    if (moved) {
      undoStack = pushUndo(undoStack, before);
      lines = sortLines(lines);
      renderStructure();
      scrollLineIntoView(els.lineList, selectedId);
    } else {
      selectLine(id, true);
    }
  };

  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

function addLine(): void {
  const time = insertionTime();
  const id = `line-${nextId}`;
  nextId += 1;
  undoStack = pushUndo(undoStack, snapshot());
  const added = insertLine(lines, id, time);
  lines = added.lines;
  selectedId = added.id;
  updateDirty();
  renderStructure();
  scrollLineIntoView(els.lineList, selectedId);
  focusRow(id);
}

function addMusic(): void {
  const position = editablePosition();
  if (position === null) {
    showToast(t(language, "player.error.no_position"));
    return;
  }
  const before = snapshot();
  const added = insertMusicMarker(lines, `line-${nextId}`, roundTime(position));
  if (!added.created) {
    selectLine(added.id, true);
    return;
  }
  nextId += 1;
  undoStack = pushUndo(undoStack, before);
  lines = added.lines;
  selectedId = added.id;
  updateDirty();
  renderStructure();
  scrollLineIntoView(els.lineList, selectedId);
  focusRow(added.id);
}

function commitLineTime(id: string, value: string): void {
  const time = parseTimecode(value);
  const row = els.lineList.querySelector<HTMLElement>(`.line-row[data-id="${id}"]`);
  if (time === null) {
    row?.querySelector(".line-time")?.setAttribute("aria-invalid", "true");
    showToast(t(language, "studio.invalid_time"));
    return;
  }
  row?.querySelector(".line-time")?.removeAttribute("aria-invalid");
  undoStack = pushUndo(undoStack, snapshot());
  editSnapshot = null;
  lines = takeTime(lines, id, time);
  selectedId = id;
  updateDirty();
  renderStructure();
  scrollLineIntoView(els.lineList, id);
  focusRow(id);
}

function setLineText(id: string, value: string): void {
  lines = updateLine(lines, id, { text: value });
  dirty = true;
  renderTimelineView();
  renderInspectorView();
  renderHeader();
}

function removeLine(id: string): void {
  const index = lines.findIndex((line) => line.id === id);
  undoStack = pushUndo(undoStack, snapshot());
  const remaining = deleteLine(lines, id);
  const next = remaining[Math.min(index, remaining.length - 1)];
  lines = remaining;
  selectedId = next?.id ?? null;
  updateDirty();
  renderStructure();
  if (selectedId) {
    focusRow(selectedId);
  }
}

function beginEdit(id: string): void {
  if (!editSnapshot) {
    editSnapshot = snapshot();
  }
  void id;
}

function endEdit(id: string): void {
  if (!editSnapshot) {
    return;
  }
  const changed =
    editSnapshot.selectedId !== selectedId ||
    editSnapshot.lines.length !== lines.length ||
    editSnapshot.lines.some((line, index) => {
      const current = lines[index];
      return (
        !current || line.id !== current.id || line.t !== current.t || line.text !== current.text
      );
    });
  if (changed) {
    undoStack = pushUndo(undoStack, editSnapshot);
  }
  editSnapshot = null;
  void id;
}

const lineHandlers = {
  select: (id: string) => selectLine(id, false),
  seek: (id: string) => selectLine(id, true),
  commitTime: commitLineTime,
  setText: setLineText,
  remove: removeLine,
  editStart: beginEdit,
  editEnd: endEdit,
};

const inspectorHandlers = {
  nudge: (delta: number) => {
    if (!selectedId) {
      return;
    }
    undoStack = pushUndo(undoStack, snapshot());
    lines = nudgeLine(lines, selectedId, delta);
    updateDirty();
    renderStructure();
    scrollLineIntoView(els.lineList, selectedId);
    focusRow(selectedId);
  },
  takeTime: () => {
    const position = editablePosition();
    if (!selectedId || position === null) {
      showToast(t(language, "player.error.no_position"));
      return;
    }
    undoStack = pushUndo(undoStack, snapshot());
    lines = takeTime(lines, selectedId, position);
    updateDirty();
    renderStructure();
    scrollLineIntoView(els.lineList, selectedId);
    focusRow(selectedId);
  },
  preview: () => {
    void previewLine();
  },
  toggleMusic: () => {
    if (!selectedId) {
      return;
    }
    undoStack = pushUndo(undoStack, snapshot());
    lines = toggleMusic(lines, selectedId);
    updateDirty();
    renderStructure();
  },
  remove: () => {
    if (selectedId) {
      removeLine(selectedId);
    }
  },
  tap: () => openTap(),
};

async function previewLine(): Promise<void> {
  const line = lines.find((candidate) => candidate.id === selectedId);
  if (!line || line.t === null) {
    return;
  }
  if (!isCurrent() || !now?.track?.can_seek) {
    showToast(t(language, "studio.error.no_seek"));
    return;
  }
  flushPreview();
  previewing = true;
  renderInspectorView();
  const rate = now?.track?.rate ?? 1;
  const start = Math.max(0, line.t - 1.5);
  try {
    await setPlayback("pause");
    await seekPlayer(start);
    anchor = {
      pos: start,
      at: performance.now(),
      playing: true,
      rate,
      ok: true,
    };
    await setPlayback("play");
    previewTimer = window.setTimeout(() => {
      stopAnchor();
      void setPlayback("pause")
        .then(() => refreshNow())
        .catch(() => undefined);
      previewing = false;
      previewTimer = null;
      renderInspectorView();
    }, 3000);
  } catch (error) {
    previewing = false;
    showToast(errorMessage(language, error));
    renderInspectorView();
  }
}

function flushPreview(): void {
  if (previewTimer !== null) {
    window.clearTimeout(previewTimer);
    previewTimer = null;
  }
  if (previewing) {
    previewing = false;
    stopAnchor();
    void setPlayback("pause").catch(() => undefined);
  }
}

function openTap(): void {
  if (!isCurrent()) {
    showToast(t(language, "studio.notice.not_current"));
    return;
  }
  if (!now?.track?.can_seek) {
    showToast(t(language, "studio.tap.needs_seek"));
    return;
  }
  if (lines.length === 0) {
    showToast(t(language, "studio.tap.no_lines"));
    return;
  }
  tapPhase = "ready";
  tapSession = startTap();
  tapOriginals = lines.map((line) => line.t);
  tapSnapshot = snapshot();
  els.tapCorrection.value = storedCorrection().toString();
  els.tapOverlay.hidden = false;
  setTapBackgroundInert(true);
  updateTapUI();
  els.tapStart.focus();
}

function startTapRun(): void {
  if (tapPhase !== "ready") {
    return;
  }
  tapPhase = "countdown";
  let count = 3;
  updateTapUI();
  els.tapCountdown.textContent = String(count);
  tapTimer = window.setInterval(() => {
    count -= 1;
    if (count > 0) {
      els.tapCountdown.textContent = String(count);
      return;
    }
    if (count === 0) {
      els.tapCountdown.textContent = t(language, "studio.tap.go");
      return;
    }
    if (tapTimer !== null) {
      window.clearInterval(tapTimer);
      tapTimer = null;
    }
    tapPhase = "tapping";
    updateTapUI();
    els.tapTap.focus();
    void beginTapping();
  }, 1000);
}

async function beginTapping(): Promise<void> {
  try {
    await seekPlayer(0);
    anchor = {
      pos: 0,
      at: performance.now(),
      playing: true,
      rate: now?.track?.rate ?? 1,
      ok: true,
    };
    await setPlayback("play");
  } catch (error) {
    showToast(errorMessage(language, error));
    finishTap(false);
  }
}

function performTap(): void {
  if (tapPhase !== "tapping") {
    return;
  }
  const position = livePosition();
  if (position === null) {
    showToast(t(language, "player.error.no_position"));
    return;
  }
  const next = nextTap(tapSession, lines.length);
  if (!next) {
    finishTap(true);
    return;
  }
  const line = lines[next.index];
  if (!line) {
    finishTap(true);
    return;
  }
  const correction = parseCorrection(els.tapCorrection.value);
  const time = correctedTapTime(position, correction, duration());
  tapSession = next.session;
  lines = updateLine(lines, line.id, { t: time }, false);
  selectedId = line.id;
  dirty = true;
  renderHeader();
  renderTimelineView();
  updateLineRowSelection(els.lineList, selectedId);
  updateLineTimeInput(els.lineList, line.id, time);
  updateTapUI();
  if (tapSession.index >= lines.length) {
    finishTap(true);
  }
}

function tapBackspace(): void {
  if (tapPhase !== "tapping") {
    return;
  }
  const back = backspaceTap(tapSession);
  if (!back) {
    return;
  }
  tapSession = back.session;
  const line = lines[back.index];
  const original = tapOriginals[back.index];
  if (line) {
    lines = updateLine(lines, line.id, { t: original ?? null }, false);
    selectedId = line.id;
    updateLineTimeInput(els.lineList, line.id, original ?? null);
    renderTimelineView();
  }
  dirty = true;
  renderHeader();
  updateTapUI();
}

function finishTap(completed: boolean): void {
  if (tapTimer !== null) {
    window.clearInterval(tapTimer);
    tapTimer = null;
  }
  if (tapSnapshot) {
    const changed =
      tapSnapshot.lines.length !== lines.length ||
      tapSnapshot.lines.some((line, index) => {
        const current = lines[index];
        return !current || line.id !== current.id || line.t !== current.t;
      });
    if (changed) {
      undoStack = pushUndo(undoStack, tapSnapshot);
    }
  }
  tapSnapshot = null;
  tapPhase = null;
  stopAnchor();
  els.tapOverlay.hidden = true;
  setTapBackgroundInert(false);
  els.tapCountdown.textContent = "";
  els.tapLine.textContent = "";
  els.tapProgress.textContent = "";
  lines = sortLines(lines);
  renderStructure();
  scrollLineIntoView(els.lineList, selectedId);
  focusRow(selectedId);
  void setPlayback("pause")
    .then(() => refreshNow())
    .catch(() => undefined);
  if (completed) {
    showToast(t(language, "studio.tap.done"));
  } else {
    showToast(t(language, "studio.tap.stopped"));
  }
}

function updateTapUI(): void {
  const total = lines.length;
  const done = tapSession.index;
  els.tapStart.hidden = tapPhase !== "ready";
  els.tapTap.hidden = tapPhase !== "tapping";
  els.tapCorrection.disabled = tapPhase === "tapping";
  els.tapHint.textContent =
    tapPhase === "tapping" ? t(language, "studio.tap.hint") : t(language, "studio.tap.ready");
  els.tapProgress.textContent = t(language, "studio.tap.progress", {
    done,
    total,
  });
  if (tapPhase === "countdown") {
    els.tapLine.textContent = t(language, "studio.tap.waiting");
    return;
  }
  const nextLine = lines[done];
  if (!nextLine) {
    els.tapLine.textContent = t(language, "studio.tap.finished");
    return;
  }
  els.tapLine.textContent = isMusicMarker(nextLine.text)
    ? t(language, "studio.music")
    : nextLine.text || t(language, "studio.text_empty");
}

function closeTap(): void {
  if (tapPhase === "ready" || tapPhase === null) {
    tapPhase = null;
    els.tapOverlay.hidden = true;
    setTapBackgroundInert(false);
    const button = els.inspector.querySelector<HTMLButtonElement>(".inspector-tap");
    button?.focus();
    return;
  }
  finishTap(tapSession.index >= lines.length);
}

function setTapBackgroundInert(inert: boolean): void {
  for (const node of [
    document.querySelector<HTMLElement>(".studio-header"),
    document.querySelector<HTMLElement>(".studio-transport"),
    document.querySelector<HTMLElement>(".studio-main"),
    els.notice,
  ]) {
    if (node) {
      node.inert = inert;
    }
  }
}

function storedCorrection(): number {
  try {
    const stored = window.localStorage.getItem("nico.tapCorrection");
    return stored === null ? DEFAULT_TAP_CORRECTION : parseCorrection(stored);
  } catch {
    return DEFAULT_TAP_CORRECTION;
  }
}

function storeCorrection(value: number): void {
  try {
    window.localStorage.setItem("nico.tapCorrection", String(value));
  } catch {
    // A persisted preference is optional.
  }
}

async function applyRawText(): Promise<void> {
  if (busy) {
    return;
  }
  const text = els.rawText.value;
  busy = true;
  renderHeader();
  try {
    const parsed = await parseLyrics(text);
    const adopted = makeEditorLines(parsed.lines, nextId);
    undoStack = pushUndo(undoStack, snapshot());
    lines = adopted.lines;
    nextId = adopted.nextId;
    selectedId = lines[0]?.id ?? null;
    migratedNotice = adopted.migrated > 0;
    dirty = true;
    renderStructure();
    syncRawText();
    showToast(t(language, "studio.raw.applied"));
  } catch (error) {
    showToast(errorMessage(language, error));
  } finally {
    busy = false;
    renderHeader();
  }
}

function syncRawText(): void {
  if (document.activeElement === els.rawText || !els.rawPanel.open) {
    return;
  }
  els.rawText.value = serializeLines(lines);
}

async function fetchFromLrclib(): Promise<void> {
  if (busy) {
    return;
  }
  if (dirty && !window.confirm(t(language, "studio.fetch_confirm"))) {
    return;
  }
  busy = true;
  fetching = true;
  renderHeader();
  try {
    const track = now?.track;
    const requested = key
      ? { item_key: key, apply: true }
      : track
        ? {
            title: track.title,
            artist: track.artist,
            album: track.album,
            length: track.length,
            apply: true,
          }
        : null;
    if (!requested) {
      showToast(t(language, "studio.notice.no_track"));
      return;
    }
    const result = await fetchLyrics(requested);
    if (!result.record_key) {
      showToast(t(language, "player.error.lyrics_not_found"));
      return;
    }
    await loadItem(result.record_key);
    showToast(t(language, "studio.fetched"));
  } catch (error) {
    showToast(errorMessage(language, error));
  } finally {
    busy = false;
    fetching = false;
    renderHeader();
  }
}

async function restoreOriginal(): Promise<void> {
  if (!key || busy) {
    return;
  }
  if (dirty && !window.confirm(t(language, "studio.discard_confirm"))) {
    return;
  }
  if (!window.confirm(t(language, "studio.restore_confirm"))) {
    return;
  }
  busy = true;
  renderHeader();
  try {
    const result = await restoreLibraryOriginal(key);
    applyItem(result);
    showToast(t(language, "studio.restored"));
  } catch (error) {
    showToast(errorMessage(language, error));
  } finally {
    busy = false;
    renderHeader();
  }
}

function bindEvents(): void {
  els.save.addEventListener("click", () => {
    void save();
  });
  els.undo.addEventListener("click", undo);
  els.back.addEventListener("click", (event) => {
    if (dirty && !window.confirm(t(language, "studio.discard_confirm"))) {
      event.preventDefault();
    }
  });
  els.play.addEventListener("click", () => {
    void onPlay();
  });
  els.fetch.addEventListener("click", () => {
    void fetchFromLrclib();
  });
  els.restore.addEventListener("click", () => {
    void restoreOriginal();
  });
  els.addLine.addEventListener("click", addLine);
  els.addMusic.addEventListener("click", addMusic);

  els.zoom.addEventListener("input", () => {
    scale = clampScale(Number(els.zoom.value));
    renderTimelineView();
  });
  els.fit.addEventListener("click", () => {
    scale = fitScale(timelineLength(), els.scroll.clientWidth);
    renderTimelineView();
  });
  els.ruler.addEventListener("pointerdown", (event) => {
    if (!isCurrent() || !now?.track?.can_seek) {
      return;
    }
    const rect = els.canvas.getBoundingClientRect();
    void seekToTime(clampTime(timeAtX(event.clientX - rect.left, scale)));
  });

  els.rawPanel.addEventListener("toggle", () => {
    if (els.rawPanel.open) {
      syncRawText();
    }
  });
  els.rawApply.addEventListener("click", () => {
    void applyRawText();
  });
  els.rawReset.addEventListener("click", () => {
    els.rawText.value = serializeLines(lines);
  });

  els.tapStart.addEventListener("click", startTapRun);
  els.tapTap.addEventListener("click", performTap);
  els.tapStop.addEventListener("click", closeTap);
  els.tapCorrection.addEventListener("change", () => {
    const correction = parseCorrection(els.tapCorrection.value);
    els.tapCorrection.value = String(correction);
    storeCorrection(correction);
  });

  document.querySelectorAll<HTMLButtonElement>("[data-language]").forEach((button) => {
    button.addEventListener("click", () => {
      const next = button.dataset.language;
      if (next === "en" || next === "nl") {
        applyLanguage(next, true);
      }
    });
  });

  window.addEventListener("resize", () => {
    renderTimelineView();
  });
  window.addEventListener("beforeunload", (event) => {
    if (dirty) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
  document.addEventListener("keydown", onKeyDown);
  els.tapOverlay.addEventListener("keydown", (event) => {
    if (event.key === "Tab") {
      trapFocus(els.tapOverlay, event);
    }
  });
}

async function onPlay(): Promise<void> {
  if (!isCurrent()) {
    showToast(t(language, "studio.notice.not_current"));
    return;
  }
  try {
    await togglePlayback();
    await refreshNow();
  } catch (error) {
    showToast(errorMessage(language, error));
  }
}

async function seekToTime(time: number): Promise<void> {
  if (!isCurrent() || !now?.track) {
    showToast(t(language, "studio.notice.not_current"));
    return;
  }
  if (!now.track.can_seek) {
    showToast(t(language, "studio.error.no_seek"));
    return;
  }
  try {
    await seekPlayer(Math.max(0, time));
    anchor = {
      pos: Math.max(0, time),
      at: performance.now(),
      playing: anchor?.playing ?? now.track.playing,
      rate: now.track.rate,
      ok: true,
    };
    renderTransport();
  } catch (error) {
    showToast(errorMessage(language, error));
  }
}

function onKeyDown(event: KeyboardEvent): void {
  const typing = isTyping(event.target);
  const command = event.ctrlKey || event.metaKey;

  if (tapPhase !== null) {
    if (event.key === " " || event.key === "Enter") {
      event.preventDefault();
      if (tapPhase === "ready") {
        startTapRun();
      } else if (tapPhase === "tapping") {
        performTap();
      }
      return;
    }
    if (event.key === "Backspace") {
      event.preventDefault();
      tapBackspace();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      closeTap();
      return;
    }
  }

  if (command && event.key.toLowerCase() === "s") {
    event.preventDefault();
    void save();
    return;
  }

  if (typing) {
    return;
  }

  if (command && event.key.toLowerCase() === "z") {
    event.preventDefault();
    undo();
    return;
  }

  if (event.key === " " && !command) {
    event.preventDefault();
    void onPlay();
    return;
  }

  if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && selectedId) {
    event.preventDefault();
    const step = event.altKey ? 0.01 : event.shiftKey ? 0.5 : 0.1;
    const delta = event.key === "ArrowLeft" ? -step : step;
    undoStack = pushUndo(undoStack, snapshot());
    lines = nudgeLine(lines, selectedId, delta);
    updateDirty();
    renderStructure();
    scrollLineIntoView(els.lineList, selectedId);
    focusRow(selectedId);
  }
}

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable ||
    target.closest("button, a") !== null
  );
}

function trapFocus(container: HTMLElement, event: KeyboardEvent): void {
  const focusables = Array.from(
    container.querySelectorAll<HTMLElement>(
      'button, input, textarea, select, a[href], [tabindex]:not([tabindex="-1"])',
    ),
  ).filter(
    (element) =>
      !element.hasAttribute("disabled") &&
      !element.hasAttribute("hidden") &&
      element.getClientRects().length > 0,
  );
  if (focusables.length === 0) {
    return;
  }
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

function frame(): void {
  if (isCurrent()) {
    const position = livePosition();
    updatePlayhead(els.playhead, position, scale);
    els.time.textContent = formatTime(position ?? 0);
  } else {
    updatePlayhead(els.playhead, null, scale);
  }
  window.requestAnimationFrame(frame);
}

async function poll(): Promise<void> {
  if (pollBusy) {
    return;
  }
  pollBusy = true;
  try {
    await refreshNow();
    if (!key && !fatalMessage && now?.lyrics.record_key && lines.length === 0 && !busy) {
      await loadItem(now.lyrics.record_key);
    }
  } catch {
    // Keep the last known state; the next poll may recover.
  } finally {
    pollBusy = false;
  }
}

async function refreshNow(): Promise<void> {
  const state = await getNow();
  now = state;
  updateAnchorFrom(state);
  renderTransport();
  renderHeader();
  if (!key && !fatalMessage && state.lyrics.record_key && lines.length === 0 && !busy) {
    await loadItem(state.lyrics.record_key);
  }
}

function updateAnchorFrom(state: Now): void {
  const track = state.track;
  if (!track) {
    anchor = null;
    anchorTrack = "";
    return;
  }
  const id = `${track.player}:${track.track_id}`;
  const target = targetPosition(track);
  if (target === null) {
    anchor = null;
    anchorTrack = id;
    return;
  }
  anchor = updateAnchor(
    anchor,
    id === anchorTrack,
    target,
    track.playing,
    track.rate,
    performance.now(),
  );
  anchorTrack = id;
}

function stopAnchor(): void {
  if (!anchor) {
    return;
  }
  anchor = {
    ...anchor,
    pos: positionAt(anchor, performance.now()) ?? anchor.pos,
    at: performance.now(),
    playing: false,
  };
}

function isCurrent(): boolean {
  return Boolean(key && now?.lyrics.record_key === key);
}

function livePosition(): number | null {
  const position = positionAt(anchor, performance.now());
  if (position === null) {
    return null;
  }
  const length = now?.track?.length;
  return typeof length === "number" && length > 0 ? Math.min(position, length) : position;
}

/** Playback position for editing actions, valid only while this item is current. */
function editablePosition(): number | null {
  return isCurrent() ? livePosition() : null;
}

function insertionTime(): number | null {
  const position = editablePosition();
  if (position !== null) {
    return roundTime(position);
  }
  const selected = lines.find((line) => line.id === selectedId);
  if (selected?.t !== null && selected?.t !== undefined) {
    return roundTime(selected.t + 1);
  }
  const last = lines.reduce<number | null>((maximum, line) => {
    if (line.t === null) {
      return maximum;
    }
    return maximum === null ? line.t : Math.max(maximum, line.t);
  }, null);
  return last === null ? 0 : roundTime(last + 1);
}

function duration(): number | null {
  if (isCurrent() && typeof now?.track?.length === "number") {
    return now.track.length;
  }
  const value = item?.lyrics.duration;
  return typeof value === "number" && value > 0 ? value : null;
}

function timelineLength(): number {
  return timelineDuration(duration(), lines);
}

function clampTime(time: number): number {
  const length = timelineLength();
  return Math.min(Math.max(0, time), length);
}

function centerTimelineOnLine(id: string | null): void {
  if (!id) {
    return;
  }
  const line = lines.find((candidate) => candidate.id === id);
  if (!line || line.t === null) {
    return;
  }
  const x = xAtTime(line.t, scale);
  els.scroll.scrollLeft = Math.max(0, x - els.scroll.clientWidth / 2);
}

function focusRow(id: string | null): void {
  if (!id) {
    return;
  }
  const row = els.lineList.querySelector<HTMLElement>(`.line-row[data-id="${id}"]`);
  if (!row) {
    return;
  }
  row.tabIndex = -1;
  row.focus({ preventScroll: true });
}

function snapshot(): EditorSnapshot {
  return {
    lines: lines.map((line) => ({ ...line })),
    selectedId,
  };
}

function showNotice(message: string): void {
  els.notice.hidden = false;
  els.notice.textContent = message;
}

function showToast(message: string): void {
  els.toast.textContent = message;
  els.toast.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    els.toast.hidden = true;
  }, 2600);
}

void boot();
