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
};

export type Anchor = {
  pos: number;
  at: number;
  playing: boolean;
  rate: number;
  ok: boolean;
};

const MUSIC_MARKERS = /^[\s♪♫♬♩🎵🎶·•…\-–—]*$/u;
const MUSIC_WORDS = /^(?:instrumental|interlude|muziek|music|solo|gitaarsolo)$/i;

export function isMusicLine(text: string): boolean {
  const value = text.trim();
  return !value || MUSIC_MARKERS.test(value) || MUSIC_WORDS.test(value);
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

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) {
    return "0:00";
  }
  const total = Math.floor(seconds);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/** Position to aim for on the next poll, including time elapsed since the sample. */
export function targetPosition(track: TrackTiming): number | null {
  if (track.position === null || !Number.isFinite(track.position)) {
    return null;
  }
  const rate = track.rate > 0 && Number.isFinite(track.rate) ? track.rate : 1;
  const age = track.playing ? Math.max(0, track.age) : 0;
  return Math.max(0, track.position + age * rate);
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

  const predicted = previous.pos + ((now - previous.at) / 1000) * previous.rate;
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
