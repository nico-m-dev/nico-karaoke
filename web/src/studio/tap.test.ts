import { describe, expect, it } from "vitest";
import {
  DEFAULT_TAP_CORRECTION,
  backspaceTap,
  correctedTapTime,
  nextTap,
  parseCorrection,
  startTap,
} from "./tap";

describe("correctedTapTime", () => {
  it("applies the default reaction-time correction", () => {
    expect(correctedTapTime(10, DEFAULT_TAP_CORRECTION, null)).toBe(9.8);
  });

  it("clamps at zero and at the song end", () => {
    expect(correctedTapTime(0.1, -0.2, null)).toBe(0);
    expect(correctedTapTime(199.95, -0.2, 200)).toBe(199.75);
  });

  it("rounds to milliseconds", () => {
    expect(correctedTapTime(4.56789, 0, null)).toBe(4.568);
  });
});

describe("tap sequence", () => {
  it("walks forward through lines and stops at the end", () => {
    let session = startTap();
    const first = nextTap(session, 2);
    expect(first?.index).toBe(0);
    session = first?.session ?? session;
    const second = nextTap(session, 2);
    expect(second?.index).toBe(1);
    session = second?.session ?? session;
    expect(nextTap(session, 2)).toBeNull();
  });

  it("walks back and refuses before the first line", () => {
    const session = { index: 2 };
    const back = backspaceTap(session);
    expect(back).toEqual({ session: { index: 1 }, index: 1 });
    expect(backspaceTap({ index: 0 })).toBeNull();
  });
});

describe("parseCorrection", () => {
  it("accepts decimals and commas and clamps the range", () => {
    expect(parseCorrection("-0.2")).toBe(-0.2);
    expect(parseCorrection("0,35")).toBe(0.35);
    expect(parseCorrection("9")).toBe(2);
    expect(parseCorrection("-9")).toBe(-2);
    expect(parseCorrection("nope")).toBe(DEFAULT_TAP_CORRECTION);
  });
});
