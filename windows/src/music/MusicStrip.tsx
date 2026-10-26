// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island while music plays (NotchMusicStrip.swift; spec 04
// §1.3): the cover in the left wing, seven bars in the right, which turn
// into play/pause under the pointer. Anywhere else opens the Music page.

import { Pause, Play } from "lucide-react";
import { useState } from "react";
import { COMPACT_EDGE_GAP, edgeInset, type Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { useIsland } from "../island/store";
import { EqualizerBars } from "./EqualizerBars";
import { Cover } from "./MusicPage";
import { isPlaying, useMusic } from "./store";

const BARS = 7;
const BAR_WIDTH = 1.8;
export const BARS_WIDTH = BAR_WIDTH * (BARS + (BARS - 1) * 0.85);

/** The cover takes the strip's height less an even gap above and below (max 26). */
export const artworkSide = (stripHeight: number) => Math.max(0, Math.min(26, stripHeight - COMPACT_EDGE_GAP * 2));
export const barHeight = (stripHeight: number) => Math.min(16, Math.max(6, stripHeight - COMPACT_EDGE_GAP * 2));

/** Music wants 44 pt wings and uses up to 56 (NotchGeometry.compactMusicGeometry). */
export const MUSIC_WING = 56;

export function MusicStrip({ geometry, size }: { geometry: Geometry; size: Size }) {
  const playback = useMusic((s) => s.playback);
  const playing = useMusic(isPlaying);
  const tint = useMusic((s) => s.tint) ?? "#fff";
  const [overBars, setOverBars] = useState(false);
  const capsule = geometry.floatingGap !== null;
  const side = artworkSide(capsule ? geometry.stripHeight - 2 * (geometry.floatingGap ?? 0) + 4 : geometry.stripHeight);
  const bars = barHeight(geometry.stripHeight);
  const coverInset = capsule ? 3 : edgeInset(size, side, side * 0.28, COMPACT_EDGE_GAP, geometry.input);
  const barsInset = capsule ? 8 : edgeInset(size, bars, BAR_WIDTH / 2, COMPACT_EDGE_GAP, geometry.input);
  const wing = capsule ? undefined : (size.width - geometry.cameraWidth) / 2;

  return (
    <div
      role="button"
      tabIndex={0}
      className="m-strip"
      style={{ width: size.width, height: size.height }}
      aria-label={playback ? `${playback.title}, ${playback.artist}` : "Music"}
      onClick={() => useIsland.getState().activateActivity("music")}
    >
      <span className="m-strip-wing" style={{ width: wing, paddingLeft: coverInset }}>
        <Cover src={playback?.artwork} size={side} radius={side * 0.28} />
      </span>
      {capsule ? <span style={{ flex: 1 }} /> : <span style={{ width: geometry.cameraWidth, flex: "none" }} />}
      <span
        className="m-strip-wing is-end"
        style={{ width: wing, paddingRight: barsInset }}
        onMouseEnter={() => setOverBars(true)}
        onMouseLeave={() => setOverBars(false)}
        onClick={(e) => {
          // The bars' own button plays and pauses in place.
          e.stopPropagation();
          void useMusic.getState().command("toggle");
        }}
      >
        <span className="m-strip-swap" style={{ opacity: overBars ? 0 : 1 }}>
          <EqualizerBars count={BARS} width={BAR_WIDTH} height={bars} color={tint} moving={playing} />
        </span>
        <span className="m-strip-swap is-over" style={{ opacity: overBars ? 1 : 0, color: tint }}>
          {playing ? (
            <Pause size={Math.min(13, Math.max(8, bars - 3))} fill="currentColor" strokeWidth={0} />
          ) : (
            <Play size={Math.min(13, Math.max(8, bars - 3))} fill="currentColor" strokeWidth={0} />
          )}
        </span>
      </span>
    </div>
  );
}
