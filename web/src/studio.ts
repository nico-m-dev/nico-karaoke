import "./styles.css";
import "./studio.css";

import {
  fetchLyrics,
  getLibraryItem,
  getNow,
  parseLyrics,
  restoreLibraryOriginal,
  saveCurrentLyrics,
  saveLibraryItem,
  seekTo as seekPlayer,
  setPlayback,
  type LibraryItemResponse,
  type LyricsSource,
  type Now,
} from "./api";
import { applyTranslations, detectLanguage, t, type Language } from "./i18n";
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
import { isStaleSeekSample, startSeekGuard, type SeekGuard } from "./studio/seek";
import {
  activeTransportIntent,
  reconcileTransportIntent,
  startTransportIntent,
  type TransportIntent,
} from "./studio/transport";
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
  updateMarkerText,
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
  tapMode: query<HTMLButtonElement>("#studio-tap-mode"),
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
  tapContext: query<HTMLElement>("#tap-context"),
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
let boundTrackId: string | null = null;
let boundSignature: string | null = null;
let boundTitle: string | null = null;
let boundArtist: string | null = null;
let item: LibraryItemResponse | null = null;
let now: Now | null = null;
let lines: EditorLine[] = [];
let metadata: string[] = [];
let extras: string[] = [];
let source: LyricsSource = "manual";
let selectedId: string | null = null;
let nextId = 1;
let dirty = false;
let leaving = false;
let busy = false;
let transportBusy = false;
let transportQueued = false;
let nowSerial = 0;
let fetching = false;
let migratedNotice = false;
let undoStack: EditorSnapshot[] = [];
let scale = DEFAULT_SCALE;
let editSnapshot: EditorSnapshot | null = null;
let anchor: Anchor | null = null;
let anchorTrack = "";
let pendingSeek: SeekGuard | null = null;
let pollBusy = false;
let pollOffline = false;
let nextRecovery = 0;
let frameScheduled = false;
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
  applyLanguage(language);
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
    if (!document.hidden) {
      void poll();
    }
  }, 500);
  void poll();
}

function applyLanguage(next: Language): void {
  language = next;
  document.documentElement.lang = next;
  document.title = t(next, "studio.page_title");
  applyTranslations(document, next);
  if (booted) {
    renderStructure();
    if (tapPhase !== null) {
      updateTapUI();
    }
  }
}

async function loadCurrent(): Promise<void> {
  try {
    const state = await getNow();
    now = state;
    updateAnchorFrom(state);
    if (state.lyrics.state === "found" && state.lyrics.record_key) {
      await loadItem(state.lyrics.record_key);
      return;
    }
    fatalMessage = null;
    key = null;
    item = null;
    resetTransportState();
    if (state.track) {
      bindCurrentTrack(state);
    } else {
      boundTrackId = null;
      boundSignature = null;
      boundTitle = null;
      boundArtist = null;
    }
    lines = [];
    metadata = [];
    extras = [];
    source = "manual";
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
  boundTrackId = null;
  boundSignature = null;
  boundTitle = null;
  boundArtist = null;
  item = result;
  resetTransportState();
  metadata = result.lyrics.metadata ?? [];
  extras = result.lyrics.extras ?? [];
  source = result.lyrics.source ?? "manual";
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

/** Bind a new, unsaved document to whatever is playing now. */
function bindCurrentTrack(state: Now): void {
  const track = state.track;
  if (!track) {
    return;
  }
  boundTrackId = track.track_id;
  boundSignature = state.lyrics.signature;
  boundTitle = track.title;
  boundArtist = track.artist;
}

function showFatal(message: string): void {
  fatalMessage = message;
  key = null;
  boundTrackId = null;
  boundSignature = null;
  boundTitle = null;
  boundArtist = null;
  item = null;
  resetTransportState();
  lines = [];
  metadata = [];
  extras = [];
  source = "manual";
  selectedId = null;
  dirty = false;
  migratedNotice = false;
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
  const title = lyrics?.title || boundTitle || track?.title || t(language, "studio.title.unknown");
  els.title.textContent = title;

  const parts = [
    lyrics?.artist || boundArtist || track?.artist || "",
    lyrics?.album || "",
    lyrics ? t(language, sourceLabelKey(lyrics.source ?? "manual")) : "",
    lyrics ? (lyrics.synced ? t(language, "library.synced") : t(language, "library.plain")) : "",
    lyrics?.original ? t(language, "player.library.original_available") : "",
  ].filter(Boolean);
  els.subtitle.textContent = parts.join(" · ");

  const text =
    !item && lines.length === 0
      ? ""
      : busy
        ? t(language, fetching ? "studio.fetching" : "studio.saving")
        : dirty
          ? t(language, "studio.unsaved")
          : t(language, "studio.saved");
  els.saveState.textContent = text;
  els.saveState.classList.toggle("dirty", dirty && !busy);

  els.save.disabled = (key ? false : !isCurrent()) || !dirty || busy;
  els.undo.disabled = undoStack.length === 0 || busy;
  els.tapMode.title = !isCurrent()
    ? t(language, "studio.notice.not_current")
    : !now?.track?.can_seek
      ? t(language, "studio.tap.needs_seek")
      : lines.length === 0
        ? t(language, "studio.tap.no_lines")
        : "";
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
  } else if (!key && boundTrackId && !isCurrent()) {
    message = t(language, "studio.notice.track_changed");
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

const TRANSPORT_GRACE_MS = 2000;
let transportReadyUntil = 0;
let transportReadyDocument = "";
let transportReadyTrack = "";
let transportIntent: TransportIntent | null = null;

/** The opened document that the transport belongs to. */
function documentIdentity(): string {
  return key ?? `unsaved:${boundTrackId ?? ""}|${boundSignature ?? ""}`;
}

function followedTrackIdentity(): string {
  const track = now?.track;
  if (!track) {
    return "";
  }
  // Some players reuse one MPRIS track id for every track, so include the signature too.
  return `${track.player}:${track.track_id}|${now?.lyrics.signature ?? ""}`;
}

/** Drop transport state that belongs to the document that was open before. */
function resetTransportState(): void {
  pendingSeek = null;
  transportQueued = false;
  transportIntent = null;
  transportReadyUntil = 0;
  transportReadyDocument = "";
  transportReadyTrack = "";
}

/** The new play state shown immediately after a click, before a poll confirms it. */
function currentTransportIntent(): TransportIntent | null {
  transportIntent = activeTransportIntent(transportIntent, performance.now());
  return transportIntent;
}

/** Play state for the playhead: the fresh intent wins over a poll from before the click. */
function transportPlaying(): boolean {
  return currentTransportIntent()?.playing ?? Boolean(now?.track?.playing);
}

function applyTransportIntent(playing: boolean): void {
  const nowMs = performance.now();
  transportIntent = startTransportIntent(playing, nowMs);
  if (anchor) {
    anchor = {
      ...anchor,
      pos: positionAt(anchor, nowMs) ?? anchor.pos,
      at: nowMs,
      playing,
    };
  }
  if (playing) {
    ensureFrame();
  }
}

/**
 * Whether the transport may control the followed player. One failed provider sample (a slow
 * D-Bus reply, for example) clears `can_play` for a single poll. Disabling the button inside
 * that window swallowed the click, so bridge a short gap instead. The grace never crosses a
 * different Studio document or a different track. A completely missing track disables the
 * control, because the server cannot reach a player it cannot see.
 */
function transportReady(): boolean {
  const nowMs = performance.now();
  const track = now?.track;
  if (isCurrent() && track?.can_play) {
    transportReadyUntil = nowMs + TRANSPORT_GRACE_MS;
    transportReadyDocument = documentIdentity();
    transportReadyTrack = followedTrackIdentity();
    return true;
  }
  if (nowMs >= transportReadyUntil || transportReadyDocument !== documentIdentity()) {
    return false;
  }
  return Boolean(track) && followedTrackIdentity() === transportReadyTrack;
}

function renderTransport(): void {
  const current = isCurrent();
  const track = now?.track;
  const intent = currentTransportIntent();
  const playing = intent ? intent.playing : current && Boolean(track?.playing);
  // Never disable while a request is in flight: a second click is queued by onPlay instead of
  // being swallowed by the browser. Also bridge a single failed `can_play` or follow sample.
  els.play.disabled = !transportReady();
  els.play.setAttribute("aria-busy", String(transportBusy));
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
  els.lineCount.textContent =
    count === 1
      ? t(language, "studio.line_count_one")
      : t(language, "studio.line_count", { count });
  els.linesEmpty.hidden = count > 0;
  renderLines(els.lineList, lines, selectedId, language, lineHandlers);
}

/** Keep the selected marker (or the viewport centre) in place while zooming. */
function applyScale(next: number): void {
  const previous = scale;
  const selected = lines.find((line) => line.id === selectedId);
  const selectedTime = selected && selected.t !== null ? selected.t : null;
  const centre = els.scroll.clientWidth / 2;
  const anchorTime = selectedTime ?? timeAtX(els.scroll.scrollLeft + centre, previous);
  const anchorOffset =
    selectedTime !== null ? xAtTime(selectedTime, previous) - els.scroll.scrollLeft : centre;
  scale = clampScale(next);
  renderTimelineView();
  els.scroll.scrollLeft = Math.max(0, xAtTime(anchorTime, scale) - anchorOffset);
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
    },
    inspectorHandlers,
  );
}

function updateDirty(): void {
  dirty = true;
  if (source !== "tap") {
    source = "manual";
  }
  renderHeader();
}

async function save(): Promise<void> {
  if (busy) {
    return;
  }
  if (!key && !isCurrent()) {
    showToast(t(language, now?.track ? "studio.notice.track_changed" : "studio.notice.no_track"));
    return;
  }
  const text = serializeLines(lines, metadata, extras);
  if (!text.trim()) {
    showToast(t(language, "player.error.lyrics_empty"));
    return;
  }
  busy = true;
  renderHeader();
  try {
    const updated = key
      ? await saveLibraryItem(key, text, source)
      : await saveCurrentLyrics(
          text,
          boundTrackId ?? undefined,
          boundSignature ?? undefined,
          source,
        );
    key = updated.lyrics.record_key ?? key;
    boundTrackId = null;
    boundSignature = null;
    boundTitle = null;
    boundArtist = null;
    metadata = updated.lyrics.metadata ?? metadata;
    extras = updated.lyrics.extras ?? extras;
    source = updated.lyrics.source ?? source;
    item = { ...updated, current: isCurrent() };
    fatalMessage = null;
    dirty = false;
    migratedNotice = false;
    undoStack = [];
    editSnapshot = null;
    await refreshNow().catch(() => undefined);
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
  const wasDirty = dirty;
  const wasSource = source;
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
    updateDirty();
  };

  let finished = false;
  const cleanup = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onCancel);
    document.body.classList.remove("dragging");
  };
  const finish = (commit: boolean) => {
    if (finished) {
      return;
    }
    finished = true;
    cleanup();
    if (!moved) {
      if (commit) {
        selectLine(id, true);
      }
      return;
    }
    if (!commit) {
      lines = before.lines.map((line) => ({ ...line }));
      selectedId = before.selectedId;
      dirty = wasDirty;
      source = wasSource;
      renderStructure();
      return;
    }
    undoStack = pushUndo(undoStack, before);
    lines = sortLines(lines);
    renderStructure();
    scrollLineIntoView(els.lineList, selectedId);
  };
  const onUp = () => finish(true);
  const onCancel = () => finish(false);

  // Capture the pointer so releasing outside the window still finishes the drag.
  if (event.currentTarget instanceof Element) {
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture is an enhancement; the window listeners still handle the common case.
    }
  }
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onCancel);
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
  if (!editSnapshot) {
    editSnapshot = snapshot();
  }
  lines = updateLine(lines, id, { text: value });
  source = "manual";
  dirty = true;
  updateMarkerText(els.markers, id, value, language);
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

function endEdit(id: string): void {
  if (!editSnapshot) {
    return;
  }
  const changed =
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
  pendingSeek = startSeekGuard(start, true, rate, performance.now());
  try {
    await setPlayback("pause");
    await seekPlayer(start);
    await setPlayback("play");
    // Start the local clock only after the player accepted play, otherwise the preview runs
    // ahead by the round-trip latency.
    anchor = {
      pos: start,
      at: performance.now(),
      playing: true,
      rate,
      ok: true,
    };
    ensureFrame();
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
    pendingSeek = null;
    stopAnchor();
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
    void beginTapping();
  }, 1000);
}

async function beginTapping(): Promise<void> {
  pendingSeek = startSeekGuard(0, true, now?.track?.rate ?? 1, performance.now());
  try {
    await seekPlayer(0);
    if (tapPhase !== "countdown") {
      pendingSeek = null;
      return;
    }
    await setPlayback("play");
    if (tapPhase !== "countdown") {
      pendingSeek = null;
      void setPlayback("pause").catch(() => undefined);
      return;
    }
    // Seed the local clock after play was accepted so taps are not systematically early.
    anchor = {
      pos: 0,
      at: performance.now(),
      playing: true,
      rate: now?.track?.rate ?? 1,
      ok: true,
    };
    ensureFrame();
    tapPhase = "tapping";
    updateTapUI();
    els.tapTap.focus();
  } catch (error) {
    pendingSeek = null;
    if (tapPhase === null) {
      return;
    }
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
  source = "tap";
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
  source = "tap";
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
  els.tapContext.replaceChildren();
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
  const start = Math.max(0, done - 2);
  const end = Math.min(total, done + 3);
  const context: HTMLElement[] = [];
  for (let index = start; index < end; index += 1) {
    const line = lines[index];
    if (!line) {
      continue;
    }
    const node = document.createElement("p");
    node.className = index === done ? "current" : index < done ? "past" : "next";
    if (index === done) {
      node.setAttribute("aria-current", "step");
    }
    node.textContent = isMusicMarker(line.text)
      ? t(language, "studio.music")
      : line.text || t(language, "studio.text_empty");
    context.push(node);
  }
  if (context.length === 0) {
    const node = document.createElement("p");
    node.className = "next";
    node.textContent = t(language, "studio.tap.finished");
    context.push(node);
  }
  els.tapContext.replaceChildren(...context);
}

function closeTap(): void {
  if (tapPhase === "ready" || tapPhase === null) {
    tapPhase = null;
    els.tapOverlay.hidden = true;
    setTapBackgroundInert(false);
    els.tapMode.focus();
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
    metadata = parsed.metadata ?? [];
    extras = parsed.extras ?? [];
    nextId = adopted.nextId;
    selectedId = lines[0]?.id ?? null;
    migratedNotice = adopted.migrated > 0;
    source = "manual";
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
  els.rawText.value = serializeLines(lines, metadata, extras);
}

async function fetchFromLrclib(): Promise<void> {
  if (busy) {
    return;
  }
  if ((dirty || item || lines.length > 0) && !window.confirm(t(language, "studio.fetch_confirm"))) {
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
      return;
    }
    leaving = true;
  });
  els.play.addEventListener("click", (event) => {
    void onPlay(event.detail);
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
    applyScale(Number(els.zoom.value));
  });
  els.fit.addEventListener("click", () => {
    scale = fitScale(timelineLength(), els.scroll.clientWidth);
    renderTimelineView();
    els.scroll.scrollLeft = 0;
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
    els.rawText.value = serializeLines(lines, metadata, extras);
  });

  els.tapMode.addEventListener("click", openTap);
  els.tapStart.addEventListener("click", startTapRun);
  els.tapTap.addEventListener("click", performTap);
  els.tapStop.addEventListener("click", closeTap);
  els.tapCorrection.addEventListener("change", () => {
    const correction = parseCorrection(els.tapCorrection.value);
    els.tapCorrection.value = String(correction);
    storeCorrection(correction);
  });

  window.addEventListener("resize", () => {
    renderTimelineView();
  });
  window.addEventListener("beforeunload", (event) => {
    if (dirty && !leaving) {
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

async function onPlay(detail = 1): Promise<void> {
  // A double-click is one intent, even when the second click arrives after the first request
  // already finished.
  if (detail > 1) {
    return;
  }
  if (transportBusy) {
    // Never drop a single click that lands while the first request is in flight; run it after.
    transportQueued = true;
    return;
  }
  if (!transportReady()) {
    showToast(t(language, now?.track ? "studio.notice.not_current" : "studio.notice.no_track"));
    return;
  }
  const track = now?.track;
  if (!track) {
    return;
  }
  // Explicit play/pause based on the intended state. A blind playpause loses the click when an
  // impatient second click arrives before the first state reached the UI.
  const wanted = !(currentTransportIntent()?.playing ?? track.playing);
  const previousAnchor = anchor;
  transportBusy = true;
  applyTransportIntent(wanted);
  renderTransport();
  try {
    await setPlayback(wanted ? "play" : "pause");
    await refreshNow();
  } catch (error) {
    transportIntent = null;
    anchor = previousAnchor;
    showToast(errorMessage(language, error));
  } finally {
    transportBusy = false;
    const queued = transportQueued;
    transportQueued = false;
    renderTransport();
    if (queued) {
      void onPlay();
    }
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
  const target = Math.max(0, time);
  const previous = anchor;
  const playing = anchor?.playing ?? now.track.playing;
  const rate = now.track.rate;
  // Hold the playhead at the requested position while the player still reports the old one,
  // and show it immediately so a slow player does not make the click look lost.
  pendingSeek = startSeekGuard(target, playing, rate, performance.now());
  anchor = { pos: target, at: performance.now(), playing, rate, ok: true };
  renderTransport();
  updatePlayhead(els.playhead, livePosition(), scale);
  if (playing) {
    ensureFrame();
  }
  try {
    await seekPlayer(target);
    // Start the local clock when the player accepted the seek, not before the round trip.
    anchor = { pos: target, at: performance.now(), playing, rate, ok: true };
  } catch (error) {
    pendingSeek = null;
    anchor = previous;
    renderTransport();
    updatePlayhead(els.playhead, livePosition(), scale);
    showToast(errorMessage(language, error));
  }
}

function onKeyDown(event: KeyboardEvent): void {
  const typing = isTyping(event.target);
  const command = event.ctrlKey || event.metaKey;
  const target = event.target;
  const editingField =
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable);

  if (tapPhase !== null && event.key === "Escape") {
    event.preventDefault();
    closeTap();
    return;
  }

  // The tap shortcuts own Space, Enter, and Backspace, but not while the correction field has
  // focus. Escape is handled above so it always leaves the overlay.
  if (tapPhase !== null && !editingField) {
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
    // Holding the key is one edit, not one undo entry per repeat.
    if (!event.repeat) {
      undoStack = pushUndo(undoStack, snapshot());
    }
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

function ensureFrame(): void {
  if (frameScheduled || document.hidden) {
    return;
  }
  frameScheduled = true;
  window.requestAnimationFrame(frame);
}

function frame(): void {
  frameScheduled = false;
  if (isCurrent()) {
    const position = livePosition();
    updatePlayhead(els.playhead, position, scale);
    els.time.textContent = formatTime(position ?? 0);
  } else {
    updatePlayhead(els.playhead, null, scale);
  }
  if (isCurrent() && transportPlaying() && !document.hidden) {
    ensureFrame();
  }
}

async function poll(): Promise<void> {
  if (pollBusy) {
    return;
  }
  pollBusy = true;
  try {
    try {
      await refreshNow();
      pollOffline = false;
    } catch {
      if (!pollOffline) {
        pollOffline = true;
        showToast(t(language, "player.offline"));
      }
      // Keep the last known state; the next poll may recover.
      return;
    }

    // A single failed boot or a deleted item must not leave the editor dead until a reload.
    if (fatalMessage && !busy && Date.now() >= nextRecovery) {
      nextRecovery = Date.now() + 5000;
      fatalMessage = null;
      await loadCurrent();
      return;
    }

    if (
      !key &&
      !fatalMessage &&
      now?.lyrics.state === "found" &&
      now.lyrics.record_key &&
      lines.length === 0 &&
      !busy
    ) {
      try {
        await loadItem(now.lyrics.record_key);
      } catch {
        // A failed item load must not be reported as a server outage.
      }
    }
  } finally {
    pollBusy = false;
  }
}

async function refreshNow(): Promise<void> {
  const serial = ++nowSerial;
  const state = await getNow();
  // A poll that started before a play/seek/save must not overwrite the fresher state with
  // its older response.
  if (serial !== nowSerial) {
    return;
  }
  now = state;
  if (state.track) {
    transportIntent = reconcileTransportIntent(
      transportIntent,
      state.track.playing,
      performance.now(),
    );
  } else {
    transportIntent = null;
  }
  // A document opened before any track was playing must bind once one starts, otherwise Save
  // keeps reporting a track change that never happened.
  if (key === null && !fatalMessage && state.track) {
    if (boundTrackId === null && boundSignature === null) {
      bindCurrentTrack(state);
    }
  }
  updateAnchorFrom(state);
  renderTransport();
  renderHeader();
  updatePlayhead(els.playhead, isCurrent() ? livePosition() : null, scale);
  if (isCurrent() && transportPlaying()) {
    ensureFrame();
  }
}

function updateAnchorFrom(state: Now): void {
  const track = state.track;
  if (!track) {
    // Keep the last clock through a missed provider sample. A real track change is detected
    // below, and a hidden playhead already makes the gap visible.
    return;
  }
  const id = `${track.player}:${track.track_id}`;
  const target = targetPosition(track);
  if (target === null) {
    anchor = null;
    anchorTrack = id;
    pendingSeek = null;
    return;
  }
  const nowMs = performance.now();
  const playing = currentTransportIntent()?.playing ?? track.playing;
  if (pendingSeek && id === anchorTrack) {
    // A sample from before the seek must not pull the playhead back. Once the player reports
    // the requested position, settle on it instead of easing across the small offset a player
    // can keep after a seek (frame rounding, for example).
    if (isStaleSeekSample(pendingSeek, target, nowMs)) {
      return;
    }
    pendingSeek = null;
    anchor = {
      pos: target,
      at: nowMs,
      playing,
      rate: track.rate > 0 && Number.isFinite(track.rate) ? track.rate : 1,
      ok: true,
    };
    anchorTrack = id;
    return;
  }
  pendingSeek = null;
  anchor = updateAnchor(anchor, id === anchorTrack, target, playing, track.rate, nowMs);
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
  if (fatalMessage) {
    return false;
  }
  if (key) {
    return now?.lyrics.record_key === key;
  }
  if ((!boundTrackId && !boundSignature) || !now?.track) {
    return false;
  }
  return (
    (!boundTrackId || now.track.track_id === boundTrackId) &&
    (boundSignature === null || now.lyrics.signature === boundSignature)
  );
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
  els.toast.classList.add("show");
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    els.toast.classList.remove("show");
    els.toast.textContent = "";
  }, 3600);
}

void boot();
