// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The companion over an activity's closed strip, which steps aside while it
// visits or comes out to react (NotchMascotActivityVisit).

import { useLocale } from "../i18n";

import type { ReactNode } from "react";
import type { Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { useIsland } from "../island/store";
import { MascotTrackView } from "./MascotView";
import { activityTrack, endsOutOfSight, takesOnlyItsWing, watchesTimer } from "./motion";
import { mascotLook, restingMood, useMascot } from "./service";

export function MascotActivityVisit(props: { geometry: Geometry; strip: Size; children: ReactNode }) {
  useLocale();
  const { geometry, strip } = props;
  const mascot = useMascot();
  const settings = useIsland((s) => s.settings);
  // Switched off, a visit under way still plays out over the strip.
  const track = mascot.on || mascot.visit ? activityTrack(geometry, strip.width, mascot.side) : null;
  // A lap or a homecoming ends where it rests, which an activity's strip has no
  // place for, so only what ends out of sight comes over it. A countdown is
  // watched from beside a camera; a capsule has none.
  const visit =
    track && mascot.visit && endsOutOfSight(mascot.visit.kind) && !(watchesTimer(mascot.visit.kind) && !track.hidden)
      ? mascot.visit
      : null;
  const hidden = track?.hidden ?? null;
  // Reacting beside a camera, it covers only the wing it stands in. A capsule has
  // no wings, so what it shows steps aside instead.
  const ownWing = visit !== null && takesOnlyItsWing(visit.kind) && hidden !== null;
  const stepsAside = visit !== null && mascot.stepsAside && !ownWing;
  const rightWing = track?.mirrored === true && visit !== null && !watchesTimer(visit.kind);
  return (
    <div className="mr-activity" style={{ width: strip.width, height: strip.height }}>
      <div className="mr-activity-content" style={{ opacity: stepsAside ? 0 : 1 }}>
        {props.children}
      </div>
      {ownWing && mascot.stepsAside && track && hidden ? (
        <span
          className="mr-wing-cover"
          style={{
            left: rightWing ? hidden[1] : 0,
            width: rightWing ? track.width - hidden[1] : hidden[0],
            height: track.height,
          }}
        />
      ) : null}
      {track && visit ? (
        <span className="mr-track">
          <MascotTrackView
            look={mascotLook(settings)}
            track={track}
            rests={false}
            visit={visit}
            mood={restingMood(mascot)}
          />
        </span>
      ) : null}
    </div>
  );
}
