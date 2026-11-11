export const DEFAULT_TAP_CORRECTION = -0.2;
export const MAX_TAP_CORRECTION = 2;

export type TapSession = {
  index: number;
};

export function startTap(): TapSession {
  return { index: 0 };
}

/** Apply the reaction-time correction and keep the result inside the song. */
export function correctedTapTime(
  position: number,
  correction: number,
  duration: number | null,
): number {
  const corrected = Math.max(0, position + correction);
  const limited =
    duration !== null && Number.isFinite(duration) && duration > 0
      ? Math.min(corrected, duration)
      : corrected;
  return Math.round(limited * 1000) / 1000;
}

export function nextTap(
  session: TapSession,
  lineCount: number,
): { session: TapSession; index: number } | null {
  if (session.index >= lineCount) {
    return null;
  }
  return { session: { index: session.index + 1 }, index: session.index };
}

export function backspaceTap(session: TapSession): { session: TapSession; index: number } | null {
  if (session.index === 0) {
    return null;
  }
  return { session: { index: session.index - 1 }, index: session.index - 1 };
}

export function parseCorrection(value: string): number {
  const number = Number.parseFloat(value.replace(",", "."));
  if (!Number.isFinite(number)) {
    return DEFAULT_TAP_CORRECTION;
  }
  return Math.min(MAX_TAP_CORRECTION, Math.max(-MAX_TAP_CORRECTION, number));
}
