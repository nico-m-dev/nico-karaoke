export const MIN_SCALE = 5;
export const MAX_SCALE = 120;
export const PAD_LEFT = 18;
export const LABEL_ROOM = 220;
export const MIN_CANVAS = 320;
export const DEFAULT_SCALE = 40;

const TICK_STEPS = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];

export function clampScale(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SCALE;
  }
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, value));
}

export function xAtTime(time: number, scale: number): number {
  return PAD_LEFT + Math.max(0, time) * scale;
}

export function timeAtX(x: number, scale: number): number {
  return Math.max(0, (x - PAD_LEFT) / Math.max(1, scale));
}

export function canvasWidth(duration: number, scale: number, viewport = 0): number {
  const content = PAD_LEFT + Math.max(0, duration) * scale + LABEL_ROOM;
  return Math.max(MIN_CANVAS, viewport, content);
}

/** Fit the whole song without going below the minimum or above the maximum zoom. */
export function fitScale(duration: number, viewport: number): number {
  if (!Number.isFinite(duration) || duration <= 0 || viewport <= 0) {
    return DEFAULT_SCALE;
  }
  return clampScale((viewport - PAD_LEFT - LABEL_ROOM) / duration);
}

export function tickStep(scale: number): number {
  const safe = Math.max(1, scale);
  return TICK_STEPS.find((step) => step * safe >= 68) ?? TICK_STEPS[TICK_STEPS.length - 1] ?? 600;
}

export type Tick = {
  time: number;
  major: boolean;
  label: string;
};

export function rulerTicks(duration: number, scale: number): Tick[] {
  const step = tickStep(scale);
  const end = Math.max(0, duration);
  const ticks: Tick[] = [];
  for (let index = 0; index * step <= end + 0.001; index += 1) {
    const time = Math.round(index * step * 1000) / 1000;
    ticks.push({ time, major: index % 5 === 0, label: formatTick(time) });
  }
  const last = ticks[ticks.length - 1];
  if (last && last.time < end) {
    ticks.push({ time: end, major: false, label: formatTick(end) });
  }
  return ticks;
}

export function formatTick(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/**
 * Decide which marker labels fit. A label is drawn when both neighbouring markers are far
 * enough away, so a dense cluster does not draw overlapping text. The selected marker is
 * forced visible by the view.
 */
export function markerLabelFits(positions: number[], minimumGap = 58): boolean[] {
  return positions.map((position, index) => {
    const previous = positions[index - 1];
    const next = positions[index + 1];
    const clearBefore = previous === undefined || position - previous >= minimumGap;
    const clearAfter = next === undefined || next - position >= minimumGap;
    return clearBefore && clearAfter;
  });
}
