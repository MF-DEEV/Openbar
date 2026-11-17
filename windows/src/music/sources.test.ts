// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
vi.mock("../shared/bridge", () => ({
  inTauri: false,
  bridge: {
    music: {
      selectSource: vi.fn(),
      now: vi.fn(),
      command: vi.fn(),
      seek: vi.fn(),
      shuffle: vi.fn(),
    },
  },
}));
vi.mock("./tint", () => ({ tintOf: async () => "#fff" }));
vi.mock("./lyricsStore", () => ({ useLyrics: { getState: () => ({ select: vi.fn() }) } }));
import { bridge } from "../shared/bridge";
import { loadPrefs, resolve, setPrefs } from "../shared/prefs";
import { useMusic } from "./store";
import { playerChoices } from "./SourcePicker";
import type { Playback, NowPlaying, Source } from "./types";

const song: Playback = {
  source: "music",
  sourceName: "Music",
  isMusicApp: true,
  title: "Song",
  artist: "Artist",
  album: "",
  isPlaying: true,
  elapsed: 0,
  duration: 100,
  rate: 1,
  sampledAt: 0,
  canPlayPause: true,
  canNext: true,
  canPrevious: true,
  canSeek: true,
  canShuffle: true,
  shuffle: false,
};
const sources: Source[] = [
  { id: "music", name: "Music", isMusicApp: true, isPlaying: true, hasTrack: true },
  { id: "browser", name: "Browser", isMusicApp: false, isPlaying: false, hasTrack: true },
];
const reading = (overrides: Partial<NowPlaying> = {}): NowPlaying => ({
  playback: song,
  sources,
  selectedSource: null,
  revision: 0,
  ...overrides,
});
beforeEach(() => {
  vi.resetAllMocks();
  vi.useFakeTimers();
  useMusic.setState({
    playback: null,
    sources: [],
    selectedSource: null,
    revision: 0,
    selecting: false,
    sourceError: null,
    failed: false,
    optimistic: null,
    tint: null,
  });
});
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe("music source selection", () => {
  it("clears old controls immediately and ignores old snapshots during a switch", async () => {
    useMusic.getState().receive(reading());
    let finish!: () => void;
    vi.mocked(bridge.music.selectSource).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const change = useMusic.getState().selectSource("browser");
    expect(useMusic.getState().playback).toBeNull();
    useMusic.getState().receive(reading());
    expect(useMusic.getState().playback).toBeNull();
    await useMusic.getState().command("toggle");
    expect(bridge.music.command).not.toHaveBeenCalled();
    useMusic.getState().receive(
      reading({
        selectedSource: "browser",
        revision: 1,
        playback: { ...song, source: "browser", sourceName: "Browser" },
      }),
    );
    finish();
    await change;
    expect(useMusic.getState().playback?.source).toBe("browser");
    useMusic.getState().receive(reading());
    expect(useMusic.getState().playback?.source).toBe("browser");
  });

  it("targets controls at the displayed session, including a manual fallback", async () => {
    useMusic.getState().receive(reading({ selectedSource: "closed", revision: 1 }));
    await useMusic.getState().command("next");
    await useMusic.getState().seek(20);
    await useMusic.getState().shuffle(true);
    expect(bridge.music.command).toHaveBeenCalledWith("next", "music");
    expect(bridge.music.seek).toHaveBeenCalledWith(20, "music");
    expect(bridge.music.shuffle).toHaveBeenCalledWith(true, "music");
    expect(useMusic.getState().selectedSource).toBe("closed");
  });

  it("does not retain playback or optimistic state when a player disconnects", () => {
    useMusic.getState().receive(reading());
    useMusic.setState({ optimistic: { playing: false, until: 10000 } });
    useMusic.getState().receive(reading({ playback: null, sources: [] }));
    expect(useMusic.getState().playback).toBeNull();
    expect(useMusic.getState().optimistic).toBeNull();
  });

  it("recovers a failed selection from the actual backend snapshot", async () => {
    vi.mocked(bridge.music.selectSource).mockRejectedValue(new Error("gone"));
    vi.mocked(bridge.music.now).mockResolvedValue(reading());
    await useMusic.getState().selectSource("browser");
    expect(useMusic.getState().sourceError).toContain("Could not change music source");
    expect(useMusic.getState().playback?.source).toBe("music");
    expect(useMusic.getState().selecting).toBe(false);
  });

  it("changing a player clears the previous player's optimistic play state", () => {
    useMusic.getState().receive(reading());
    useMusic.setState({ optimistic: { playing: false, until: 10000 } });
    useMusic.getState().receive(reading({ playback: { ...song, source: "browser" } }));
    expect(useMusic.getState().optimistic).toBeNull();
  });

  it("preserves saved unavailable choices and merges dynamic unknown players", () => {
    const options = playerChoices([sources[0]], sources, "uninstalled-player");
    expect(options.map((s) => s.id)).toEqual(["browser", "music", "uninstalled-player"]);
  });
});

describe("music preferences", () => {
  it("defaults to Automatic and includes other players", () => {
    const prefs = resolve(null).island;
    expect(prefs.preferredPlayer).toBe("");
    expect(prefs.includeOtherPlayers).toBe(true);
  });
  it("persists preferred player and filter through the existing settings API", async () => {
    await setPrefs({ island: { preferredPlayer: "custom-player", includeOtherPlayers: false } });
    const prefs = await loadPrefs();
    expect(prefs.island.preferredPlayer).toBe("custom-player");
    expect(prefs.island.includeOtherPlayers).toBe(false);
    await setPrefs({ island: { preferredPlayer: null, includeOtherPlayers: null } });
  });
  it("rejects malformed preferred identifiers", () => {
    expect(resolve({ island: { preferredPlayer: "x".repeat(257) } }).island.preferredPlayer).toBe("");
    expect(resolve({ island: { preferredPlayer: "bad\0id" } }).island.preferredPlayer).toBe("");
  });
});
