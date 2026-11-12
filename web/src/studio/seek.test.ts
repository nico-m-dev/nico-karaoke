import { describe, expect, it } from "vitest";
import { SEEK_SETTLE_MS, isStaleSeekSample, startSeekGuard } from "./seek";

describe("seek guard", () => {
  it("ignores the pre-seek position while a forward seek settles", () => {
    const guard = startSeekGuard(100, true, 1, 1000);
    expect(isStaleSeekSample(guard, 40, 1100)).toBe(true);
    expect(isStaleSeekSample(guard, 40, 2200)).toBe(true);
  });

  it("accepts the sample once the player catches up", () => {
    const guard = startSeekGuard(100, true, 1, 1000);
    expect(isStaleSeekSample(guard, 100.5, 1600)).toBe(false);
  });

  it("ignores a position ahead of a backward seek", () => {
    const guard = startSeekGuard(30, true, 1, 1000);
    expect(isStaleSeekSample(guard, 300, 1100)).toBe(true);
    expect(isStaleSeekSample(guard, 30.4, 1500)).toBe(false);
  });

  it("trusts the exact position of a paused player", () => {
    const guard = startSeekGuard(30, false, 1, 1000);
    expect(isStaleSeekSample(guard, 30, 2000)).toBe(false);
    expect(isStaleSeekSample(guard, 90, 2000)).toBe(true);
  });

  it("stops guarding after the settle window", () => {
    const guard = startSeekGuard(100, true, 1, 0);
    expect(isStaleSeekSample(guard, 10, SEEK_SETTLE_MS + 1)).toBe(false);
  });

  it("follows the playback rate when it is not 1", () => {
    const guard = startSeekGuard(0, true, 2, 0);
    expect(isStaleSeekSample(guard, 2, 1000)).toBe(false);
    expect(isStaleSeekSample(guard, 0, 1000)).toBe(true);
  });
});
