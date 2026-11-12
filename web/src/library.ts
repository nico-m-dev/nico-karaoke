import type { LibrarySummary } from "./api";

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

export function youtubeSearchUrl(title: string, artist: string): string {
  return `https://www.youtube.com/results?search_query=${encodeURIComponent(
    `${title} ${artist}`.trim(),
  )}`;
}

export function spotifySearchUrl(title: string, artist: string): string {
  return `https://open.spotify.com/search/${encodeURIComponent(`${title} ${artist}`.trim())}`;
}
