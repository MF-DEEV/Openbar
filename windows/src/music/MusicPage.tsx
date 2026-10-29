// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Music page (NotchMusicView.swift; spec 04 §1.2): a 68 pt cover with
// the title beside it, the timeline, and five equal transport slots so play
// stays centred. Idle, the buttons open the player.

import { FastForward, Music, Pause, Play, Rewind, Shuffle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { VolumeControl } from "./VolumeControl";
import { EqualizerBars } from "./EqualizerBars";
import { ScrollingText } from "./ScrollingText";
import { isPlaying, useMusic } from "./store";
import { position, time, type Playback } from "./types";
import { LyricsSection } from "./LyricsPanel";
import { useLyrics } from "./lyricsStore";

export const MUSIC_WIDTH = 344;
const COVER = 68;

/** head + 14 + (timeline 16 + 10 while it has one) + 46 + 8 (NotchLayout.player). */
export function playerHeight(p: Playback | null, lyricsExpanded = false): number {
  return COVER + 14 + (p && p.duration > 0 ? 16 + 10 : 0) + 46 + 8 + (p ? 38 + (lyricsExpanded ? 250 : 0) : 0);
}

export function Cover(props: { src?: string; size: number; radius: number; dimmed?: boolean }) {
  return (
    <span
      className="m-cover"
      style={{
        width: props.size,
        height: props.size,
        borderRadius: props.radius,
        transform: props.dimmed ? "scale(0.94)" : undefined,
      }}
    >
      {props.src ? <img src={props.src} alt="" draggable={false} /> : <Music size={props.size * 0.3} strokeWidth={2} />}
    </span>
  );
}

export function MusicPage() {
  const playback = useMusic((s) => s.playback);
  const sources = useMusic((s) => s.sources);
  const playing = useMusic(isPlaying);
  const tint = useMusic((s) => s.tint);
  const failed = useMusic((s) => s.failed);
  const command = useMusic((s) => s.command);
  const lyricsExpanded = useLyrics((s) => s.expanded);
  const [opening, setOpening] = useState(false);
  const openingPlayer = useRef(false);
  const [failedSource, setFailedSource] = useState<string | null>(null);
  const idlePlayer = sources.find((source) => source.isPlaying && source.id) ?? sources.find((source) => source.id);
  const playerSource = playback?.source || idlePlayer?.id;
  const playerName = (playback?.source ? playback.sourceName : idlePlayer?.name) || "music player";
  const openLabel = playerSource ? `Open ${playerName}` : "No music player available";
  const openFailed = !!playerSource && failedSource === playerSource;

  async function openPlayer() {
    if (!playerSource || openingPlayer.current) return;
    openingPlayer.current = true;
    setOpening(true);
    setFailedSource(null);
    try {
      if ((await bridge.music.openPlayer(playerSource)) === false) setFailedSource(playerSource);
    } catch {
      setFailedSource(playerSource);
    } finally {
      openingPlayer.current = false;
      setOpening(false);
    }
  }

  // Space toggles while the page has the keyboard.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== " " || (e.target as Element)?.closest?.("input,textarea,button,[data-lyrics-scroll]")) return;
      e.preventDefault();
      void command("toggle");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [command]);

  const accent = tint ?? "#fff";
  return (
    <div className="m-page">
      <div className="m-head">
        <button
          type="button"
          className="m-cover-button"
          aria-label={openLabel}
          title={openLabel}
          disabled={!playerSource}
          aria-disabled={opening || undefined}
          aria-busy={opening}
          onClick={() => void openPlayer()}
        >
          <Cover src={playback?.artwork} size={COVER} radius={COVER * 0.19} dimmed={!!playback && !playing} />
        </button>
        <div className="m-titles">
          <ScrollingText
            className="m-title"
            text={playback ? playback.title || playback.sourceName : "Nothing playing"}
          />
          <ScrollingText
            className={`m-artist${failed || openFailed ? " is-failed" : ""}`}
            text={
              openFailed
                ? "Could not open the music player. Try again."
                : failed
                  ? "Could not change playback."
                  : playback
                    ? playback.artist || playback.album || "Now playing"
                    : playerSource
                      ? openLabel
                      : "Your music and playback controls appear here."
            }
          />
        </div>
        {playback ? (
          <div className="m-accessory">
            <EqualizerBars count={4} width={2.5} height={15} color="rgba(255,255,255,0.5)" moving={playing} />
          </div>
        ) : null}
      </div>
      {playback && playback.duration > 0 ? <Timeline playback={playback} accent={accent} /> : null}
      <div className="m-transport">
        <Slot>
          {playback?.canShuffle ? (
            <button
              type="button"
              className="m-button is-small"
              aria-label="Shuffle"
              aria-pressed={playback.shuffle}
              onClick={() => void useMusic.getState().shuffle(!playback.shuffle)}
              style={{ color: playback.shuffle ? "#30d158" : "rgba(255,255,255,0.55)" }}
            >
              <Shuffle size={17} strokeWidth={2.25} />
            </button>
          ) : null}
        </Slot>
        <Slot>
          <button
            type="button"
            className="m-button"
            aria-label="Previous"
            disabled={!!playback && !playback.canPrevious}
            style={{ visibility: playback && !playback.canPrevious ? "hidden" : undefined }}
            onClick={() => (playback ? void command("previous") : undefined)}
          >
            <Rewind size={22} fill="currentColor" strokeWidth={0} />
          </button>
        </Slot>
        <Slot>
          <button
            type="button"
            className="m-button is-large"
            aria-label={playing ? "Pause" : "Play"}
            disabled={!playback?.canPlayPause}
            onClick={() => void command("toggle")}
          >
            {playing ? (
              <Pause size={32} fill="currentColor" strokeWidth={0} />
            ) : (
              <Play size={32} fill="currentColor" strokeWidth={0} />
            )}
          </button>
        </Slot>
        <Slot>
          <button
            type="button"
            className="m-button"
            aria-label="Next"
            disabled={!!playback && !playback.canNext}
            style={{ visibility: playback && !playback.canNext ? "hidden" : undefined }}
            onClick={() => (playback ? void command("next") : undefined)}
          >
            <FastForward size={22} fill="currentColor" strokeWidth={0} />
          </button>
        </Slot>
        <Slot>
          <VolumeControl
            key={playback?.source ?? "none"}
            source={playback?.source ?? ""}
            name={playback?.sourceName ?? "Player"}
            accent={accent}
          />
        </Slot>
      </div>
      {playback && (
        <>
          <button
            type="button"
            className="m-lyrics-toggle"
            aria-expanded={lyricsExpanded}
            aria-controls="music-lyrics"
            onClick={() => useLyrics.getState().toggle()}
          >
            Lyrics
          </button>
          {lyricsExpanded && <LyricsSection />}
        </>
      )}
    </div>
  );
}

function Slot({ children }: { children?: React.ReactNode }) {
  return <div className="m-slot">{children}</div>;
}

/**
 * Elapsed on the left, −remaining on the right, each reserving the widest
 * reading so the bar never changes length; dragging seeks once on release
 * and the thumb holds until the player catches up (±2 s, 1 s at most).
 */
function Timeline({ playback, accent }: { playback: Playback; accent: string }) {
  const [, tick] = useState(0);
  const [drag, setDrag] = useState<number | null>(null);
  const hold = useRef<{ value: number; until: number } | null>(null);
  useEffect(() => {
    if (!playback.isPlaying) return;
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [playback.isPlaying]);

  let now = position(playback);
  const h = hold.current;
  if (h && Math.abs(now - h.value) > 2 && Date.now() < h.until) now = h.value;
  else hold.current = null;
  const shown = drag ?? now;
  const fraction = playback.duration > 0 ? shown / playback.duration : 0;
  const widest = time(playback.duration).replace(/\d/g, "0");

  return (
    <div className="m-timeline">
      <span className="m-time" data-widest={widest}>
        {time(shown)}
      </span>
      {playback.canSeek ? (
        <input
          type="range"
          className="m-slider"
          min={0}
          max={playback.duration}
          step={0.5}
          value={shown}
          aria-label="Position"
          style={{ "--fill": `${fraction * 100}%`, "--accent": accent } as React.CSSProperties}
          onChange={(e) => setDrag(Number(e.target.value))}
          onPointerUp={() => {
            if (drag === null) return;
            hold.current = { value: drag, until: Date.now() + 1000 };
            void useMusic.getState().seek(drag);
            setDrag(null);
          }}
        />
      ) : (
        <span className="m-meter">
          <span style={{ width: `${fraction * 100}%`, background: accent }} />
        </span>
      )}
      <span className="m-time" data-widest={`−${widest}`}>
        −{time(playback.duration - shown)}
      </span>
    </div>
  );
}

/** Keeps the island's music page height in step with what the page shows. */
export function useMusicHeight() {
  const playback = useMusic((s) => s.playback);
  const expanded = useLyrics((s) => s.expanded);
  const setPageContent = useIsland((s) => s.setPageContent);
  useEffect(
    () => setPageContent("music", playerHeight(playback, expanded)),
    [playback?.duration, !!playback, expanded, setPageContent],
  );
}
