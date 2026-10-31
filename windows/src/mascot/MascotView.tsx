// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Holds the companion in a view (NotchMascotHostView): it draws only through
// its layers and never takes a click, since what it stands on decides what a
// click does. `MascotFace` is the companion on its own, centred in its frame;
// `MascotTrackView` is the companion in a closed strip, resting or strolling
// through, and hidden behind the camera as it passes.

import { useLocale } from "../i18n";

import { type CSSProperties, useEffect, useLayoutEffect, useRef } from "react";
import "./mascot.css";
import {
  type MascotLook,
  type MascotMood,
  type MascotReaction,
  type MascotReactionEvent,
  pointerGaze,
  ReactionGate,
} from "./model";
import {
  CAMEO_ARRIVAL,
  endsOutOfSight,
  hopHeight,
  leftSide,
  type MascotTrack,
  type MascotVisit,
  visitPath,
  visitReaction,
  watchesTimer,
} from "./motion";
import { type MascotCue, MascotRig, mediaTime } from "./rig";
import { figureOffset } from "./shapes";
import { useMascotLowPower } from "./power";

const reducedMotion = () => globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

/** A rig mounted in `host` for as long as the view lives. */
function useRig() {
  const host = useRef<HTMLSpanElement>(null);
  const rig = useRef<MascotRig | null>(null);
  rig.current ??= new MascotRig();
  useLayoutEffect(() => {
    const current = rig.current!;
    host.current?.append(current.root);
    return () => {
      current.dispose();
      current.root.remove();
    };
  }, []);
  return { host, rig: rig.current };
}

/** The companion on its own: a notice's mark, the drop hint and the choices in Settings. */
export function MascotFace(props: {
  look: MascotLook;
  size: number;
  mood?: MascotMood;
  idles?: boolean;
  /** A reaction it plays once, as it first shows. */
  reaction?: MascotReaction;
  /** A one-off motion, played whenever `cueId` changes. */
  cue?: MascotCue;
  cueId?: number;
  style?: CSSProperties;
}) {
  useLocale();
  const { look, size, mood = "idle", idles = false, reaction, cue, cueId = 0 } = props;
  const { host, rig } = useRig();
  const lowPower = useMascotLowPower();
  const first = useRef(true);
  const lastCue = useRef(0);

  useLayoutEffect(() => {
    rig.reduceMotion = reducedMotion();
    rig.lowPower = lowPower;
    rig.configure(look, size);
    rig.root.style.transform = `translate(0px, ${-figureOffset(look) * size}px)`;
    rig.setMood(mood, !first.current);
    rig.setIdles(idles);
  });

  useEffect(() => {
    if (!first.current) return;
    first.current = false;
    // Small in a notice, it hops a little higher for its size to read.
    if (reaction) rig.react(reaction, size * 0.32);
  }, [reaction, rig, size]);

  useEffect(() => {
    if (cue && cueId !== lastCue.current) rig.cue(cue);
    lastCue.current = cueId;
  }, [cue, cueId, rig]);

  return <span ref={host} className="mr-face" style={{ width: size, height: size, ...props.style }} aria-hidden />;
}

export function MascotTrackView(props: {
  look: MascotLook;
  track: MascotTrack;
  /** Whether it stands at its resting place, or only comes for the visit. */
  rests: boolean;
  visit: MascotVisit | null;
  /** The face it keeps at rest. */
  mood?: MascotMood;
  reaction?: MascotReactionEvent | null;
  /** Switched off, the Settings preview shows it asleep: eyes shut, no blinking, no eyes for the pointer. */
  awake?: boolean;
}) {
  useLocale();
  const { look, track, rests, visit, mood = "idle", reaction = null, awake = true } = props;
  const { host, rig } = useRig();
  const lowPower = useMascotLowPower();
  const state = useRef({
    requestedMood: null as MascotMood | null,
    playedVisit: null as number | null,
    playedReaction: null as number | null,
    cameo: undefined as ReturnType<typeof setTimeout> | undefined,
    mounted: false,
  });

  const shift = figureOffset(look) * track.size;
  const offstage = track.mirrored ? -track.size * 2 : track.width + track.size * 2;
  const x = rests ? track.rest : offstage;
  const y = track.baseline - shift;

  useLayoutEffect(() => {
    const own = state.current;
    rig.reduceMotion = reducedMotion();
    rig.lowPower = lowPower;
    rig.configure(look, track.size);
    const wanted = awake ? mood : "sleepy";
    if (wanted !== own.requestedMood) {
      rig.setMood(wanted, own.requestedMood !== null);
      own.requestedMood = wanted;
    }
    rig.setIdles(rests && awake);
    rig.root.style.transform = `translate(${x - track.size / 2}px, ${y - track.size / 2}px)`;

    // A countdown is watched from the camera's left whatever the side.
    const stand = (visit && watchesTimer(visit.kind) ? leftSide(track) : track).rest;
    if (!visit) {
      // Ended early while it stands in its place, as when what it reacted
      // over went away: it stays there rather than leave.
      if (rig.isVisiting && rig.standsAt(stand)) rig.endVisit();
      else if (!rig.isVisiting) rig.clearFinishedVisit();
    } else if (visit.id !== own.playedVisit) {
      own.playedVisit = visit.id;
      clearTimeout(own.cameo);
      rig.wake(false);
      rig.playVisit(
        visitPath(visit.kind, track),
        visit.greeting,
        y,
        { x: stand, y },
        visit.start,
        endsOutOfSight(visit.kind),
        { x, y },
      );
      const coming = visitReaction(visit.kind);
      if (coming) {
        // A strip drawn again after it landed picks the visit up there, and
        // does not play the reaction late. Lingering, it is already there.
        const wait = visit.start + (visit.kind.type === "linger" ? 0 : CAMEO_ARRIVAL) - mediaTime();
        if (wait > -0.2) {
          own.cameo = setTimeout(() => rig.react(coming, hopHeight(track, 0.22)), Math.max(0, wait) * 1000);
        }
      }
    }
    if (rests && reaction && reaction.id !== own.playedReaction) {
      own.playedReaction = reaction.id;
      // Only while it's fresh: a strip drawn after its moment keeps still.
      if (mediaTime() - reaction.start < 1.5 && !rig.isVisiting) rig.react(reaction.reaction, hopHeight(track, 0.22));
    }
  });

  useEffect(() => {
    const own = state.current;
    return () => clearTimeout(own.cameo);
  }, []);

  // A resting companion's eyes follow the pointer over what it stands on, the
  // pointer wakes it from a long rest, and a second on it is a pat on the head.
  // Moves come only while the pointer moves: nothing runs while it rests.
  const follows = rests && awake;
  useEffect(() => {
    // Not its own parents, which let the pointer through to what it stands on.
    const target = host.current?.closest<HTMLElement>(".n-activation, .st-mascot-stage");
    if (!follows || !target) return;
    let petting: ReturnType<typeof setTimeout> | undefined;
    let lastPetting = -Infinity;
    const at = (event: PointerEvent) => {
      const box = host.current!.getBoundingClientRect();
      return { x: event.clientX - box.left, y: event.clientY - box.top };
    };
    const center = () => ({ x: rig.size / 2 + currentX(rig), y: currentY(rig) + rig.size / 2 });
    const follow = (event: PointerEvent) => {
      if (rig.isVisiting) return;
      rig.attend(pointerGaze(center(), at(event), rig.size));
    };
    const enter = (event: PointerEvent) => {
      if (rig.isDozing && !rig.isVisiting) rig.react("wakeUp", 0);
      else rig.wake(true);
      follow(event);
    };
    const move = (event: PointerEvent) => {
      follow(event);
      const middle = center();
      const point = at(event);
      const reach = rig.size / 2 + 3;
      if (Math.abs(point.x - middle.x) > reach || Math.abs(point.y - middle.y) > reach || rig.isVisiting) {
        clearTimeout(petting);
        petting = undefined;
        return;
      }
      if (petting !== undefined || mediaTime() - lastPetting <= ReactionGate.REPEAT_INTERVAL) return;
      petting = setTimeout(() => {
        petting = undefined;
        lastPetting = mediaTime();
        rig.react("love", 0);
      }, 1000);
    };
    const leave = () => {
      clearTimeout(petting);
      petting = undefined;
      rig.lookAhead(0.25);
    };
    target.addEventListener("pointerenter", enter);
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerleave", leave);
    return () => {
      clearTimeout(petting);
      target.removeEventListener("pointerenter", enter);
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerleave", leave);
      rig.lookAhead();
    };
  }, [follows, host, rig]);

  const hidden = track.hidden;
  // Where it can be seen: either side of the camera, and well above and below for its hops.
  const clip = hidden
    ? `path("M${-track.size * 3} ${-track.height}H${hidden[0]}V${track.height * 2}H${-track.size * 3}Z` +
      `M${hidden[1]} ${-track.height}H${track.width + track.size * 3}V${track.height * 2}H${hidden[1]}Z")`
    : undefined;

  return (
    <span
      ref={host}
      className="mr-stage"
      style={{ width: track.width, height: track.height, clipPath: clip }}
      aria-hidden
    />
  );
}

const currentX = (rig: MascotRig) => new DOMMatrixReadOnly(getComputedStyle(rig.root).transform).m41;
const currentY = (rig: MascotRig) => new DOMMatrixReadOnly(getComputedStyle(rig.root).transform).m42;
