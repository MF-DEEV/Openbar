// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, it, expect, vi } from "vitest";
import { createQueueStore, type QueueSnapshot } from "./queueStore";

const queue = (title: string, count = 5): QueueSnapshot => ({
  source: "Spotify.exe",
  title,
  status: "ready",
  items: Array.from({ length: count }, (_, i) => ({ title: `Song ${i}`, artist: "Artist" })),
});
describe("upcoming queue ownership", () => {
  it("fills the fifth visible slot immediately from lookahead while the new native read is pending", async () => {
    const initial = {
      ...queue("Now"),
      prefetch: [
        { title: "Song 5", artist: "Artist" },
        { title: "Song 6", artist: "Artist" },
      ],
    };
    const store = createQueueStore((_, title) => (title === "Now" ? Promise.resolve(initial) : new Promise(() => {})));
    await store.getState().refresh("Spotify.exe", "Now");
    const kept = store
      .getState()
      .snapshot!.items.slice(1)
      .map((item) => item.id);
    void store.getState().refresh("Spotify.exe", "Song 0");
    expect(store.getState().snapshot!.items.map((item) => item.title)).toEqual([
      "Song 1",
      "Song 2",
      "Song 3",
      "Song 4",
      "Song 5",
    ]);
    expect(
      store
        .getState()
        .snapshot!.items.slice(0, 4)
        .map((item) => item.id),
    ).toEqual(kept);
    expect(store.getState().snapshot!.prefetch).toEqual([{ title: "Song 6", artist: "Artist" }]);
  });
  it("shows at most five real entries and preserves their order", async () => {
    const store = createQueueStore(async () => queue("Now", 8));
    await store.getState().refresh("Spotify.exe", "Now");
    expect(store.getState().snapshot?.items.map((s) => s.title)).toEqual([
      "Song 0",
      "Song 1",
      "Song 2",
      "Song 3",
      "Song 4",
    ]);
  });
  it("keeps repeated songs and does not pad a short queue", async () => {
    const snapshot = queue("Now", 2);
    snapshot.items[1] = { ...snapshot.items[0] };
    const store = createQueueStore(async () => snapshot);
    await store.getState().refresh("Spotify.exe", "Now");
    expect(store.getState().snapshot?.items.map(({ title, artist }) => ({ title, artist }))).toEqual([
      snapshot.items[0],
      snapshot.items[0],
    ]);
    expect(new Set(store.getState().snapshot?.items.map((item) => item.id)).size).toBe(2);
  });
  it("rejects another player's queue even when its current song has the same title", async () => {
    const store = createQueueStore(async () => ({ ...queue("Now"), source: "Other.exe" }));
    await store.getState().refresh("Spotify.exe", "Now");
    expect(store.getState().snapshot).toMatchObject({ status: "unavailable", items: [] });
  });
  it("retains rows through a brief native failure, then expires them", async () => {
    const clock = vi.spyOn(Date, "now").mockReturnValue(1000);
    let fails = false;
    const store = createQueueStore(async () => {
      if (fails) throw Error("Disconnected");
      return queue("Now");
    });
    await store.getState().refresh("Spotify.exe", "Now");
    fails = true;
    await store.getState().refresh("Spotify.exe", "Now");
    expect(store.getState().snapshot?.items).toHaveLength(5);
    expect(store.getState().recovering).toBe(true);
    clock.mockReturnValue(8000);
    await store.getState().refresh("Spotify.exe", "Now");
    expect(store.getState().snapshot).toMatchObject({ status: "unavailable", items: [] });
    clock.mockRestore();
  });
  it("rejects a late snapshot from the previous track", async () => {
    let finish!: (q: QueueSnapshot) => void;
    const store = createQueueStore((_, title) =>
      title === "Old"
        ? new Promise((resolve) => {
            finish = resolve;
          })
        : Promise.resolve(queue(title)),
    );
    const old = store.getState().refresh("Spotify.exe", "Old");
    await store.getState().refresh("Spotify.exe", "New");
    finish(queue("Old"));
    await old;
    expect(store.getState().snapshot?.title).toBe("New");
  });
  it("does not restore a queue after closing it", async () => {
    let finish!: (q: QueueSnapshot) => void;
    const store = createQueueStore(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    store.getState().toggle();
    const request = store.getState().refresh("Spotify.exe", "Now");
    store.getState().close();
    finish(queue("Now"));
    await request;
    expect(store.getState()).toMatchObject({ expanded: false, snapshot: null, loading: false });
  });
  it("rejects a snapshot anchored to another source/song and does not invent rows on failure", async () => {
    const store = createQueueStore(async () => queue("Wrong"));
    await store.getState().refresh("Spotify.exe", "Now");
    expect(store.getState().snapshot).toMatchObject({ status: "unavailable", items: [] });
  });
  it("distinguishes an empty real queue from an unsupported player", async () => {
    const store = createQueueStore(async () => queue("Now", 0));
    await store.getState().refresh("Spotify.exe", "Now");
    expect(store.getState().snapshot).toMatchObject({ status: "ready", items: [] });
  });
  it("slides existing rows forward immediately and preserves their IDs through the refresh", async () => {
    let finish!: (q: QueueSnapshot) => void;
    const store = createQueueStore((_, title) =>
      title === "Now"
        ? Promise.resolve(queue("Now"))
        : new Promise((resolve) => {
            finish = resolve;
          }),
    );
    await store.getState().refresh("Spotify.exe", "Now");
    const previous = store.getState().snapshot!.items;
    const request = store.getState().refresh("Spotify.exe", "Song 0");
    expect(store.getState().snapshot?.items).toEqual(previous.slice(1));
    expect(store.getState().loading).toBe(true);
    finish({
      ...queue("Song 0"),
      items: Array.from({ length: 5 }, (_, i) => ({ title: `Song ${i + 1}`, artist: "Artist" })),
    });
    await request;
    expect(
      store
        .getState()
        .snapshot?.items.slice(0, 4)
        .map((item) => item.id),
    ).toEqual(previous.slice(1).map((item) => item.id));
    expect(store.getState().snapshot?.items).toHaveLength(5);
  });
  it("keeps the mounted list during a same-player jump and clears it for a different player", async () => {
    const store = createQueueStore((_, title) =>
      title === "Now" ? Promise.resolve(queue("Now")) : new Promise(() => {}),
    );
    await store.getState().refresh("Spotify.exe", "Now");
    const previous = store.getState().snapshot;
    void store.getState().refresh("Spotify.exe", "Unrelated song");
    expect(store.getState().snapshot).toBe(previous);
    void store.getState().refresh("Other.exe", "Other song");
    expect(store.getState().snapshot).toBeNull();
  });
});
