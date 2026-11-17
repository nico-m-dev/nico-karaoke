import { describe, expect, it } from "vitest";
import type { LibrarySummary } from "./api";
import {
  currentFirst,
  detectLibraryFilter,
  detectLibrarySort,
  isLibraryFilter,
  isLibrarySort,
  matchesLibraryFilter,
  matchesLibraryQuery,
  rememberLibraryFilter,
  rememberLibrarySort,
  showNewLyricsItem,
  sortLibrary,
  spotifySearchUrl,
  youtubeSearchUrl,
} from "./library";

const item = {
  title: "Blue Monday",
  artist: "New Order",
  album: "Power, Corruption & Lies",
};

function summary(overrides: Partial<LibrarySummary> = {}): LibrarySummary {
  return {
    key: "key",
    title: "Song",
    artist: "Artist",
    album: "",
    duration: null,
    source: "lrclib",
    synced: true,
    offset: 0,
    updated_at: 0,
    line_count: 1,
    has_ends: false,
    has_original: false,
    current: false,
    ...overrides,
  };
}

describe("matchesLibraryQuery", () => {
  it("matches title, artist, and album case-insensitively", () => {
    expect(matchesLibraryQuery(item, "blue")).toBe(true);
    expect(matchesLibraryQuery(item, "NEW ORDER")).toBe(true);
    expect(matchesLibraryQuery(item, "corruption")).toBe(true);
  });

  it("returns true for an empty query", () => {
    expect(matchesLibraryQuery(item, "  ")).toBe(true);
  });

  it("rejects non-matches", () => {
    expect(matchesLibraryQuery(item, "nothing here")).toBe(false);
  });
});

describe("matchesLibraryFilter", () => {
  it("keeps everything for the all filter", () => {
    expect(matchesLibraryFilter(summary(), "all")).toBe(true);
  });

  it("separates synced and unsynced text", () => {
    expect(matchesLibraryFilter(summary({ synced: true }), "synced")).toBe(true);
    expect(matchesLibraryFilter(summary({ synced: false }), "synced")).toBe(false);
    expect(matchesLibraryFilter(summary({ synced: false }), "unsynced")).toBe(true);
    expect(matchesLibraryFilter(summary({ synced: true }), "unsynced")).toBe(false);
  });

  it("treats anything not from LRCLIB as an own edit", () => {
    expect(matchesLibraryFilter(summary({ source: "lrclib" }), "edited")).toBe(false);
    expect(matchesLibraryFilter(summary({ source: "manual" }), "edited")).toBe(true);
    expect(matchesLibraryFilter(summary({ source: "tap" }), "edited")).toBe(true);
  });
});

describe("sortLibrary", () => {
  const items = [
    summary({ key: "b", title: "beta", artist: "Zed", updated_at: 10 }),
    summary({ key: "a", title: "Alpha", artist: "zed", updated_at: 30 }),
    summary({ key: "c", title: "Gamma", artist: "Amy", updated_at: 20 }),
  ];

  it("sorts by most recently updated", () => {
    expect(sortLibrary(items, "updated").map((entry) => entry.key)).toEqual(["a", "c", "b"]);
  });

  it("sorts title and artist case-insensitively", () => {
    expect(sortLibrary(items, "title").map((entry) => entry.key)).toEqual(["a", "b", "c"]);
    expect(sortLibrary(items, "artist").map((entry) => entry.key)).toEqual(["c", "a", "b"]);
  });

  it("breaks text ties with the other field", () => {
    const tied = [
      summary({ key: "z", title: "Same", artist: "Zulu" }),
      summary({ key: "a", title: "Same", artist: "Alpha" }),
    ];
    expect(sortLibrary(tied, "title").map((entry) => entry.key)).toEqual(["a", "z"]);
  });

  it("does not mutate the input", () => {
    const original = items.map((entry) => entry.key);
    sortLibrary(items, "title");
    expect(items.map((entry) => entry.key)).toEqual(original);
  });
});

describe("library view preferences", () => {
  const storageFor = (key: string, value: string | null) => ({
    getItem: (requested: string) => (requested === key ? value : null),
  });

  it("accepts every supported filter and sort", () => {
    for (const filter of ["all", "synced", "unsynced", "edited"] as const) {
      expect(isLibraryFilter(filter)).toBe(true);
      expect(detectLibraryFilter(storageFor("nico.libraryFilter", filter))).toBe(filter);
    }
    for (const sort of ["updated", "title", "artist"] as const) {
      expect(isLibrarySort(sort)).toBe(true);
      expect(detectLibrarySort(storageFor("nico.librarySort", sort))).toBe(sort);
    }
  });

  it("falls back to the defaults for unsupported or unavailable storage", () => {
    const blocked = {
      getItem: () => {
        throw new Error("blocked");
      },
    };
    expect(detectLibraryFilter(storageFor("nico.libraryFilter", "unknown"))).toBe("all");
    expect(detectLibrarySort(storageFor("nico.librarySort", null))).toBe("updated");
    expect(detectLibraryFilter(blocked)).toBe("all");
    expect(detectLibrarySort(blocked)).toBe("updated");
  });

  it("reads only its own storage key", () => {
    expect(detectLibraryFilter(storageFor("other", "unsynced"))).toBe("all");
    expect(detectLibrarySort(storageFor("other", "artist"))).toBe("updated");
  });

  it("stores both choices and ignores a blocked write", () => {
    const values = new Map<string, string>();
    const writable = { setItem: (key: string, value: string) => values.set(key, value) };
    rememberLibraryFilter("unsynced", writable);
    rememberLibrarySort("artist", writable);
    expect(values.get("nico.libraryFilter")).toBe("unsynced");
    expect(values.get("nico.librarySort")).toBe("artist");

    const blocked = {
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(() => rememberLibraryFilter("all", blocked)).not.toThrow();
    expect(() => rememberLibrarySort("title", blocked)).not.toThrow();
  });
});

describe("showNewLyricsItem", () => {
  const track = { title: "Blue Monday", artist: "New Order", album: "" };

  it("offers creation only when a lookup finished without lyrics", () => {
    expect(showNewLyricsItem(track, { state: "missing" })).toBe(true);
    expect(showNewLyricsItem(track, { state: "found" })).toBe(false);
    expect(showNewLyricsItem(track, { state: "loading" })).toBe(false);
    expect(showNewLyricsItem(track, { state: "error" })).toBe(false);
  });

  it("does not offer creation without a track", () => {
    expect(showNewLyricsItem(null, { state: "missing" })).toBe(false);
  });
});

describe("search links", () => {
  it("encodes title and artist", () => {
    expect(youtubeSearchUrl("Song & More", "Art ist")).toContain("Song%20%26%20More%20Art%20ist");
    expect(spotifySearchUrl("Song & More", "Art ist")).toContain("Song%20%26%20More%20Art%20ist");
  });
});

describe("currentFirst", () => {
  it("moves the current item to the front and keeps the rest in order", () => {
    const items = [
      { title: "A", current: false },
      { title: "B", current: true },
      { title: "C", current: false },
    ];
    expect(currentFirst(items).map((item) => item.title)).toEqual(["B", "A", "C"]);
  });

  it("leaves the order unchanged when nothing is current", () => {
    const items = [
      { title: "A", current: false },
      { title: "B", current: false },
    ];
    expect(currentFirst(items).map((item) => item.title)).toEqual(["A", "B"]);
  });

  it("does not mutate the input", () => {
    const items = [
      { title: "A", current: false },
      { title: "B", current: true },
    ];
    currentFirst(items);
    expect(items.map((item) => item.title)).toEqual(["A", "B"]);
  });
});
