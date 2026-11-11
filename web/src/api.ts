import type { LyricLine } from "./player";

export type PlayerInfo = {
  player: string;
  label: string;
  title: string;
  artist: string;
  playing: boolean;
};

export type Track = {
  player: string;
  label: string;
  title: string;
  artist: string;
  album: string;
  track_id: string;
  length: number | null;
  position: number | null;
  age: number;
  playing: boolean;
  status: string;
  can_seek: boolean;
  can_play: boolean;
  rate: number;
};

export type OriginalLyrics = {
  source: string;
  fetched_at: number;
  lrclib_id: number | null;
  synced: boolean;
  same: boolean;
};

export type Lyrics = {
  state: "none" | "loading" | "found" | "missing" | "error";
  signature: string | null;
  record_key: string | null;
  synced: boolean;
  offset: number;
  source: string | null;
  title: string;
  artist: string;
  album: string;
  duration: number | null;
  text: string;
  lines: LyricLine[];
  lrclib_id: number | null;
  fetched_at: number;
  updated_at: number;
  original: OriginalLyrics | null;
  error: string | null;
};

export type Now = {
  track: Track | null;
  players: PlayerInfo[];
  lyrics: Lyrics;
};

export type LibrarySummary = {
  key: string;
  title: string;
  artist: string;
  album: string;
  duration: number | null;
  source: string;
  synced: boolean;
  offset: number;
  updated_at: number;
  line_count: number;
  has_ends: boolean;
  has_original: boolean;
  current: boolean;
};

export type LibraryResponse = {
  items: LibrarySummary[];
  current: string | null;
  signature: string | null;
};

export type LibraryItemResponse = {
  lyrics: Lyrics;
  current: boolean;
};

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
  ) {
    super(code);
    this.name = "ApiError";
  }
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    cache: "no-store",
    ...init,
    signal: init?.signal ?? AbortSignal.timeout(5000),
    headers: {
      Accept: "application/json",
      ...(init?.headers ?? {}),
    },
  });

  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const code =
      typeof payload === "object" &&
      payload !== null &&
      "error" in payload &&
      typeof payload.error === "string"
        ? payload.error
        : "request_failed";
    throw new ApiError(response.status, code);
  }
  return payload as T;
}

export function getNow(): Promise<Now> {
  return request<Now>("/api/now");
}

export function choosePlayer(player: string): Promise<{ ok: boolean }> {
  return request<{ ok: boolean }>("/api/player", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ player }),
  });
}

export function togglePlayback(): Promise<{ ok: boolean }> {
  return request<{ ok: boolean }>("/api/control", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ action: "playpause" }),
  });
}

export function getLibrary(): Promise<LibraryResponse> {
  return request<LibraryResponse>("/api/library");
}

export function getLibraryItem(key: string): Promise<LibraryItemResponse> {
  return request<LibraryItemResponse>(`/api/library/${encodeURIComponent(key)}`);
}

export function saveLibraryItem(
  key: string,
  text: string,
  source = "manual",
): Promise<LibraryItemResponse> {
  return request<LibraryItemResponse>(`/api/library/${encodeURIComponent(key)}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text, source }),
  });
}

export function deleteLibraryItem(key: string): Promise<{ ok: boolean }> {
  return request<{ ok: boolean }>(`/api/library/${encodeURIComponent(key)}`, {
    method: "DELETE",
  });
}

export function restoreLibraryOriginal(key: string): Promise<LibraryItemResponse> {
  return request<LibraryItemResponse>(`/api/library/${encodeURIComponent(key)}/original`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({}),
  });
}
