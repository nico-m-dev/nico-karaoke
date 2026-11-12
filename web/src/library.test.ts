import { describe, expect, it } from "vitest";
import { currentFirst, matchesLibraryQuery, spotifySearchUrl, youtubeSearchUrl } from "./library";

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
