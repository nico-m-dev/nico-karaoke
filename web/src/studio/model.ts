import type { LyricLine } from "../player";

export type EditorLine = {
  id: string;
  t: number | null;
  text: string;
};

export type EditorSnapshot = {
  lines: EditorLine[];
  selectedId: string | null;
};

export const MUSIC_TEXT = "♪";
export const MAX_UNDO = 30;
export const LEGACY_GAP = 0.5;
export const MUSIC_DUPLICATE_WINDOW = 0.1;

export function makeEditorLines(
  source: LyricLine[],
  startIndex = 1,
): { lines: EditorLine[]; migrated: number; normalized: number; nextId: number } {
  const normalized = source.filter((line) => line.t !== null && line.text.trim() === "").length;
  const migrated = migrateLegacyEnds(source);
  const lines = migrated.lines.map((line, index) => ({
    id: `line-${startIndex + index}`,
    t: line.t,
    text: line.text,
  }));
  return {
    lines: sortLines(lines),
    migrated: migrated.migrated,
    normalized,
    nextId: startIndex + lines.length,
  };
}

/**
 * Replace legacy `[end:]` tags with `♪` markers where the tag leaves a real instrumental gap.
 * The end time itself is not kept: new files express breaks as separate lines.
 */
export function migrateLegacyEnds(source: LyricLine[]): {
  lines: LyricLine[];
  migrated: number;
} {
  let migrated = 0;
  const plain: LyricLine[] = source.map((line) => {
    if (typeof line.end === "number") {
      migrated += 1;
    }
    // The player treats a timed line without text as an instrumental break. Keep the
    // Studio in sync instead of showing an empty "Lyric text" row.
    const text = line.t !== null && line.text.trim() === "" ? MUSIC_TEXT : line.text;
    return { t: line.t, text };
  });

  const timed = source
    .flatMap((line) => {
      if (typeof line.t === "number" && typeof line.end === "number" && line.end > line.t) {
        return [{ t: line.t, end: line.end }];
      }
      return [];
    })
    .sort((left, right) => left.t - right.t);
  const starts = source
    .flatMap((line) => (typeof line.t === "number" ? [line.t] : []))
    .sort((left, right) => left - right);

  const markers: number[] = [];
  for (const line of timed) {
    const next = starts.find((start) => start > line.t);
    const end = line.end;
    if (next === undefined || next - end < LEGACY_GAP) {
      continue;
    }
    if (markers.some((marker) => Math.abs(marker - end) < MUSIC_DUPLICATE_WINDOW)) {
      continue;
    }
    markers.push(end);
  }

  const lines = sortLines([...plain, ...markers.map((t) => ({ t, text: MUSIC_TEXT }))]);
  return { lines, migrated };
}

export function sortLines<T extends { t: number | null; text: string }>(lines: T[]): T[] {
  return [...lines].sort((left, right) => {
    if (left.t === null && right.t === null) {
      return 0;
    }
    if (left.t === null) {
      return 1;
    }
    if (right.t === null) {
      return -1;
    }
    return left.t - right.t;
  });
}

export function serializeLines(lines: EditorLine[]): string {
  const output: string[] = [];
  for (const line of lines) {
    if (line.t === null) {
      if (line.text.trim()) {
        output.push(line.text);
      }
      continue;
    }
    output.push(`[${formatTimecode(line.t)}]${line.text ? ` ${line.text}` : ""}`);
  }
  return output.join("\n");
}

/** `mm:ss.xx`, rounded to centiseconds, with minutes allowed to pass 59. */
export function formatTimecode(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) {
    return "";
  }
  const centiseconds = Math.max(0, Math.round(seconds * 100));
  const minutes = Math.floor(centiseconds / 6000);
  const rest = centiseconds % 6000;
  const whole = Math.floor(rest / 100);
  const fraction = rest % 100;
  return `${String(minutes).padStart(2, "0")}:${String(whole).padStart(2, "0")}.${String(
    fraction,
  ).padStart(2, "0")}`;
}

/** Accept `mm:ss.xx`, `[mm:ss.xx]`, `h:mm:ss.xx`, or bare seconds. */
export function parseTimecode(value: string): number | null {
  const trimmed = value.trim().replace(/^\[/, "").replace(/\]$/, "").trim();
  if (!trimmed) {
    return null;
  }
  const parts = trimmed.split(":");
  if (parts.length > 3) {
    return null;
  }
  let seconds = 0;
  for (const part of parts) {
    const number = parsePart(part);
    if (number === null) {
      return null;
    }
    seconds = seconds * 60 + number;
  }
  return Number.isFinite(seconds) ? Math.round(seconds * 1000) / 1000 : null;
}

function parsePart(value: string): number | null {
  const normalised = value.trim().replace(",", ".");
  if (!/^\d+(?:\.\d+)?$/.test(normalised)) {
    return null;
  }
  const number = Number.parseFloat(normalised);
  return Number.isFinite(number) ? number : null;
}

export function insertLine(
  lines: EditorLine[],
  id: string,
  time: number | null,
  text = "",
): { lines: EditorLine[]; id: string } {
  return {
    lines: sortLines([...lines, { id, t: time, text }]),
    id,
  };
}

export function insertMusicMarker(
  lines: EditorLine[],
  id: string,
  time: number,
): { lines: EditorLine[]; id: string; created: boolean } {
  const existing = lines.find(
    (line) =>
      isMusicMarker(line.text) &&
      line.t !== null &&
      Math.abs(line.t - time) < MUSIC_DUPLICATE_WINDOW,
  );
  if (existing) {
    return { lines, id: existing.id, created: false };
  }
  return { ...insertLine(lines, id, time, MUSIC_TEXT), created: true };
}

export function deleteLine(lines: EditorLine[], id: string): EditorLine[] {
  return lines.filter((line) => line.id !== id);
}

export function updateLine(
  lines: EditorLine[],
  id: string,
  update: Partial<Pick<EditorLine, "t" | "text">>,
  sorted = false,
): EditorLine[] {
  const next = lines.map((line) => (line.id === id ? { ...line, ...update } : line));
  return sorted ? sortLines(next) : next;
}

export function nudgeLine(lines: EditorLine[], id: string, delta: number): EditorLine[] {
  const line = lines.find((candidate) => candidate.id === id);
  if (!line || line.t === null) {
    return lines;
  }
  return updateLine(lines, id, { t: roundTime(Math.max(0, line.t + delta)) }, true);
}

export function takeTime(lines: EditorLine[], id: string, time: number): EditorLine[] {
  return updateLine(lines, id, { t: roundTime(Math.max(0, time)) }, true);
}

export function toggleMusic(lines: EditorLine[], id: string): EditorLine[] {
  const line = lines.find((candidate) => candidate.id === id);
  if (!line) {
    return lines;
  }
  return updateLine(lines, id, {
    text: isMusicMarker(line.text) ? "" : MUSIC_TEXT,
  });
}

export function isMusicMarker(text: string): boolean {
  return text.trim() === MUSIC_TEXT;
}

export function roundTime(seconds: number): number {
  return Math.round(seconds * 1000) / 1000;
}

export function selectedIndex(lines: EditorLine[], selectedId: string | null): number {
  return lines.findIndex((line) => line.id === selectedId);
}

export function pushUndo(stack: EditorSnapshot[], snapshot: EditorSnapshot): EditorSnapshot[] {
  const next = [...stack, snapshot];
  return next.length > MAX_UNDO ? next.slice(next.length - MAX_UNDO) : next;
}

export function popUndo(stack: EditorSnapshot[]): {
  snapshot: EditorSnapshot | null;
  stack: EditorSnapshot[];
} {
  if (stack.length === 0) {
    return { snapshot: null, stack };
  }
  const snapshot = stack[stack.length - 1] ?? null;
  return { snapshot, stack: stack.slice(0, -1) };
}

/** Duration to draw on the timeline: track length, or the last timed line plus a margin. */
export function timelineDuration(trackLength: number | null, lines: EditorLine[]): number {
  const last = lines.reduce((maximum, line) => {
    return line.t !== null ? Math.max(maximum, line.t) : maximum;
  }, 0);
  const fromLines = last + 15;
  if (trackLength !== null && Number.isFinite(trackLength) && trackLength > 0) {
    return Math.max(trackLength, 1);
  }
  return Math.max(fromLines, 30);
}
