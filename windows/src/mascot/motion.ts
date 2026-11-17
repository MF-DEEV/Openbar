// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Where the companion can walk in a closed strip, and its strolls, sampled
// densely enough to play as given (NotchMascotTrack, NotchMascotMotion).

import { type Geometry, restingWingWidth } from "../island/geometry";
import { type MascotMood, type MascotReaction, type MascotSide, REACTION_LENGTH, WING_ROOM } from "./model";

export type VisitKind =
  /** From its resting place, around the island and back. */
  | { type: "lap" }
  /** In at one end and out at the other, over what the island shows. */
  | { type: "pass" }
  /** Out from behind the camera over what the island shows, to play a reaction and go back. */
  | { type: "cameo"; reaction: MascotReaction }
  /** Still where it rested as an activity took its place: it plays a reaction there and goes. */
  | { type: "linger"; reaction: MascotReaction }
  /** The last seconds of a countdown, this long: out over the timer's mark to watch it run out. */
  | { type: "countdown"; total: number }
  /** A countdown it watched was paused: back behind the camera. */
  | { type: "retreat" }
  /** Switched off: a glad hop where it rests and away, before the wings fold. */
  | { type: "farewell" }
  /** Moved to the camera's other side: behind the camera, across, and out to its new place. */
  | { type: "cross" }
  /** Switched on: out from behind the camera as the wings open, or in at a capsule's near end. */
  | { type: "arrive" };

export interface MascotVisit {
  id: number;
  kind: VisitKind;
  greeting: MascotMood;
  /** When it began, in seconds on the page's clock, so a strip drawn again picks it up where it is. */
  start: number;
}

/** It ends out of sight, so it can cross an activity's strip and leave nothing behind. */
export function endsOutOfSight(kind: VisitKind): boolean {
  return !["lap", "cross", "arrive"].includes(kind.type);
}

/** It stands over the timer's mark only, and the reading stays in view. */
export const watchesTimer = (kind: VisitKind) => kind.type === "countdown" || kind.type === "retreat";

/** Over an activity beside a camera it takes only the wing it stands in; only a stroll takes the whole strip. */
export const takesOnlyItsWing = (kind: VisitKind) =>
  kind.type === "countdown" || kind.type === "retreat" || kind.type === "cameo" || kind.type === "linger";

/** The reaction it comes out for, played as it lands or at once where it stands. */
export const visitReaction = (kind: VisitKind): MascotReaction | null =>
  kind.type === "cameo" || kind.type === "linger" ? kind.reaction : null;

export interface MascotTrack {
  width: number;
  height: number;
  /** The companion's square. */
  size: number;
  /** Its centre while resting. */
  rest: number;
  /** The camera, as [from, to]: walking there, it is behind it. */
  hidden: readonly [number, number] | null;
  /** The height of its centre. */
  baseline: number;
  /** The top of the island over it: the screen's edge, or a capsule's. */
  ceiling: number;
  /** It rests right of the camera, and every stroll runs the other way. */
  mirrored: boolean;
  /** How many times larger than the island it is drawn, for the Settings preview. */
  scale: number;
}

/** How high a hop of `share` of its size lifts it, kept clear of the island's top. */
export function hopHeight(track: MascotTrack, share: number): number {
  return Math.min(
    track.size * share,
    Math.max(
      track.scale,
      track.baseline - track.size / 2 - track.ceiling - (track.hidden === null ? 1 : 2) * track.scale,
    ),
  );
}

/** Where it stands just out of sight behind the camera, `margin` past the camera's edge on its side. */
const behindCamera = (track: MascotTrack, hidden: readonly [number, number], margin: number) =>
  hidden[0] + track.size / 2 + margin * track.scale;

/** The same track `scale` times larger, for the Settings preview. */
export function scaledTrack(track: MascotTrack, scale: number): MascotTrack {
  return {
    width: track.width * scale,
    height: track.height * scale,
    size: track.size * scale,
    rest: track.rest * scale,
    hidden: track.hidden ? [track.hidden[0] * scale, track.hidden[1] * scale] : null,
    baseline: track.baseline * scale,
    ceiling: track.ceiling * scale,
    mirrored: track.mirrored,
    scale: track.scale * scale,
  };
}

/** The same track with its resting place on the camera's left. */
export function leftSide(track: MascotTrack): MascotTrack {
  return track.mirrored ? { ...track, mirrored: false, rest: track.width - track.rest } : track;
}

/** Where it says hello across the strip: past the camera, or toward a capsule's far end. */
function farSpot(track: MascotTrack): number {
  if (!track.hidden) return Math.min(track.width - track.size * 0.8, track.rest + track.width * 0.22);
  return track.hidden[1] + (track.hidden[0] - track.rest);
}

/** Its square in a closed strip of `height`, with or without a capsule's margins around its body. */
export const mascotSize = (stripHeight: number, floats: boolean) =>
  floats ? Math.max(10, Math.min(18, stripHeight - 6)) : Math.max(12, Math.min(20, stripHeight - 12));

/** Inside a wing, a little way from the camera; a capsule has no camera, so it rests in the middle. */
export function makeTrack(input: {
  stripWidth: number;
  stripHeight: number;
  wing: number;
  cameraWidth: number;
  floats: boolean;
  bodyHeight: number;
  side?: MascotSide;
}): MascotTrack {
  const { stripWidth, stripHeight, wing, cameraWidth, floats, bodyHeight, side = "left" } = input;
  if (floats) {
    return {
      width: stripWidth,
      height: stripHeight,
      size: mascotSize(bodyHeight, true),
      rest: stripWidth / 2,
      hidden: null,
      baseline: stripHeight / 2,
      ceiling: Math.max(0, (stripHeight - bodyHeight) / 2),
      mirrored: false,
      scale: 1,
    };
  }
  const size = mascotSize(stripHeight, false);
  const gap = Math.max(4, Math.min(8, wing - size - 6));
  const rest = Math.max(size / 2, wing - gap - size / 2);
  return {
    width: stripWidth,
    height: stripHeight,
    size,
    rest: side === "right" ? stripWidth - rest : rest,
    hidden: [wing, wing + cameraWidth],
    baseline: stripHeight / 2 + 0.5,
    ceiling: 0,
    mirrored: side === "right",
    scale: 1,
  };
}

const stripBody = (geometry: Geometry) =>
  geometry.floatingGap !== null ? geometry.stripHeight - 2 * geometry.floatingGap : geometry.stripHeight;

/** Where it walks in the closed island at rest, `width` being a capsule's. */
export function restingTrack(geometry: Geometry, width: number, side: MascotSide): MascotTrack {
  const floats = geometry.floatingGap !== null;
  const wing = floats ? 0 : restingWingWidth(geometry);
  return makeTrack({
    stripWidth: floats ? width : geometry.cameraWidth + 2 * wing,
    stripHeight: geometry.stripHeight,
    wing,
    cameraWidth: geometry.cameraWidth,
    floats,
    bodyHeight: stripBody(geometry),
    side,
  });
}

/** Where it comes out over an activity's strip of `width`, or null when the wing on its side has no room. */
export function activityTrack(geometry: Geometry, width: number, side: MascotSide): MascotTrack | null {
  const floats = geometry.floatingGap !== null;
  const wing = floats ? 0 : (width - geometry.cameraWidth) / 2;
  if (!floats && wing < mascotSize(geometry.stripHeight, false) + WING_ROOM) return null;
  return makeTrack({
    stripWidth: width,
    stripHeight: geometry.stripHeight,
    wing,
    cameraWidth: floats ? 0 : geometry.cameraWidth,
    floats,
    bodyHeight: stripBody(geometry),
    side,
  });
}

export interface MascotPath {
  keyTimes: number[];
  x: number[];
  lift: number[];
  /** Positive squashes it flat, negative stretches it tall. */
  squash: number[];
  /** Where its eyes look across, in shares of its size. */
  gaze: number[];
  duration: number;
  /** When its greeting face shows, in seconds from the start. */
  greeting: readonly [number, number];
}

const emptyPath = (): MascotPath => ({
  keyTimes: [],
  x: [],
  lift: [],
  squash: [],
  gaze: [],
  duration: 0,
  greeting: [0, 0],
});

export const LAP_DURATION = 3.6;
export const PASS_DURATION = 3.4;
export const HOME_DURATION = 0.62;
/** A cameo lands where it would rest this long after it starts. */
export const CAMEO_ARRIVAL = 0.5;
/** It is back behind the camera as its exit ends. */
export const CAMEO_EXIT = 0.32;
export const FAREWELL_DURATION = 0.78;
export const CROSS_DURATION = 1.24;
/** How long before a countdown runs out it comes to watch it. */
export const COUNTDOWN_LEAD = 5;
/** It stays past the end for a beat, under the notice that takes over. */
const COUNTDOWN_LINGER = 0.1;

/** How long a cameo stays where it landed: its reaction and a beat after it. */
export const cameoHold = (reaction: MascotReaction) => REACTION_LENGTH[reaction] + 0.3;

export function visitDuration(kind: VisitKind): number {
  switch (kind.type) {
    case "lap":
      return LAP_DURATION;
    case "pass":
      return PASS_DURATION;
    case "arrive":
      return HOME_DURATION;
    case "cameo":
      return CAMEO_ARRIVAL + cameoHold(kind.reaction) + CAMEO_EXIT;
    case "linger":
      return cameoHold(kind.reaction) + CAMEO_EXIT;
    case "countdown":
      return kind.total + COUNTDOWN_LINGER + CAMEO_EXIT;
    case "retreat":
      return CAMEO_EXIT;
    case "farewell":
      return FAREWELL_DURATION;
    case "cross":
      return CROSS_DURATION;
  }
}

/**
 * When a reaction's visit gives back what it covered, from its start. Beside a
 * camera it is behind the camera before it is halfway home; in a capsule it
 * walks out past the end, so what it covered starts back a quarter of the way.
 */
export function handBack(kind: VisitKind, floats: boolean): number | null {
  const reaction = visitReaction(kind);
  if (!reaction) return null;
  const arrival = kind.type === "linger" ? 0 : CAMEO_ARRIVAL;
  return arrival + cameoHold(reaction) + (floats ? CAMEO_EXIT / 4 : 0);
}

type Step =
  | { type: "stay"; length: number; bounces?: number }
  | { type: "hop"; to: number; length: number; height: number }
  | { type: "walk"; to: number; length: number }
  | { type: "jump"; to: number };

const stay = (length: number, bounces = 0): Step => ({ type: "stay", length, bounces });
const hop = (to: number, length: number, height: number): Step => ({ type: "hop", to, length, height });
const walk = (to: number, length: number): Step => ({ type: "walk", to, length });

function sample(steps: Step[], start: number, track: MascotTrack, duration: number): MascotPath {
  const rate = 60;
  const path = emptyPath();
  let time = 0;
  let position = start;
  // A landing's squash eases out over the start of the next step.
  let landing = { squash: 0, time: 0 };
  const add = (x: number, lift: number, squash: number, gaze: number) => {
    const recovery = landing.squash * Math.max(0, 1 - (time - landing.time) / 0.14);
    path.keyTimes.push(Math.min(1, time / duration));
    path.x.push(x);
    path.lift.push(lift);
    path.squash.push(squash !== 0 ? squash : recovery);
    path.gaze.push(gaze);
  };
  add(position, 0, 0, 0);
  for (const step of steps) {
    switch (step.type) {
      case "jump":
        // Off stage at both ends, and in no time: two frames at the same
        // moment, so no frame ever draws it on the way across.
        position = step.to;
        add(position, 0, 0, 0.05);
        break;
      case "stay": {
        const frames = Math.max(1, Math.round(step.length * rate));
        const bounces = step.bounces ?? 0;
        for (let frame = 1; frame <= frames; frame++) {
          const share = frame / frames;
          time += step.length / frames;
          const wave = bounces > 0 ? Math.abs(Math.sin(share * Math.PI * bounces)) : 0;
          add(position, wave * hopHeight(track, 0.16), 0, 0);
        }
        break;
      }
      case "hop": {
        const from = position;
        const frames = Math.max(2, Math.round(step.length * rate));
        const direction = step.to >= from ? 1 : -1;
        for (let frame = 1; frame <= frames; frame++) {
          const share = frame / frames;
          time += step.length / frames;
          // Eased along the ground, a parabola above it.
          const eased = share * share * (3 - 2 * share);
          const squash = share < 0.2 ? -0.12 * (1 - share / 0.2) : share > 0.88 ? (0.14 * (share - 0.88)) / 0.12 : 0;
          add(from + (step.to - from) * eased, step.height * 4 * share * (1 - share), squash, 0.05 * direction);
        }
        position = step.to;
        landing = { squash: 0.14, time };
        break;
      }
      case "walk": {
        const from = position;
        const frames = Math.max(2, Math.round(step.length * rate));
        const direction = step.to >= from ? 1 : -1;
        for (let frame = 1; frame <= frames; frame++) {
          const share = frame / frames;
          time += step.length / frames;
          // Little steps: a small bob on each.
          add(
            from + (step.to - from) * share,
            Math.abs(Math.sin(share * Math.PI * 3)) * hopHeight(track, 0.06),
            0,
            0.05 * direction,
          );
        }
        position = step.to;
        break;
      }
    }
  }
  // The last step lands, and it stays for whatever time is left.
  if (time < duration) {
    time = duration;
    add(position, 0, 0, 0);
  } else if (path.keyTimes.length > 0) {
    path.keyTimes[path.keyTimes.length - 1] = 1;
  }
  path.duration = duration;
  return path;
}

function lap(track: MascotTrack): MascotPath {
  const size = track.size;
  const steps: Step[] = [stay(0.18)];
  if (track.hidden) {
    steps.push(
      hop(behindCamera(track, track.hidden, 2), 0.34, hopHeight(track, 0.24)),
      walk(track.hidden[1] - size / 2 - 2 * track.scale, 0.5),
      hop(farSpot(track), 0.34, hopHeight(track, 0.24)),
    );
  } else {
    steps.push(hop(farSpot(track), 0.6, hopHeight(track, 0.3)), stay(0.58));
  }
  steps.push(
    stay(0.94, 2),
    walk(track.width + size, 0.4),
    { type: "jump", to: -size },
    hop(track.rest, 0.52, hopHeight(track, 0.26)),
  );
  const path = sample(steps, track.rest, track, LAP_DURATION);
  const greet = track.hidden ? 1.3 : 0.78;
  path.greeting = [greet, greet + 1.0];
  return path;
}

function pass(track: MascotTrack): MascotPath {
  const size = track.size;
  const steps: Step[] = [stay(0.2), hop(track.rest, 0.42, hopHeight(track, 0.26)), stay(0.88, 2)];
  if (track.hidden) {
    steps.push(
      hop(behindCamera(track, track.hidden, 2), 0.34, hopHeight(track, 0.24)),
      walk(track.hidden[1] - size / 2 - 2 * track.scale, 0.5),
      hop(farSpot(track), 0.34, hopHeight(track, 0.24)),
      stay(0.22),
    );
  } else {
    steps.push(hop(farSpot(track), 0.6, hopHeight(track, 0.3)), stay(0.8));
  }
  steps.push(walk(track.width + size, 0.4));
  const path = sample(steps, -size, track, PASS_DURATION);
  path.greeting = [0.55, 1.55];
  return path;
}

/** Out from behind the camera with a little hop to its place. */
function home(track: MascotTrack): MascotPath {
  if (!track.hidden) {
    const path = sample([stay(0.08), hop(track.rest, 0.36, hopHeight(track, 0.2))], track.rest, track, HOME_DURATION);
    path.greeting = [0, 0.4];
    return path;
  }
  const path = sample(
    [stay(0.06), hop(track.rest, 0.42, hopHeight(track, 0.2))],
    behindCamera(track, track.hidden, 2),
    track,
    HOME_DURATION,
  );
  path.greeting = [0, 0.44];
  return path;
}

function cameo(track: MascotTrack, hold: number): MascotPath {
  const duration = CAMEO_ARRIVAL + hold + CAMEO_EXIT;
  if (!track.hidden) {
    return sample(
      [
        stay(0.06),
        hop(track.rest, CAMEO_ARRIVAL - 0.06, hopHeight(track, 0.3)),
        stay(hold),
        walk(track.width + track.size, CAMEO_EXIT),
      ],
      -track.size,
      track,
      duration,
    );
  }
  // Just out of sight, so it shows the moment it moves and is gone as it lands.
  const behind = behindCamera(track, track.hidden, 1);
  return sample(
    [
      stay(0.06),
      hop(track.rest, CAMEO_ARRIVAL - 0.06, hopHeight(track, 0.2)),
      stay(hold),
      hop(behind, CAMEO_EXIT, hopHeight(track, 0.16)),
    ],
    behind,
    track,
    duration,
  );
}

function linger(track: MascotTrack, hold: number): MascotPath {
  const duration = hold + CAMEO_EXIT;
  if (!track.hidden) {
    return sample([stay(hold), walk(track.width + track.size, CAMEO_EXIT)], track.rest, track, duration);
  }
  return sample(
    [stay(hold), hop(behindCamera(track, track.hidden, 1), CAMEO_EXIT, hopHeight(track, 0.16))],
    track.rest,
    track,
    duration,
  );
}

/** A small hop with every second that goes, eyes on the reading, and back just after it runs out. */
function countdown(track: MascotTrack, total: number): MascotPath {
  const duration = total + COUNTDOWN_LINGER + CAMEO_EXIT;
  if (!track.hidden) return sample([stay(duration)], -track.size, track, duration);
  const behind = behindCamera(track, track.hidden, 1);
  const steps: Step[] = [stay(0.06), hop(track.rest, CAMEO_ARRIVAL - 0.06, hopHeight(track, 0.2))];
  let time = CAMEO_ARRIVAL;
  // The reading changes on every whole second left; it hops just as it does.
  for (let left = Math.ceil(total) - 1; left >= 1; left--) {
    const tick = total - left;
    if (tick - 0.15 <= time + 0.1) continue;
    steps.push(stay(tick - 0.15 - time), hop(track.rest, 0.3, hopHeight(track, 0.1)));
    time = tick + 0.15;
  }
  steps.push(stay(Math.max(0, total + COUNTDOWN_LINGER - time)), hop(behind, CAMEO_EXIT, hopHeight(track, 0.16)));
  const path = sample(steps, behind, track, duration);
  path.gaze = path.gaze.map((gaze, index) => {
    const at = path.keyTimes[index]! * duration;
    return at >= CAMEO_ARRIVAL && at <= total + COUNTDOWN_LINGER ? 0.07 : gaze;
  });
  return path;
}

function arrive(track: MascotTrack): MascotPath {
  if (track.hidden) return home(track);
  const path = sample([stay(0.06), hop(track.rest, 0.46, hopHeight(track, 0.3))], -track.size, track, HOME_DURATION);
  path.greeting = [0, 0.52];
  return path;
}

function cross(track: MascotTrack): MascotPath {
  if (!track.hidden) return home(track);
  const path = sample(
    [
      stay(0.06),
      hop(track.hidden[1] - track.size / 2 - 2 * track.scale, 0.34, hopHeight(track, 0.24)),
      walk(behindCamera(track, track.hidden, 2), 0.38),
      hop(track.rest, 0.36, hopHeight(track, 0.24)),
      stay(0.1),
    ],
    farSpot(track),
    track,
    CROSS_DURATION,
  );
  path.greeting = [0.72, CROSS_DURATION];
  return path;
}

function farewell(track: MascotTrack): MascotPath {
  if (!track.hidden) {
    const path = sample(
      [stay(0.06), hop(track.rest, 0.3, hopHeight(track, 0.22)), stay(0.02), walk(track.width + track.size, 0.4)],
      track.rest,
      track,
      FAREWELL_DURATION,
    );
    path.greeting = [0, 0.44];
    return path;
  }
  const path = sample(
    [
      stay(0.06),
      hop(track.rest, 0.3, hopHeight(track, 0.22)),
      stay(0.08),
      hop(behindCamera(track, track.hidden, 1), CAMEO_EXIT, hopHeight(track, 0.16)),
    ],
    track.rest,
    track,
    FAREWELL_DURATION,
  );
  path.greeting = [0, 0.48];
  return path;
}

function retreat(track: MascotTrack): MascotPath {
  if (!track.hidden) return sample([stay(CAMEO_EXIT)], -track.size, track, CAMEO_EXIT);
  return sample(
    [hop(behindCamera(track, track.hidden, 1), CAMEO_EXIT, hopHeight(track, 0.16))],
    track.rest,
    track,
    CAMEO_EXIT,
  );
}

/** A stroll of `kind`. Resting right of the camera, it walks the left side's stroll seen in a mirror. */
export function visitPath(kind: VisitKind, track: MascotTrack): MascotPath {
  // A countdown is watched from the camera's left, over the timer's mark,
  // whichever side it rests on: the reading is on the right.
  if (kind.type === "countdown") return countdown(leftSide(track), kind.total);
  if (kind.type === "retreat") return retreat(leftSide(track));
  if (track.mirrored) {
    const path = visitPath(kind, leftSide(track));
    return { ...path, x: path.x.map((x) => track.width - x), gaze: path.gaze.map((gaze) => -gaze) };
  }
  switch (kind.type) {
    case "lap":
      return lap(track);
    case "pass":
      return pass(track);
    case "cameo":
      return cameo(track, cameoHold(kind.reaction));
    case "linger":
      return linger(track, cameoHold(kind.reaction));
    case "farewell":
      return farewell(track);
    case "cross":
      return cross(track);
    case "arrive":
      return arrive(track);
  }
}
