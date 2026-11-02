// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it, vi } from "vitest";
import { createArtworkStore, selectSecondaryArtwork, createArtworkLoader, selectArtwork } from "./queueArtwork";

const item = { title: "Out of My League", artist: "Fitz and The Tantrums" };
const metadata = {
  trackName: item.title,
  artistName: item.artist,
  artworkUrl100: "https://is1-ssl.mzstatic.com/image/cover.jpg",
  trackViewUrl: "https://music.apple.com/us/album/song/123?i=456",
};
describe("queue artwork", () => {
  it("publishes a cover only after its image is downloaded and decoded", async () => {
    let decoded!: () => void;
    const cover = { image: metadata.artworkUrl100, link: metadata.trackViewUrl };
    const prepare = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          decoded = resolve;
        }),
    );
    const store = createArtworkStore(async () => cover, prepare);
    const request = store.getState().request(item);
    await Promise.resolve();
    expect(Object.keys(store.getState().covers)).toHaveLength(0);
    decoded();
    await request;
    expect(Object.values(store.getState().covers)).toEqual([cover]);
  });
  it("does not publish an image that failed to load", async () => {
    const store = createArtworkStore(
      async () => ({ image: metadata.artworkUrl100, link: "" }),
      async () => {
        throw Error("Image offline");
      },
    );
    await store.getState().request(item);
    expect(store.getState().covers).toEqual({});
  });
  it("matches both recording title and artist and rejects another remix", () => {
    expect(
      selectArtwork(item, [
        { ...metadata, trackName: item.title + " (Remix)" },
        { ...metadata, artistName: "Other artist" },
        metadata,
      ]),
    ).toEqual({ image: metadata.artworkUrl100, link: metadata.trackViewUrl });
    expect(selectArtwork(item, [{ ...metadata, trackName: item.title + " (Remix)" }])).toBeNull();
  });
  it("rejects untrusted artwork URLs and invalid metadata", () => {
    expect(selectArtwork(item, [{ ...metadata, artworkUrl100: "https://example.com/cover.jpg" }])).toBeNull();
    expect(selectArtwork(item, [{ ...metadata, trackViewUrl: "javascript:alert(1)" }])).toBeNull();
    expect(selectArtwork(item, null)).toBeNull();
  });
  it("matches featured-artist title formatting without substituting a remix", () => {
    expect(selectArtwork({ ...item, title: item.title + " (FEAT. Guest)" }, [metadata])).not.toBeNull();
    expect(selectArtwork(item, [{ ...metadata, trackName: item.title + " (feat. Guest)" }])).not.toBeNull();
    expect(selectArtwork(item, [{ ...metadata, trackName: item.title + " (Live)" }])).toBeNull();
  });
  it("matches a full artist credit in parentheses while preserving edition suffixes", () => {
    const song = {
      title: "Am I Dreaming (Metro Boomin & A$AP Rocky, Roisee)",
      artist: "Metro Boomin, A$AP Rocky, Roisee",
    };
    const result = { ...metadata, trackName: "Am I Dreaming", artistName: "Metro Boomin" };
    expect(selectArtwork(song, [result])).not.toBeNull();
    expect(selectArtwork({ ...song, title: "Am I Dreaming (Live)" }, [result])).toBeNull();
  });
  it("coalesces requests and caches the same cover across row moves and skips", async () => {
    const fetcher = vi.fn(async () => new Response(JSON.stringify({ results: [metadata] })));
    const load = createArtworkLoader(fetcher);
    const [a, b] = await Promise.all([load(item), load({ ...item, id: "moved-row" })]);
    expect(a).toEqual(b);
    await load(item);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("caches missing matches and tolerates offline responses", async () => {
    const fetcher = vi.fn(async () => new Response(JSON.stringify({ results: [] })));
    const load = createArtworkLoader(fetcher);
    expect(await load(item)).toBeNull();
    expect(await load(item)).toBeNull();
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(
      await createArtworkLoader(async () => {
        throw Error("Offline");
      })(item),
    ).toBeNull();
  });
  it("uses the second catalog when a song is absent from iTunes and coalesces requests", async () => {
    const song = { title: "Palaces", artist: "EsDeeKid, Rico Ace" };
    const row = {
      title: "Palaces",
      artist: { name: "EsDeeKid" },
      album: { cover_medium: "https://cdn-images.dzcdn.net/images/cover/test/250x250.jpg" },
    };
    expect(
      selectSecondaryArtwork(song, {
        data: [
          { ...row, title: "Palaces (Remix)" },
          { ...row, artist: { name: "Another artist" } },
        ],
      }),
    ).toBeNull();
    expect(
      selectSecondaryArtwork(song, { data: [{ ...row, album: { cover_medium: "https://example.com/cover.jpg" } }] }),
    ).toBeNull();
    const fetcher = vi.fn(async () => new Response(JSON.stringify({ results: [] })));
    const secondary = vi.fn(async () => ({ data: [row] }));
    const load = createArtworkLoader(fetcher, secondary);
    const [first, duplicate] = await Promise.all([load(song), load(song)]);
    expect(first).toEqual({
      image: row.album.cover_medium,
      link: "",
    });
    expect(duplicate).toEqual(first);
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(secondary).toHaveBeenCalledTimes(1);
  });
});
