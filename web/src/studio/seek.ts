/** Time allowed for a player to report a position after a seek. */
export const SEEK_SETTLE_MS = 3000;

/** How far a sample may differ from the expected post-seek position and still be trusted. */
export const SEEK_TOLERANCE = 1.2;

export type SeekGuard = {
  position: number;
  at: number;
  playing: boolean;
  rate: number;
};

export function startSeekGuard(
  position: number,
  playing: boolean,
  rate: number,
  now: number,
): SeekGuard {
  return {
    position: Math.max(0, position),
    at: now,
    playing,
    rate: rate > 0 && Number.isFinite(rate) ? rate : 1,
  };
}

/**
 * True while a player sample still describes the position before a seek, or an intermediate
 * one. Ignoring those samples keeps the playhead at the requested position instead of
 * snapping back and forth until the player catches up.
 */
export function isStaleSeekSample(guard: SeekGuard, sample: number, now: number): boolean {
  const elapsed = Math.max(0, now - guard.at);
  if (elapsed > SEEK_SETTLE_MS) {
    return false;
  }
  const expected = guard.position + (guard.playing ? (elapsed / 1000) * guard.rate : 0);
  return Math.abs(sample - expected) > SEEK_TOLERANCE;
}
