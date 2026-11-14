import { describe, expect, it, vi } from "vitest";
import { ApiError, type UpdateStatus } from "./api";
import { createUpdateStore } from "./updates";

function status(overrides: Partial<UpdateStatus> = {}): UpdateStatus {
  return {
    supported: true,
    phase: "idle",
    version: null,
    notes: null,
    progress: null,
    error: null,
    ...overrides,
  };
}

function api(
  overrides: {
    getUpdate?: () => Promise<UpdateStatus>;
    checkUpdate?: () => Promise<UpdateStatus>;
    installUpdate?: () => Promise<UpdateStatus>;
  } = {},
) {
  return {
    getUpdate: vi.fn(overrides.getUpdate ?? (async () => status())),
    checkUpdate: vi.fn(
      overrides.checkUpdate ?? (async () => status({ phase: "available", version: "1.1.0" })),
    ),
    installUpdate: vi.fn(
      overrides.installUpdate ?? (async () => status({ phase: "available", version: "1.1.0" })),
    ),
  };
}

describe("createUpdateStore", () => {
  it("checks once at startup when the server is idle", async () => {
    const fake = api();
    const store = createUpdateStore(fake);

    await store.start();
    await store.start();

    expect(fake.getUpdate).toHaveBeenCalledTimes(1);
    expect(fake.checkUpdate).toHaveBeenCalledTimes(1);
    expect(store.status()?.phase).toBe("available");
  });

  it("does not check when the status is already resolved", async () => {
    const fake = api({
      getUpdate: async () => status({ phase: "up_to_date" }),
    });
    const store = createUpdateStore(fake);

    await store.start();

    expect(fake.getUpdate).toHaveBeenCalledTimes(1);
    expect(fake.checkUpdate).not.toHaveBeenCalled();
  });

  it("does not check an unsupported build", async () => {
    const fake = api({
      getUpdate: async () => status({ supported: false, phase: "disabled" }),
    });
    const store = createUpdateStore(fake);

    await store.start();

    expect(fake.checkUpdate).not.toHaveBeenCalled();
    expect(store.status()?.supported).toBe(false);
  });

  it("treats a busy check as a refresh instead of an error", async () => {
    const fake = api({
      checkUpdate: async () => {
        throw new ApiError(409, "update_busy");
      },
    });
    const store = createUpdateStore(fake);

    await store.start();

    expect(fake.getUpdate).toHaveBeenCalledTimes(2);
    expect(store.status()?.phase).toBe("idle");
  });

  it("ignores a second install while the first request is in flight", async () => {
    let resolveInstall: (value: UpdateStatus) => void = () => {};
    const fake = api({
      getUpdate: async () => status({ phase: "available", version: "1.1.0" }),
      installUpdate: () =>
        new Promise<UpdateStatus>((resolve) => {
          resolveInstall = resolve;
        }),
    });
    const store = createUpdateStore(fake);
    await store.start();

    const first = store.install();
    const second = store.install();
    expect(fake.installUpdate).toHaveBeenCalledTimes(1);

    resolveInstall(status({ phase: "available", version: "1.1.0" }));
    await Promise.all([first, second]);
  });

  it("refreshes when the installer reports no pending update", async () => {
    const fake = api({
      getUpdate: async () => status({ phase: "available", version: "1.1.0" }),
      installUpdate: async () => {
        throw new ApiError(409, "update_not_available");
      },
    });
    const store = createUpdateStore(fake);
    await store.start();

    await store.install();

    expect(store.status()?.phase).toBe("available");
    expect(store.status()?.error).toBe(null);
  });

  it("polls while checking and stops after a final status", async () => {
    vi.useFakeTimers();
    let current = status({ phase: "checking" });
    const fake = api({
      getUpdate: async () => current,
    });
    const store = createUpdateStore(fake);

    await store.start();
    expect(fake.getUpdate).toHaveBeenCalledTimes(1);

    current = status({ phase: "up_to_date" });
    await vi.advanceTimersByTimeAsync(1000);
    expect(fake.getUpdate).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(1000);
    expect(fake.getUpdate).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  it("notifies subscribers immediately with the current state", async () => {
    const fake = api({
      getUpdate: async () => status({ phase: "up_to_date" }),
    });
    const store = createUpdateStore(fake);
    await store.start();

    const seen: string[] = [];
    const unsubscribe = store.subscribe((next) => seen.push(next.phase));
    expect(seen).toEqual(["up_to_date"]);

    unsubscribe();
  });
});
