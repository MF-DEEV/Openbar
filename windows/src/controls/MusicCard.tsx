// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The player on the home page (NotchMusicControlsView, NotchMusicView.swift
// :724-875; spec 04 §1.2): the cover, the title and artist, and the compact
// transport at the end of their line, or under the title where the card is
// narrow. The cover and the title open the Music page.

import { FastForward, Pause, Play, Rewind } from "lucide-react";
import { useIsland } from "../island/store";
import { Cover } from "../music/MusicPage";
import { isPlaying, useMusic } from "../music/store";
import { S } from "./strings";

export function MusicCard({ height }: { height: number }) {
  const playback = useMusic((s) => s.playback);
  const playing = useMusic(isPlaying);
  const failed = useMusic((s) => s.failed);
  const command = useMusic((s) => s.command);
  const cover = Math.max(40, height - 24);
  const open = () => useIsland.getState().select("music");
  const artist = failed ? S.playbackFailed : playback ? playback.artist || playback.album : S.musicHint;
  return (
    <div className="ct-music u-surface" style={{ height }}>
      <button type="button" className="ct-music-cover" aria-label={S.nowPlaying} title={S.nowPlaying} onClick={open}>
        <Cover src={playback?.artwork} size={cover} radius={cover * 0.19} dimmed={!!playback && !playing} />
      </button>
      <div className="ct-music-body">
        <button type="button" className="ct-music-titles" onClick={open}>
          <span className="ct-music-title">{playback ? playback.title || playback.sourceName : S.nothingPlaying}</span>
          {artist ? <span className={`ct-music-artist${failed ? " is-failed" : ""}`}>{artist}</span> : null}
        </button>
        {playback ? (
          <div className="ct-music-transport">
            <button
              type="button"
              className="ct-music-button"
              aria-label={S.previous}
              title={S.previous}
              disabled={!playback.canPrevious}
              onClick={() => void command("previous")}
            >
              <Rewind size={16} fill="currentColor" strokeWidth={0} />
            </button>
            <button
              type="button"
              className="ct-music-button is-play"
              aria-label={playing ? S.pause : S.play}
              title={playing ? S.pause : S.play}
              disabled={!playback.canPlayPause}
              onClick={() => void command("toggle")}
            >
              {playing ? (
                <Pause size={22} fill="currentColor" strokeWidth={0} />
              ) : (
                <Play size={22} fill="currentColor" strokeWidth={0} />
              )}
            </button>
            <button
              type="button"
              className="ct-music-button"
              aria-label={S.next}
              title={S.next}
              disabled={!playback.canNext}
              onClick={() => void command("next")}
            >
              <FastForward size={16} fill="currentColor" strokeWidth={0} />
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
