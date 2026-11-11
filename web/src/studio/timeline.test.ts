import { describe, expect, it } from "vitest";
import {
  MAX_SCALE,
  MIN_SCALE,
  PAD_LEFT,
  canvasWidth,
  clampScale,
  fitScale,
  markerLabelFits,
  rulerTicks,
  tickStep,
  timeAtX,
  xAtTime,
} from "./timeline";

describe("timeline scale", () => {
  it("clamps the zoom range", () => {
    expect(clampScale(1)).toBe(MIN_SCALE);
    expect(clampScale(500)).toBe(MAX_SCALE);
    expect(clampScale(Number.NaN)).toBe(40);
  });

  it("maps time to x and back", () => {
    expect(xAtTime(0, 10)).toBe(PAD_LEFT);
    expect(xAtTime(12.5, 10)).toBe(PAD_LEFT + 125);
    expect(timeAtX(PAD_LEFT + 125, 10)).toBe(12.5);
    expect(timeAtX(-50, 10)).toBe(0);
  });

  it("never draws narrower than the viewport", () => {
    expect(canvasWidth(10, 10, 800)).toBe(800);
    expect(canvasWidth(100, 10, 0)).toBe(PAD_LEFT + 1000 + 220);
  });

  it("fits the song into the viewport with clamping", () => {
    expect(fitScale(20, 1000)).toBeCloseTo((1000 - PAD_LEFT - 220) / 20, 5);
    expect(fitScale(200, 1000)).toBe(MIN_SCALE);
    expect(fitScale(3600, 1000)).toBe(MIN_SCALE);
    expect(fitScale(0, 1000)).toBe(40);
  });
});

describe("ruler ticks", () => {
  it("picks a readable step for the zoom level", () => {
    expect(tickStep(5)).toBe(15);
    expect(tickStep(40)).toBe(2);
    expect(tickStep(120)).toBe(1);
  });

  it("covers the song and marks every fifth tick", () => {
    const ticks = rulerTicks(12, 40);
    expect(ticks[0]).toEqual({ time: 0, major: true, label: "0:00" });
    expect(ticks.map((tick) => tick.time)).toEqual([0, 2, 4, 6, 8, 10, 12]);
    expect(ticks[5]?.major).toBe(true);
    expect(ticks[4]?.major).toBe(false);
    expect(ticks[6]?.label).toBe("0:12");
  });

  it("formats labels past the hour correctly", () => {
    expect(rulerTicks(130, 5).at(-1)?.label).toBe("2:10");
  });
});

describe("marker labels", () => {
  it("hides a label when a neighbour is too close", () => {
    expect(markerLabelFits([0, 80, 100], 58)).toEqual([true, false, false]);
  });

  it("labels markers with room on both sides", () => {
    expect(markerLabelFits([0, 100])).toEqual([true, true]);
  });

  it("hides every label in a dense cluster and keeps a single marker labelled", () => {
    expect(markerLabelFits([0, 10, 20])).toEqual([false, false, false]);
    expect(markerLabelFits([10])).toEqual([true]);
  });
});
