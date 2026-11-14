import { ApiError, checkUpdate, getUpdate, installUpdate, type UpdateStatus } from "./api";

type Listener = (status: UpdateStatus) => void;

type UpdateApi = {
  getUpdate: () => Promise<UpdateStatus>;
  checkUpdate: () => Promise<UpdateStatus>;
  installUpdate: () => Promise<UpdateStatus>;
};

const defaultApi: UpdateApi = { getUpdate, checkUpdate, installUpdate };

export type UpdateStore = {
  start: () => Promise<void>;
  refresh: () => Promise<void>;
  check: () => Promise<void>;
  install: () => Promise<void>;
  status: () => UpdateStatus | null;
  subscribe: (listener: Listener) => () => void;
};

/**
 * Local update state backed by the desktop shell's `/api/update` endpoints. The browser
 * server reports `supported: false`, so the same UI works without Tauri.
 */
export function createUpdateStore(api: UpdateApi = defaultApi): UpdateStore {
  let current: UpdateStatus | null = null;
  let started = false;
  let requestBusy = false;
  const listeners = new Set<Listener>();
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  function emit(status: UpdateStatus): void {
    current = status;
    for (const listener of listeners) {
      listener(status);
    }
    syncPolling();
  }

  function syncPolling(): void {
    if (current && (current.phase === "checking" || current.phase === "downloading")) {
      if (pollTimer === null) {
        pollTimer = globalThis.setInterval(() => {
          void refresh();
        }, 1000);
      }
    } else if (pollTimer !== null) {
      globalThis.clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  async function refresh(): Promise<void> {
    try {
      emit(await api.getUpdate());
    } catch {
      // The player already has an offline indicator; keep the last update state.
    }
  }

  function isConflict(error: unknown): boolean {
    return (
      error instanceof ApiError &&
      (error.code === "update_busy" || error.code === "update_not_available")
    );
  }

  async function check(): Promise<void> {
    if (requestBusy || !current?.supported) {
      return;
    }
    requestBusy = true;
    try {
      emit(await api.checkUpdate());
    } catch (error) {
      if (isConflict(error)) {
        await refresh();
        return;
      }
      emit(failed(error));
    } finally {
      requestBusy = false;
    }
  }

  async function install(): Promise<void> {
    if (requestBusy || !current?.supported) {
      return;
    }
    requestBusy = true;
    try {
      emit(await api.installUpdate());
    } catch (error) {
      if (isConflict(error)) {
        await refresh();
        return;
      }
      emit(failed(error));
    } finally {
      requestBusy = false;
    }
  }

  function failed(error: unknown): UpdateStatus {
    return {
      supported: true,
      phase: "error",
      version: null,
      notes: null,
      progress: null,
      error: error instanceof Error ? error.message : String(error),
    };
  }

  async function start(): Promise<void> {
    if (started) {
      return;
    }
    started = true;
    await refresh();
    if (current?.supported && current.phase === "idle") {
      await check();
    }
  }

  function subscribe(listener: Listener): () => void {
    listeners.add(listener);
    if (current) {
      listener(current);
    }
    return () => {
      listeners.delete(listener);
    };
  }

  return {
    start,
    refresh,
    check,
    install,
    status: () => current,
    subscribe,
  };
}
