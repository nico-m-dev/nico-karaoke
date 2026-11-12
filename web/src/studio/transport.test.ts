import { describe, expect, it } from "vitest";
import {
  TRANSPORT_INTENT_MS,
  activeTransportIntent,
  reconcileTransportIntent,
  startTransportIntent,
} from "./transport";

describe("transport intent", () => {
  it("keeps the intended state while a poll still reports the old one", () => {
    const intent = startTransportIntent(true, 1000);
    expect(activeTransportIntent(intent, 1200)?.playing).toBe(true);
    expect(reconcileTransportIntent(intent, false, 1200)).toBe(intent);
  });

  it("drops the intent when the player reports it", () => {
    const intent = startTransportIntent(false, 1000);
    expect(reconcileTransportIntent(intent, false, 1200)).toBeNull();
  });

  it("expires after the intent window", () => {
    const intent = startTransportIntent(true, 1000);
    expect(activeTransportIntent(intent, 1000 + TRANSPORT_INTENT_MS)).toBeNull();
    expect(reconcileTransportIntent(intent, false, 1000 + TRANSPORT_INTENT_MS)).toBeNull();
  });

  it("handles a missing intent", () => {
    expect(activeTransportIntent(null, 0)).toBeNull();
    expect(reconcileTransportIntent(null, true, 0)).toBeNull();
  });
});
