import { describe, expect, it } from "vitest";
import {
  MAX_UNDO,
  deleteLine,
  formatTimecode,
  insertLine,
  insertMusicMarker,
  isMusicMarker,
  makeEditorLines,
  nudgeLine,
  parseTimecode,
  popUndo,
  pushUndo,
  serializeLines,
  sortLines,
  takeTime,
  timelineDuration,
  toggleMusic,
  type EditorLine,
} from "./model";

const source = [
  { t: 1, text: "First", end: null },
  { t: 5, text: "Second", end: 7.5 },
  { t: 12, text: "Third", end: null },
];

describe("makeEditorLines", () => {
  it("assigns stable ids and drops legacy end tags", () => {
    const result = makeEditorLines(source);
    expect(result.lines.map((line) => line.id)).toEqual(["line-1", "line-2", "line-3", "line-4"]);
    expect(result.lines.map((line) => line.t)).toEqual([1, 5, 7.5, 12]);
    expect(result.migrated).toBe(1);
    expect(result.nextId).toBe(5);
  });

  it("inserts a music marker when the end tag leaves a real gap", () => {
    const result = makeEditorLines(source);
    expect(result.lines.map((line) => line.text)).toEqual(["First", "Second", "♪", "Third"]);
    expect(result.lines[2]?.t).toBe(7.5);
  });

  it("does not insert a marker when the next line follows immediately", () => {
    const result = makeEditorLines([
      { t: 1, text: "First", end: 1.2 },
      { t: 1.3, text: "Second", end: null },
    ]);
    expect(result.lines.map((line) => line.text)).toEqual(["First", "Second"]);
  });

  it("deduplicates markers from duplicate end tags", () => {
    const result = makeEditorLines([
      { t: 1, text: "First", end: 3 },
      { t: 1, text: "First again", end: 3.05 },
      { t: 10, text: "Next", end: null },
    ]);
    expect(result.lines.filter((line) => isMusicMarker(line.text))).toHaveLength(1);
  });

  it("turns timed blank lines into music markers", () => {
    const result = makeEditorLines([
      { t: 5, text: "" },
      { t: 8, text: "   " },
      { t: 12, text: "Line", end: null },
    ]);
    expect(result.lines.map((line) => line.text)).toEqual(["♪", "♪", "Line"]);
    expect(result.normalized).toBe(2);
  });

  it("keeps plain lines after timed lines", () => {
    const result = makeEditorLines([
      { t: null, text: "Plain" },
      { t: 2, text: "Timed", end: null },
    ]);
    expect(result.lines.map((line) => line.text)).toEqual(["Timed", "Plain"]);
  });
});

describe("timecodes", () => {
  it("formats minutes with centiseconds", () => {
    expect(formatTimecode(0)).toBe("00:00.00");
    expect(formatTimecode(83.456)).toBe("01:23.46");
    expect(formatTimecode(3671.2)).toBe("61:11.20");
    expect(formatTimecode(null)).toBe("");
  });

  it("parses bracket, minute, and second forms", () => {
    expect(parseTimecode("[01:23.45]")).toBe(83.45);
    expect(parseTimecode("1:23")).toBe(83);
    expect(parseTimecode("0:02,50")).toBe(2.5);
    expect(parseTimecode("12.345")).toBe(12.345);
    expect(parseTimecode("")).toBeNull();
    expect(parseTimecode("abc")).toBeNull();
    expect(parseTimecode("1:2:3:4")).toBeNull();
  });
});

describe("serialization", () => {
  it("round-trips timed and plain lines", () => {
    const lines: EditorLine[] = [
      { id: "a", t: 1.5, text: "First" },
      { id: "b", t: 3, text: "" },
      { id: "c", t: null, text: "Plain" },
    ];
    const text = serializeLines(lines);
    expect(text).toBe("[00:01.50] First\n[00:03.00]\nPlain");
    expect(
      makeEditorLines([
        { t: 1.5, text: "First" },
        { t: 3, text: "" },
        { t: null, text: "Plain" },
      ]).lines,
    ).toHaveLength(3);
  });
});

describe("line operations", () => {
  const lines: EditorLine[] = [
    { id: "a", t: 1, text: "First" },
    { id: "b", t: 5, text: "Second" },
  ];

  it("sorts after a nudge and clamps at zero", () => {
    const nudged = nudgeLine(lines, "b", -4.5);
    expect(nudged.map((line) => line.id)).toEqual(["b", "a"]);
    expect(nudged[0]?.t).toBe(0.5);
    expect(nudgeLine(lines, "a", -5)[0]?.t).toBe(0);
  });

  it("takes the current time and rounds it", () => {
    expect(takeTime(lines, "a", 2.34567)[0]?.t).toBe(2.346);
  });

  it("toggles music text", () => {
    const music = toggleMusic(lines, "a");
    expect(music[0]?.text).toBe("♪");
    expect(isMusicMarker(music[0]?.text ?? "")).toBe(true);
    expect(toggleMusic(music, "a")[0]?.text).toBe("");
  });

  it("inserts and deletes lines", () => {
    const added = insertLine(lines, "c", 3, "Middle");
    expect(added.lines.map((line) => line.id)).toEqual(["a", "c", "b"]);
    expect(deleteLine(added.lines, "c")).toHaveLength(2);
  });

  it("deduplicates music markers within 0.1 seconds", () => {
    const first = insertMusicMarker(lines, "m", 3);
    expect(first.created).toBe(true);
    const second = insertMusicMarker(first.lines, "m2", 3.05);
    expect(second.created).toBe(false);
    expect(second.id).toBe("m");
    expect(second.lines.filter((line) => isMusicMarker(line.text))).toHaveLength(1);
  });

  it("keeps sorting stable for equal times", () => {
    const sorted = sortLines([
      { id: "x", t: 5, text: "X" },
      { id: "y", t: 1, text: "Y" },
      { id: "z", t: 5, text: "Z" },
    ]);
    expect(sorted.map((line) => line.id)).toEqual(["y", "x", "z"]);
  });
});

describe("undo", () => {
  it("caps the stack and pops the newest snapshot", () => {
    let stack: { lines: EditorLine[]; selectedId: string | null }[] = [];
    for (let index = 0; index < MAX_UNDO + 5; index += 1) {
      stack = pushUndo(stack, { lines: [], selectedId: `id-${index}` });
    }
    expect(stack).toHaveLength(MAX_UNDO);
    const popped = popUndo(stack);
    expect(popped.snapshot?.selectedId).toBe(`id-${MAX_UNDO + 4}`);
    expect(popped.stack).toHaveLength(MAX_UNDO - 1);
    expect(popUndo([]).snapshot).toBeNull();
  });
});

describe("timelineDuration", () => {
  it("prefers a real track length", () => {
    expect(timelineDuration(210, [{ id: "a", t: 3, text: "x" }])).toBe(210);
  });

  it("falls back to the last line plus a margin", () => {
    expect(timelineDuration(null, [{ id: "a", t: 40, text: "x" }])).toBe(55);
    expect(timelineDuration(null, [])).toBe(30);
  });
});
