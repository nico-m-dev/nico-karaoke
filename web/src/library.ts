import type { LibrarySummary, Lyrics, Track } from "./api";

export type LibraryFilter = "all" | "synced" | "unsynced" | "edited";
export type LibrarySort = "updated" | "title" | "artist";

export const LIBRARY_FILTERS: readonly LibraryFilter[] = ["all", "synced", "unsynced", "edited"];
export const LIBRARY_SORTS: readonly LibrarySort[] = ["updated", "title", "artist"];

export const DEFAULT_LIBRARY_FILTER: LibraryFilter = "all";
export const DEFAULT_LIBRARY_SORT: LibrarySort = "updated";

export const LIBRARY_FILTER_STORAGE_KEY = "nico.libraryFilter";
export const LIBRARY_SORT_STORAGE_KEY = "nico.librarySort";

type ReadableStorage = Pick<Storage, "getItem">;
type WritableStorage = Pick<Storage, "setItem">;

export function matchesLibraryQuery(
  item: Pick<LibrarySummary, "title" | "artist" | "album">,
  query: string,
): boolean {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) {
    return true;
  }
  return [item.title, item.artist, item.album].some((value) =>
    value.toLocaleLowerCase().includes(needle),
  );
}

/** "edited" means the working text is not the untouched LRCLIB copy. */
export function matchesLibraryFilter(
  item: Pick<LibrarySummary, "synced" | "source">,
  filter: LibraryFilter,
): boolean {
  switch (filter) {
    case "all":
      return true;
    case "synced":
      return item.synced;
    case "unsynced":
      return !item.synced;
    case "edited":
      return item.source !== "lrclib";
  }
}

export function sortLibrary(items: LibrarySummary[], sort: LibrarySort): LibrarySummary[] {
  const byText = (left: string, right: string) =>
    left.localeCompare(right, undefined, { sensitivity: "base" });
  return [...items].sort((left, right) => {
    if (sort === "title") {
      return byText(left.title, right.title) || byText(left.artist, right.artist);
    }
    if (sort === "artist") {
      return byText(left.artist, right.artist) || byText(left.title, right.title);
    }
    return right.updated_at - left.updated_at || byText(left.title, right.title);
  });
}

export function isLibraryFilter(value: string | null): value is LibraryFilter {
  return value !== null && (LIBRARY_FILTERS as readonly string[]).includes(value);
}

export function isLibrarySort(value: string | null): value is LibrarySort {
  return value !== null && (LIBRARY_SORTS as readonly string[]).includes(value);
}

export function detectLibraryFilter(storage: ReadableStorage = localStorage): LibraryFilter {
  try {
    const stored = storage.getItem(LIBRARY_FILTER_STORAGE_KEY);
    if (isLibraryFilter(stored)) {
      return stored;
    }
  } catch {
    // Storage can be unavailable in private or embedded contexts; use the default.
  }
  return DEFAULT_LIBRARY_FILTER;
}

export function rememberLibraryFilter(
  filter: LibraryFilter,
  storage: WritableStorage = localStorage,
): void {
  try {
    storage.setItem(LIBRARY_FILTER_STORAGE_KEY, filter);
  } catch {
    // A persisted choice is nice, not required.
  }
}

export function detectLibrarySort(storage: ReadableStorage = localStorage): LibrarySort {
  try {
    const stored = storage.getItem(LIBRARY_SORT_STORAGE_KEY);
    if (isLibrarySort(stored)) {
      return stored;
    }
  } catch {
    // Storage can be unavailable in private or embedded contexts; use the default.
  }
  return DEFAULT_LIBRARY_SORT;
}

export function rememberLibrarySort(
  sort: LibrarySort,
  storage: WritableStorage = localStorage,
): void {
  try {
    storage.setItem(LIBRARY_SORT_STORAGE_KEY, sort);
  } catch {
    // A persisted choice is nice, not required.
  }
}

/** Put the currently playing item first; the sort is stable, so the rest keeps its order. */
export function currentFirst<T extends { current: boolean }>(items: T[]): T[] {
  return [...items].sort((left, right) => Number(right.current) - Number(left.current));
}

/**
 * Whether the library should offer to create lyrics for the current track. Only a completed
 * lookup without a result counts; loading and temporary errors are not "no lyrics yet".
 */
export function showNewLyricsItem(
  track: Pick<Track, "title" | "artist" | "album"> | null,
  lyrics: Pick<Lyrics, "state">,
): boolean {
  return track !== null && lyrics.state === "missing";
}

export function youtubeSearchUrl(title: string, artist: string): string {
  return `https://www.youtube.com/results?search_query=${encodeURIComponent(
    `${title} ${artist}`.trim(),
  )}`;
}

export function spotifySearchUrl(title: string, artist: string): string {
  return `https://open.spotify.com/search/${encodeURIComponent(`${title} ${artist}`.trim())}`;
}
