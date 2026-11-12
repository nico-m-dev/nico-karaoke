/** Window in which the UI trusts its own play/pause intent over a stale poll. */
export const TRANSPORT_INTENT_MS = 1500;

export type TransportIntent = {
  playing: boolean;
  until: number;
};

export function startTransportIntent(playing: boolean, now: number): TransportIntent {
  return { playing, until: now + TRANSPORT_INTENT_MS };
}

/** The intended state while the intent is fresh, otherwise null. */
export function activeTransportIntent(
  intent: TransportIntent | null,
  now: number,
): TransportIntent | null {
  return intent && now < intent.until ? intent : null;
}

/**
 * Drop the intent once the player reports it, or when it has expired. A fresh mismatch keeps
 * the intent, because the poll can describe the state before the control request.
 */
export function reconcileTransportIntent(
  intent: TransportIntent | null,
  reported: boolean,
  now: number,
): TransportIntent | null {
  if (!intent) {
    return null;
  }
  if (now >= intent.until || reported === intent.playing) {
    return null;
  }
  return intent;
}
