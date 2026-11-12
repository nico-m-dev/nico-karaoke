export type LyricLine = {
  t: number | null;
  text: string;
  end?: number | null;
};

export type TrackTiming = {
  position: number | null;
  age: number;
  playing: boolean;
  rate: number;
  length?: number | null;
};

export type Anchor = {
  pos: number;
  at: number;
  playing: boolean;
  rate: number;
  ok: boolean;
};

const MUSIC_MARKERS = /^[\s♪♫♬♩🎵🎶·•…\-–—]*$/u;
const MUSIC_WORDS = /^(?:instrumental|instrumentaal|interlude)$/i;
// "Solo" and "Music" are plausible lyric words, so only treat them as a marker when they are
// clearly labelled, for example "(Solo)".
const MUSIC_LABELS =
  /^[([（](?:solo|music|muziek|gitaarsolo|instrumental|instrumentaal|interlude)[)\])]$/i;

export function isMusicLine(text: string): boolean {
  const value = text.trim();
  return !value || MUSIC_MARKERS.test(value) || MUSIC_WORDS.test(value) || MUSIC_LABELS.test(value);
}

/** Last line whose start time is at or before `time`, or -1 before the first line. */
export function findActiveLine(lines: LyricLine[], time: number): number {
  let low = 0;
  let high = lines.length - 1;
  let best = -1;

  while (low <= high) {
    const mid = (low + high) >> 1;
    const start = lines[mid]?.t;
    if (start !== null && start !== undefined && start <= time) {
      best = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }

  return best;
}

export function lineEnd(
  lines: LyricLine[],
  index: number,
  trackLength: number | null,
): number | null {
  const line = lines[index];
  if (!line || line.t === null || line.t === undefined) {
    return null;
  }
  if (typeof line.end === "number" && Number.isFinite(line.end)) {
    return line.end;
  }
  const next = lines[index + 1]?.t;
  if (typeof next === "number" && Number.isFinite(next)) {
    return next;
  }
  if (typeof trackLength === "number" && Number.isFinite(trackLength)) {
    return trackLength;
  }
  return line.t + 6;
}

/** Fill percentage for the active lyric line, clamped to 0..1. */
export function fillProgress(
  lines: LyricLine[],
  index: number,
  trackLength: number | null,
  time: number,
): number {
  const line = lines[index];
  if (!line || line.t === null || line.t === undefined || isMusicLine(line.text)) {
    return 0;
  }
  const end = lineEnd(lines, index, trackLength);
  if (end === null) {
    return 0;
  }
  const span = Math.max(0.35, end - line.t);
  return clamp((time - line.t) / span, 0, 1);
}

export type FillRect = {
  left: number;
  top: number;
  width: number;
  height: number;
};

/**
 * Clip path for a lyric that wraps over more than one visual line. A single horizontal slice
 * would colour a short last line completely while the first line is still filling, so follow
 * the rendered line boxes in reading order instead.
 */
export function fillClipPath(rects: FillRect[], progress: number): string | null {
  if (rects.length < 2) {
    return null;
  }
  const total = rects.reduce((sum, rect) => sum + Math.max(0, rect.width), 0);
  if (total <= 0) {
    return null;
  }
  const clamped = clamp(progress, 0, 1);
  if (clamped <= 0 || clamped >= 1) {
    return null;
  }

  let remaining = clamped * total;
  const parts: string[] = [];
  for (const rect of rects) {
    if (remaining <= 0) {
      break;
    }
    const width = Math.max(0, rect.width);
    if (width <= 0) {
      continue;
    }
    const take = Math.min(width, remaining);
    remaining -= take;
    const left = roundCoordinate(rect.left);
    const top = roundCoordinate(rect.top);
    const right = roundCoordinate(rect.left + take);
    const bottom = roundCoordinate(rect.top + rect.height);
    parts.push(`M${left} ${top}H${right}V${bottom}H${left}Z`);
  }

  return parts.length > 0 ? `path("${parts.join("")}")` : null;
}

function roundCoordinate(value: number): number {
  return Math.round(value * 100) / 100;
}

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) {
    return "0:00";
  }
  const total = Math.floor(seconds);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/** Player clock text. A missing position is explicit instead of pretending to be at zero. */
export function formatClock(position: number | null, length: number | null): string {
  const elapsed = position === null ? "—" : formatTime(position);
  if (typeof length === "number" && Number.isFinite(length) && length > 0) {
    return `${elapsed} / ${formatTime(length)}`;
  }
  return elapsed;
}

/** Position to aim for on the next poll, including time elapsed since the sample. */
export function targetPosition(track: TrackTiming): number | null {
  if (track.position === null || !Number.isFinite(track.position)) {
    return null;
  }
  const rate = track.rate > 0 && Number.isFinite(track.rate) ? track.rate : 1;
  const age = track.playing ? Math.max(0, track.age) : 0;
  const value = Math.max(0, track.position + age * rate);
  return typeof track.length === "number" && Number.isFinite(track.length) && track.length > 0
    ? Math.min(value, track.length)
    : value;
}

/**
 * Smooth small provider corrections while following real seeks immediately. A small negative
 * drift must not move the lyric line backwards.
 */
export function updateAnchor(
  previous: Anchor | null,
  sameTrack: boolean,
  target: number,
  playing: boolean,
  rate: number,
  now: number,
): Anchor {
  const safeRate = rate > 0 && Number.isFinite(rate) ? rate : 1;
  if (!previous || !previous.ok || !sameTrack) {
    return { pos: target, at: now, playing, rate: safeRate, ok: true };
  }

  if (previous.playing !== playing) {
    return { pos: target, at: now, playing, rate: safeRate, ok: true };
  }

  const predicted =
    previous.pos + (previous.playing ? ((now - previous.at) / 1000) * previous.rate : 0);
  const drift = target - predicted;
  let corrected: number;

  if (Math.abs(drift) > 1.5) {
    corrected = target;
  } else if (playing && drift < 0) {
    corrected = predicted;
  } else {
    corrected = predicted + drift * 0.25;
  }

  return {
    pos: Math.max(0, corrected),
    at: now,
    playing,
    rate: safeRate,
    ok: true,
  };
}

export function positionAt(anchor: Anchor | null, now: number): number | null {
  if (!anchor || !anchor.ok) {
    return null;
  }
  const elapsed = anchor.playing ? ((now - anchor.at) / 1000) * anchor.rate : 0;
  return Math.max(0, anchor.pos + elapsed);
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
