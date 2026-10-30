// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { makeGeometry } from "../island/geometry";
import { resolve } from "../shared/prefs";
import {
  blinkInterval,
  blinksBeforeSleep,
  EXPRESSIONS,
  faceOf,
  type MascotEye,
  MASCOT_MOMENTS,
  moodBlinks,
  MOMENT_REACTION,
  nextVisitDelay,
  paletteColors,
  pointerGaze,
  powerReaction,
  randomGreeting,
  ReactionGate,
  timerReaction,
} from "./model";
import {
  activityTrack,
  CAMEO_ARRIVAL,
  cameoHold,
  endsOutOfSight,
  handBack,
  hopHeight,
  LAP_DURATION,
  leftSide,
  makeTrack,
  mascotSize,
  PASS_DURATION,
  restingTrack,
  scaledTrack,
  takesOnlyItsWing,
  type VisitKind,
  visitDuration,
  visitPath,
  visitReaction,
  watchesTimer,
} from "./motion";
import {
  bodyPath,
  EYE_CURVES,
  eyePath,
  eyePoints,
  figureOffset,
  resample,
  robotPaths,
  smoothClosedPath,
} from "./shapes";

const EYES: MascotEye[] = ["open", "closed", "happy", "heart", "sleepy", "wide", "squint", "determined"];
const capsule = makeTrack({ stripWidth: 80, stripHeight: 24, wing: 0, cameraWidth: 0, floats: true, bodyHeight: 20 });
const notch = makeTrack({
  stripWidth: 223,
  stripHeight: 24,
  wing: 44,
  cameraWidth: 135,
  floats: false,
  bodyHeight: 24,
});
const KINDS: VisitKind[] = [
  { type: "lap" },
  { type: "pass" },
  { type: "cameo", reaction: "celebrate" },
  { type: "linger", reaction: "yawn" },
  { type: "countdown", total: 5 },
  { type: "retreat" },
  { type: "farewell" },
  { type: "cross" },
  { type: "arrive" },
];

describe("preferences", () => {
  it("starts off, with the Mac's defaults", () => {
    const { island } = resolve(null);
    expect(island.mascotEnabled).toBe(false);
    expect(island.mascotVisits).toBe(true);
    expect(island.mascotReactions).toBe(true);
    expect(island.mascotHidesWhenIdle).toBe(true);
    expect([island.mascotStyle, island.mascotShape, island.mascotPalette]).toEqual(["minimal", "ball", "pearl"]);
    expect(island.mascotSide).toBe("left");
    expect(island.mascotVisitFrequency).toBe("normal");
  });

  it("reads a hand-edited file's unknown choices as the default", () => {
    const stored = {
      mascotStyle: "cat",
      mascotShape: "cube",
      mascotPalette: "ochre",
      mascotSide: "up",
      mascotVisitFrequency: "always",
      mascotEnabled: "yes",
    };
    const { island } = resolve({ island: stored as never });
    expect([island.mascotStyle, island.mascotShape, island.mascotPalette]).toEqual(["minimal", "ball", "pearl"]);
    expect(island.mascotSide).toBe("left");
    expect(island.mascotVisitFrequency).toBe("normal");
    expect(island.mascotEnabled).toBe(false);
  });
});

describe("model", () => {
  it("draws sRGB palette colours", () => {
    expect(paletteColors("pearl").light).toBe("rgb(252 250 245)");
    expect(paletteColors("mint").shade).toBe("rgb(102 212 168)");
  });

  it("blinks only with eyes that can", () => {
    expect(moodBlinks("idle")).toBe(true);
    expect(moodBlinks("alert")).toBe(true);
    expect(moodBlinks("happy")).toBe(false);
    expect(moodBlinks("wink")).toBe(false);
    expect(moodBlinks("sleepy")).toBe(false);
    expect(moodBlinks("love")).toBe(false);
  });

  it("lays the face out per body, the robot's whatever its shape", () => {
    expect(faceOf({ style: "minimal", shape: "egg" }).leftEye.x).toBe(0.375);
    expect(faceOf({ style: "robot", shape: "egg" })).toEqual(faceOf({ style: "robot", shape: "pill" }));
  });

  it("spaces reactions and keeps the same one from coming back soon", () => {
    const gate = new ReactionGate();
    expect(gate.admits("celebrate", 100)).toBe(true);
    expect(gate.admits("love", 101)).toBe(false);
    expect(gate.admits("celebrate", 110)).toBe(false);
    expect(gate.admits("love", 110)).toBe(true);
    expect(gate.admits("love", 139.9)).toBe(false);
    expect(gate.admits("love", 140)).toBe(true);
  });

  it("clamps the random values it is given", () => {
    expect(nextVisitDelay("rare", 0)).toBe(600);
    expect(nextVisitDelay("normal", 1)).toBe(540);
    expect(nextVisitDelay("frequent", 9)).toBe(180);
    expect(nextVisitDelay("frequent", -1)).toBe(90);
    expect(randomGreeting(0)).toBe("happy");
    expect(randomGreeting(0.999)).toBe("love");
    expect(randomGreeting(5)).toBe("love");
  });

  it("dozes sooner at night and blinks half as often in Energy saver", () => {
    expect(blinksBeforeSleep(12)).toBe(36);
    expect(blinksBeforeSleep(22)).toBe(12);
    expect(blinksBeforeSleep(5)).toBe(12);
    expect(blinksBeforeSleep(6)).toBe(36);
    expect(blinkInterval(true)).toEqual([5.2, 12.8]);
    expect(blinkInterval(false)).toEqual([2.6, 6.4]);
  });

  it("keeps a followed pointer inside its face", () => {
    expect(pointerGaze({ x: 0, y: 0 }, { x: 1000, y: -1000 }, 16)).toEqual({ x: 0.08, y: -0.05 });
    expect(pointerGaze({ x: 5, y: 5 }, { x: 5, y: 5 }, 16)).toEqual({ x: 0, y: 0 });
    expect(pointerGaze({ x: 0, y: 0 }, { x: 10, y: 10 }, 0)).toEqual({ x: 0, y: 0 });
  });

  it("takes power changes and a timer's end as the Mac does", () => {
    expect(powerReaction(true, false, false)).toBe("love");
    expect(powerReaction(false, true, false)).toBe("celebrate");
    expect(powerReaction(false, false, true)).toBe("yawn");
    expect(powerReaction(false, false, false)).toBeNull();
    expect(timerReaction("focus")).toBe("celebrate");
    expect(timerReaction("longBreak")).toBe("ready");
    expect(timerReaction("timer")).toBe("surprised");
  });

  it("gives every moment but a visit a reaction", () => {
    for (const moment of MASCOT_MOMENTS) expect(MOMENT_REACTION[moment] === null).toBe(moment === "visit");
  });
});

describe("shapes", () => {
  it("gives every eye the same number of points, so any can become another", () => {
    for (const eye of EYES) {
      for (const right of [false, true]) {
        const points = eyePoints(eye, 1.6, 3.2, right);
        expect(points).toHaveLength(EYE_CURVES);
        for (const point of points) {
          expect(Number.isFinite(point.x)).toBe(true);
          expect(Number.isFinite(point.y)).toBe(true);
        }
      }
    }
  });

  it("writes each eye with the same commands, which is what lets it morph", () => {
    const shape = (d: string) => d.replace(/[^A-Z]/g, "");
    const open = shape(eyePath("open", 2, 4, { x: 4, y: 4 }, false));
    expect(open).toBe(`M${"C".repeat(EYE_CURVES)}Z`);
    for (const eye of EYES) expect(shape(eyePath(eye, 2, 4, { x: 4, y: 4 }, true))).toBe(open);
  });

  it("mirrors the right eye of a lopsided one", () => {
    const left = eyePoints("determined", 2, 4, false);
    const right = eyePoints("determined", 2, 4, true);
    const span = (points: { x: number }[]) => points.reduce((sum, point) => sum + point.x, 0);
    expect(span(left)).toBeCloseTo(-span(right), 3);
  });

  it("survives a size too small to draw", () => {
    for (const eye of EYES) expect(eyePoints(eye, 0, 0, false)).toHaveLength(EYE_CURVES);
  });

  it("leaves an outline too short to resample as it is", () => {
    const short = [
      { x: 0, y: 0 },
      { x: 1, y: 1 },
    ];
    expect(resample(short, 24)).toEqual(short);
    expect(smoothClosedPath(short)).toBe("");
  });

  it("draws every body and part, and centres each figure", () => {
    for (const shape of ["ball", "egg", "squircle", "pill"] as const) {
      expect(bodyPath(shape, 18)).toMatch(/^M.*Z$/);
      expect(figureOffset({ style: "minimal", shape })).toBeGreaterThan(0);
    }
    expect(figureOffset({ style: "robot", shape: "ball" })).toBeCloseTo(-0.01);
    for (const part of Object.values(robotPaths(18))) expect(part).toMatch(/^M.*Z$/);
  });
});

describe("tracks", () => {
  it("sizes it to the strip within its limits", () => {
    expect(mascotSize(20, true)).toBe(14);
    expect(mascotSize(40, true)).toBe(18);
    expect(mascotSize(4, true)).toBe(10);
    expect(mascotSize(24, false)).toBe(12);
    expect(mascotSize(64, false)).toBe(20);
  });

  it("rests in a capsule's middle whatever the side", () => {
    expect(capsule.rest).toBe(40);
    expect(capsule.hidden).toBeNull();
    const right = makeTrack({
      stripWidth: 80,
      stripHeight: 24,
      wing: 0,
      cameraWidth: 0,
      floats: true,
      bodyHeight: 20,
      side: "right",
    });
    expect(right.rest).toBe(40);
    expect(right.mirrored).toBe(false);
  });

  it("rests in a wing beside the notch, on the side chosen", () => {
    expect(notch.hidden).toEqual([44, 179]);
    expect(notch.rest).toBeLessThan(44);
    const right = makeTrack({
      stripWidth: 223,
      stripHeight: 24,
      wing: 44,
      cameraWidth: 135,
      floats: false,
      bodyHeight: 24,
      side: "right",
    });
    expect(right.rest).toBe(223 - notch.rest);
    expect(right.mirrored).toBe(true);
    expect(leftSide(right).rest).toBe(notch.rest);
  });

  it("never hops above the island's top", () => {
    for (const track of [capsule, notch]) {
      const top = track.baseline - track.size / 2 - hopHeight(track, 5);
      expect(top).toBeGreaterThanOrEqual(Math.min(track.ceiling, 0) - track.scale);
    }
  });

  it("grows every measure with the preview's scale", () => {
    const big = scaledTrack(notch, 2);
    expect(big.width).toBe(446);
    expect(big.size).toBe(notch.size * 2);
    expect(big.hidden).toEqual([88, 358]);
    expect(big.scale).toBe(2);
  });

  it("finds its way over an activity only where the wing fits", () => {
    const geometry = makeGeometry({
      screen: { width: 1920, height: 1080 },
      devicePixelRatio: 1,
      silhouette: "notch",
      layout: "spacious",
    });
    expect(activityTrack(geometry, geometry.cameraWidth + 2 * 44, "left")).not.toBeNull();
    expect(activityTrack(geometry, geometry.cameraWidth + 2 * 10, "left")).toBeNull();
    const floating = makeGeometry({
      screen: { width: 1920, height: 1080 },
      devicePixelRatio: 1,
      silhouette: "capsule",
      layout: "spacious",
    });
    expect(activityTrack(floating, 100, "right")?.hidden).toBeNull();
    expect(restingTrack(floating, 100, "right").rest).toBe(50);
    expect(restingTrack(geometry, 100, "right").width).toBe(geometry.cameraWidth + 88);
  });
});

describe("visits", () => {
  it("samples every kind in step, from start to end, on both tracks and sides", () => {
    const mirrored = { ...notch, mirrored: true, rest: notch.width - notch.rest };
    for (const track of [capsule, notch, mirrored]) {
      for (const kind of KINDS) {
        const path = visitPath(kind, track);
        const count = path.keyTimes.length;
        expect(count).toBeGreaterThan(1);
        expect(path.x).toHaveLength(count);
        expect(path.lift).toHaveLength(count);
        expect(path.squash).toHaveLength(count);
        expect(path.gaze).toHaveLength(count);
        expect(path.keyTimes[0]).toBe(0);
        expect(path.keyTimes[count - 1]).toBe(1);
        for (let index = 1; index < count; index++) {
          expect(path.keyTimes[index]!).toBeGreaterThanOrEqual(path.keyTimes[index - 1]!);
        }
        // A capsule has no sides to cross, so it only hops where it rests.
        if (kind.type !== "cross" || track.hidden) expect(path.duration).toBeCloseTo(visitDuration(kind));
      }
    }
  });

  it("ends a lap and an arrival where it rests, the rest out of sight", () => {
    for (const kind of KINDS) {
      const path = visitPath(kind, notch);
      const last = path.x[path.x.length - 1]!;
      if (endsOutOfSight(kind)) {
        if (kind.type !== "countdown" && kind.type !== "retreat") {
          expect(last).toBeGreaterThanOrEqual(notch.hidden![0]);
        }
      } else {
        expect(last).toBeCloseTo(notch.rest);
      }
    }
  });

  it("jumps from the far end to the near one in no time", () => {
    const path = visitPath({ type: "lap" }, capsule);
    const at = path.x.findIndex((x) => x === -capsule.size);
    expect(at).toBeGreaterThan(0);
    expect(path.keyTimes[at]).toBe(path.keyTimes[at - 1]);
    expect(LAP_DURATION).toBeGreaterThan(PASS_DURATION);
  });

  it("mirrors a stroll for the right side", () => {
    const right = { ...notch, mirrored: true, rest: notch.width - notch.rest };
    const seen = visitPath({ type: "pass" }, right);
    const plain = visitPath({ type: "pass" }, notch);
    expect(seen.x[10]).toBeCloseTo(notch.width - plain.x[10]!);
    expect(seen.gaze[40]).toBeCloseTo(-plain.gaze[40]!);
  });

  it("watches a countdown from the camera's left whichever side it rests on", () => {
    const right = { ...notch, mirrored: true, rest: notch.width - notch.rest };
    const path = visitPath({ type: "countdown", total: 5 }, right);
    const middle = path.x[Math.floor(path.x.length / 2)]!;
    expect(middle).toBeLessThan(notch.hidden![0]);
    const gazes = path.gaze.filter((gaze) => gaze === 0.07);
    expect(gazes.length).toBeGreaterThan(0);
  });

  it("hands the strip back on its way home", () => {
    const celebrate: VisitKind = { type: "cameo", reaction: "celebrate" };
    expect(handBack(celebrate, false)).toBeCloseTo(CAMEO_ARRIVAL + cameoHold("celebrate"));
    expect(handBack(celebrate, true)).toBeCloseTo(CAMEO_ARRIVAL + cameoHold("celebrate") + 0.08);
    expect(handBack({ type: "linger", reaction: "yawn" }, false)).toBeCloseTo(cameoHold("yawn"));
    expect(handBack({ type: "lap" }, false)).toBeNull();
  });

  it("names what each kind does", () => {
    expect(visitReaction({ type: "cameo", reaction: "love" })).toBe("love");
    expect(visitReaction({ type: "pass" })).toBeNull();
    expect(watchesTimer({ type: "retreat" })).toBe(true);
    expect(watchesTimer({ type: "cameo", reaction: "love" })).toBe(false);
    expect(takesOnlyItsWing({ type: "pass" })).toBe(false);
    expect(takesOnlyItsWing({ type: "linger", reaction: "yawn" })).toBe(true);
    expect(EXPRESSIONS.wink.left).toBe("open");
  });
});
