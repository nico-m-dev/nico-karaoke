import type { LibrarySummary, Lyrics, Track } from "./api";

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
