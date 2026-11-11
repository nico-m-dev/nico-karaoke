import { describe, expect, it } from "vitest";
import {
  findActiveLine,
  fillProgress,
  formatClock,
  formatTime,
  isMusicLine,
  lineEnd,
  positionAt,
  targetPosition,
  updateAnchor,
  type Anchor,
  type LyricLine,
} from "./player";

const lines: LyricLine[] = [
  { t: 1, text: "First" },
  { t: 3, text: "Second" },
  { t: 5, text: "♪" },
  { t: 7, text: "Third" },
];

describe("formatClock", () => {
  it("shows an explicit dash when the player has no position", () => {
    expect(formatClock(null, 180)).toBe("— / 3:00");
    expect(formatClock(null, null)).toBe("—");
  });

  it("shows elapsed and total when both are known", () => {
    expect(formatClock(83.9, 180)).toBe("1:23 / 3:00");
    expect(formatClock(10, null)).toBe("0:10");
  });
});

describe("findActiveLine", () => {
  it("returns -1 before the first line", () => {
    expect(findActiveLine(lines, 0.5)).toBe(-1);
  });

  it("finds exact boundaries", () => {
    expect(findActiveLine(lines, 1)).toBe(0);
    expect(findActiveLine(lines, 3)).toBe(1);
  });

  it("finds the line between boundaries", () => {
    expect(findActiveLine(lines, 4.5)).toBe(1);
  });

  it("keeps the last line active after the end", () => {
    expect(findActiveLine(lines, 100)).toBe(3);
  });

  it("handles empty and non-finite times", () => {
    expect(findActiveLine([], 1)).toBe(-1);
    expect(findActiveLine(lines, Number.NaN)).toBe(-1);
  });
});

describe("line timing", () => {
  it("uses the next line as the end", () => {
    expect(lineEnd(lines, 0, 30)).toBe(3);
  });

  it("uses a legacy end tag first", () => {
    const timed: LyricLine[] = [
      { t: 1, text: "One", end: 2.5 },
      { t: 4, text: "Two" },
    ];
    expect(lineEnd(timed, 0, 30)).toBe(2.5);
  });

  it("falls back to track length and then a default gap", () => {
    expect(lineEnd([{ t: 1, text: "One" }], 0, 30)).toBe(30);
    expect(lineEnd([{ t: 1, text: "One" }], 0, null)).toBe(7);
  });

  it("clamps fill progress", () => {
    expect(fillProgress(lines, 0, 30, 0)).toBe(0);
    expect(fillProgress(lines, 0, 30, 2)).toBeCloseTo(0.5);
    expect(fillProgress(lines, 0, 30, 10)).toBe(1);
  });

  it("does not fill music markers", () => {
    expect(fillProgress(lines, 2, 30, 6)).toBe(0);
    expect(isMusicLine("♪")).toBe(true);
    expect(isMusicLine("Instrumental")).toBe(true);
    expect(isMusicLine("instrumentaal")).toBe(true);
    expect(isMusicLine("Text")).toBe(false);
  });
});

describe("formatTime", () => {
  it("formats seconds and handles invalid input", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(62.9)).toBe("1:02");
    expect(formatTime(Number.NaN)).toBe("0:00");
    expect(formatTime(-1)).toBe("0:00");
  });
});

describe("playback anchor", () => {
  it("interpolates a playing position and keeps a paused one fixed", () => {
    expect(targetPosition({ position: 10, age: 2, playing: true, rate: 1 })).toBe(12);
    expect(targetPosition({ position: 10, age: 2, playing: false, rate: 1 })).toBe(10);
    expect(targetPosition({ position: 10, age: 2, playing: true, rate: 1, length: 11 })).toBe(11);
    expect(targetPosition({ position: Number.NaN, age: 2, playing: true, rate: 1 })).toBeNull();
    expect(targetPosition({ position: null, age: 0, playing: true, rate: 1 })).toBeNull();
  });

  it("resets on a new track", () => {
    const previous: Anchor = { pos: 10, at: 0, playing: true, rate: 1, ok: true };
    expect(updateAnchor(previous, false, 2, true, 1, 1000)).toEqual({
      pos: 2,
      at: 1000,
      playing: true,
      rate: 1,
      ok: true,
    });
  });

  it("follows a large seek and smooths a small correction", () => {
    const previous: Anchor = { pos: 10, at: 0, playing: true, rate: 1, ok: true };
    const followed = updateAnchor(previous, true, 20, true, 1, 2000);
    expect(followed.pos).toBe(20);

    const smoothed = updateAnchor(previous, true, 12.4, true, 1, 2000);
    expect(smoothed.pos).toBeCloseTo(12.1);
  });

  it("does not move backwards on a small negative drift", () => {
    const previous: Anchor = { pos: 12, at: 0, playing: true, rate: 1, ok: true };
    const updated = updateAnchor(previous, true, 13.8, true, 1, 2000);
    expect(updated.pos).toBe(14);
  });

  it("keeps a paused anchor still instead of extrapolating it forward", () => {
    const previous: Anchor = { pos: 5, at: 0, playing: false, rate: 1, ok: true };
    const updated = updateAnchor(previous, true, 5.01, false, 1, 5000);
    expect(updated.pos).toBeCloseTo(5.0025);
    expect(updated.playing).toBe(false);
  });

  it("snaps to the server position when playback starts or stops", () => {
    const playing: Anchor = { pos: 10, at: 0, playing: true, rate: 1, ok: true };
    const paused = updateAnchor(playing, true, 10.2, false, 1, 2000);
    expect(paused.pos).toBe(10.2);
    expect(paused.playing).toBe(false);

    const resumed = updateAnchor(paused, true, 10.3, true, 1, 3000);
    expect(resumed.pos).toBe(10.3);
    expect(resumed.playing).toBe(true);
  });

  it("returns a live position only when the anchor is valid", () => {
    const anchor: Anchor = { pos: 10, at: 1000, playing: true, rate: 2, ok: true };
    expect(positionAt(anchor, 2000)).toBe(12);
    expect(positionAt({ ...anchor, playing: false }, 2000)).toBe(10);
    expect(positionAt(null, 2000)).toBeNull();
  });
});
