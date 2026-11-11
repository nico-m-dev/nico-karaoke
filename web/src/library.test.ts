import { describe, expect, it } from "vitest";
import { matchesLibraryQuery, spotifySearchUrl, textIsDirty, youtubeSearchUrl } from "./library";

const item = {
  title: "Blue Monday",
  artist: "New Order",
  album: "Power, Corruption & Lies",
};

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

describe("search links", () => {
  it("encodes title and artist", () => {
    expect(youtubeSearchUrl("Song & More", "Art ist")).toContain("Song%20%26%20More%20Art%20ist");
    expect(spotifySearchUrl("Song & More", "Art ist")).toContain("Song%20%26%20More%20Art%20ist");
  });
});

describe("textIsDirty", () => {
  it("detects a changed draft", () => {
    expect(textIsDirty("new", "old")).toBe(true);
    expect(textIsDirty("same", "same")).toBe(false);
  });
});
