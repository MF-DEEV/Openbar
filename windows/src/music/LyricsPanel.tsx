// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { AudioLines, FilePlus2, Minus, Plus } from "lucide-react";
import { memo, useEffect, useRef, useState } from "react";
import { activeIndex, MAX_LRC_BYTES, nextLyricDelay, songKey } from "./lyrics";
import { setPrefs } from "../shared/prefs";
import { useLyrics } from "./lyricsStore";
import { useMusic } from "./store";
import { position } from "./types";

/** The panel while it is on screen; lookups follow this, not the toggle alone. */
export function LyricsSection() {
  const key = useLyrics((s) => s.key);
  useEffect(() => {
    useLyrics.getState().setVisible(true);
    return () => useLyrics.getState().setVisible(false);
  }, []);
  return (
    <div id="music-lyrics">
      <LyricsPanel key={key ?? ""} />
    </div>
  );
}

function LyricsPanel() {
  const lyrics = useLyrics((s) => s.lyrics);
  const status = useLyrics((s) => s.status);
  const offset = useLyrics((s) => s.offset);
  const error = useLyrics((s) => s.error);
  const input = useRef<HTMLInputElement>(null);
  const importKey = useRef<string | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const song = useMusic((s) => s.playback);
  return (
    <section className="m-lyrics" aria-label="Song lyrics">
      {lyrics && !lyrics.instrumental ? (
        <LyricText />
      ) : (
        <div className="m-lyrics-empty" role="status">
          {status === "consent"
            ? "Lyrics aren't looked up online. While lyrics are open, the song title, artist, album and duration go to lrclib.net. You can also import an .lrc file."
            : status === "loading"
              ? "Finding lyrics…"
              : status === "failed"
                ? "Could not load lyrics. Check your connection."
                : lyrics?.instrumental
                  ? "Instrumental"
                  : "Lyrics unavailable for this song."}
          {status === "consent" ? (
            <button type="button" onClick={() => void setPrefs({ island: { lyricsOnline: true } })}>
              Find lyrics online
            </button>
          ) : (
            status !== "loading" &&
            !lyrics && (
              <button type="button" onClick={() => void useLyrics.getState().select(song, true)}>
                Retry
              </button>
            )
          )}
        </div>
      )}
      <div className="m-lyrics-tools">
        <button
          type="button"
          aria-label="Import .lrc lyrics"
          title="Import .lrc lyrics"
          onClick={() => {
            importKey.current = useLyrics.getState().key;
            input.current?.click();
          }}
        >
          <FilePlus2 size={15} />
        </button>
        {lyrics?.lines.length ? (
          <>
            <span>Timing</span>
            <span className="m-lyrics-spacer" />
            <button
              type="button"
              aria-label="Lyrics earlier by 0.5 seconds"
              disabled={offset <= -10}
              onClick={() => useLyrics.getState().adjustOffset(-0.5)}
            >
              <Minus size={14} />
            </button>
            <button
              type="button"
              className="m-lyrics-offset"
              title="Reset timing"
              aria-label={`Timing ${offset > 0 ? "+" : ""}${offset.toFixed(2)} seconds. Reset timing`}
              onClick={() => useLyrics.getState().adjustOffset(-offset)}
            >
              {offset >= 0 ? "+" : ""}
              {offset.toFixed(2)} s
            </button>
            <button
              type="button"
              aria-label="Lyrics later by 0.5 seconds"
              disabled={offset >= 10}
              onClick={() => useLyrics.getState().adjustOffset(0.5)}
            >
              <Plus size={14} />
            </button>
          </>
        ) : (
          <span>Import .lrc lyrics</span>
        )}
        <input
          ref={input}
          type="file"
          accept=".lrc"
          hidden
          onChange={async (event) => {
            const file = event.target.files?.[0];
            const expected = importKey.current;
            event.target.value = "";
            if (!file || !expected) return;
            setFileError(null);
            if (!file.name.toLowerCase().endsWith(".lrc") || file.size > MAX_LRC_BYTES) {
              setFileError("Choose an .lrc file up to 128 KB.");
              return;
            }
            try {
              useLyrics.getState().importLrc(await file.text(), expected);
            } catch {
              if (useLyrics.getState().key === expected) setFileError("Could not read this file.");
            }
          }}
        />
      </div>
      {(error || fileError) && (
        <div className="m-lyrics-error" role="alert">
          {fileError ?? error}
        </div>
      )}
    </section>
  );
}

function LyricText() {
  const playback = useMusic((s) => s.playback);
  const lyrics = useLyrics((s) => s.lyrics)!;
  const offset = useLyrics((s) => s.offset);
  const [active, setActive] = useState(-1);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    function update() {
      if (!playback) {
        setActive(-1);
        return;
      }
      setActive(activeIndex(lyrics.lines, position(playback), offset));
      const delay = nextLyricDelay(lyrics.lines, playback, offset);
      if (delay !== null) timer = setTimeout(update, Math.min(delay, 2147483647));
    }
    update();
    return () => clearTimeout(timer);
  }, [playback, lyrics, offset]);
  return (
    <Verses lyrics={lyrics} active={playback?.duration ? active : -1} timed={!!playback && playback.duration > 0} />
  );
}

/** The verse tree only renders at lyric boundaries, even if SMTC emits frequently. */
const Verses = memo(function Verses({
  lyrics,
  active,
  timed,
}: {
  lyrics: NonNullable<ReturnType<typeof useLyrics.getState>["lyrics"]>;
  active: number;
  timed: boolean;
}) {
  const scroll = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const [showSync, setShowSync] = useState(false);
  const canSeek = useMusic((s) => !!s.playback?.canSeek && s.playback.duration > 0);

  function updateSyncButton() {
    const container = scroll.current;
    if (!container || following.current || !timed || !lyrics.lines.length) {
      setShowSync(false);
      return;
    }
    const line = container.querySelector<HTMLElement>(`[data-line="${active}"]`);
    const viewport = container.getBoundingClientRect();
    const verse = line?.getBoundingClientRect();
    setShowSync(
      verse ? verse.bottom < viewport.top + 16 || verse.top > viewport.bottom - 44 : container.scrollTop > 24,
    );
  }

  function scrollToActive(index = active) {
    const container = scroll.current;
    if (!container) return;
    const line = container.querySelector<HTMLElement>(`[data-line="${index}"]`);
    container.scrollTo({
      top: line ? line.offsetTop - container.clientHeight / 2 + line.clientHeight / 2 : 0,
      behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth",
    });
  }

  function browse() {
    const container = scroll.current;
    if (!container || !timed || !lyrics.lines.length) return;
    following.current = false;
    // Cancel a pending smooth follow before wheel, touch or scrollbar input.
    container.scrollTo({ top: container.scrollTop, behavior: "instant" });
    updateSyncButton();
  }

  async function playVerse(index: number) {
    const playback = useMusic.getState().playback;
    if (!playback?.canSeek || playback.duration <= 0 || useLyrics.getState().lyrics !== lyrics) return;
    const target = Math.min(playback.duration, Math.max(0, lyrics.lines[index].time + useLyrics.getState().offset));
    const ok = await useMusic.getState().seek(target);
    const current = useMusic.getState().playback;
    if (
      !ok ||
      !current ||
      current.source !== playback.source ||
      songKey(current) !== songKey(playback) ||
      useLyrics.getState().lyrics !== lyrics
    )
      return;
    following.current = true;
    setShowSync(false);
    scrollToActive(index);
    if (!playback.isPlaying && !current.isPlaying) await useMusic.getState().command("toggle");
  }

  useEffect(() => {
    following.current = true;
    setShowSync(false);
  }, [lyrics]);
  useEffect(() => {
    if (following.current) scrollToActive();
    else updateSyncButton();
  }, [active, lyrics]);
  return (
    <div className="m-lyrics-viewport">
      <div
        className="m-lyrics-scroll"
        ref={scroll}
        tabIndex={0}
        aria-label="Lyrics"
        data-lyrics-scroll
        onWheel={browse}
        onPointerDown={browse}
        onTouchStart={browse}
        onScroll={updateSyncButton}
        onKeyDown={(event) => {
          if (["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End", " "].includes(event.key)) browse();
        }}
      >
        {!timed && <p className="m-lyrics-hint">This player does not provide a playback timeline.</p>}
        {lyrics.lines.length ? (
          <>
            {timed && active < 0 && <p className="m-lyrics-hint">Waiting for lyrics…</p>}
            {lyrics.lines.map((line, index) => (
              <button
                type="button"
                key={line.time}
                disabled={!timed || !canSeek}
                title={timed && canSeek ? "Play from this line" : undefined}
                onClick={() => void playVerse(index).catch(() => useMusic.setState({ failed: true }))}
                data-line={index}
                aria-current={index === active ? "true" : undefined}
                className={`m-lyric${index === active ? " is-active" : ""}`}
              >
                {line.text || "♪"}
              </button>
            ))}
          </>
        ) : (
          <p className="m-lyrics-plain">{lyrics.plain}</p>
        )}
      </div>
      {showSync && (
        <button
          type="button"
          className="m-lyrics-sync"
          onClick={() => {
            following.current = true;
            setShowSync(false);
            scrollToActive();
            scroll.current?.focus({ preventScroll: true });
          }}
        >
          <AudioLines size={16} aria-hidden="true" />
          Sync
        </button>
      )}
    </div>
  );
});
