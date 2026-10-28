// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Clawd in pixels (NotchClawdSprite.swift; spec 02-sessions §5.4-5.5): a body
// 13 pixels wide and 8 tall, eyes a pixel wide and two tall, arms and four
// short legs, with each costume part a small stamp laid over it. Plain data
// and pure functions; Clawd.tsx only puts the result on screen.

import {
  SHELL_RGB,
  type Character,
  type ClawdCostume,
  type ClawdEyewear,
  type ClawdExtra,
  type ClawdHat,
  type ClawdShell,
  type Rgb,
} from "./costume";

// MARK: Canvas

export const WIDTH = 24;
export const HEIGHT = 22;
/** Where the body's top-left pixel sits on the canvas. */
export const BODY_X = 5;
export const BODY_Y = 12;
/**
 * The part of the canvas a figure is fitted by, the same for every costume
 * so every Clawd draws at one size; tall hats, raised arms and bubbles reach
 * past it and are not clipped.
 */
export const FRAME = { left: 3, columns: 17, top: 8, rows: 14 } as const;

/** Steps a second while it moves. */
export const RATE = 12;
/** How long the hop at the end of work, or after a wardrobe change, lasts. */
export const CHEER_LENGTH = 1.4;

// MARK: Poses

/** What a Clawd does for as long as it is shown. */
export type ClawdMotion = "rest" | "content" | "doze" | "think" | "work" | "slow" | "alert" | "ask" | "done";
export const MOTIONS: readonly ClawdMotion[] = [
  "rest",
  "content",
  "doze",
  "think",
  "work",
  "slow",
  "alert",
  "ask",
  "done",
];

export type Arms = "down" | "up" | "typeLeft" | "typeRight" | "waveUp" | "waveOut";
export type Legs = "stand" | "stepLeft" | "stepRight";
export type Eyes = "open" | "closed" | "happy" | "left" | "right";
export type Bubble = { kind: "alert" } | { kind: "question" } | { kind: "sleep"; phase: number };

/** One drawing of a Clawd. Equal keys draw the same image. */
export interface ClawdPose {
  arms: Arms;
  legs: Legs;
  eyes: Eyes;
  bubble: Bubble | null;
  /** How far the body rises over its legs, which stretch to stay on the ground. */
  rise: number;
  /** How far it is off the ground, legs tucked. */
  jump: number;
  /** Every other step, for what turns or steams on its own. */
  tick: number;
}

export function basePose(overrides: Partial<ClawdPose> = {}): ClawdPose {
  return { arms: "down", legs: "stand", eyes: "open", bubble: null, rise: 0, jump: 0, tick: 0, ...overrides };
}

/** `arms.legs.eyes.bubble.rise.jump.tick`, as the Mac's cache key. */
export function poseKey(pose: ClawdPose): string {
  const b = pose.bubble;
  const bubble = !b ? "-" : b.kind === "alert" ? "!" : b.kind === "question" ? "?" : `z${b.phase}`;
  return `${pose.arms}.${pose.legs}.${pose.eyes}.${bubble}.${pose.rise}.${pose.jump}.${pose.tick}`;
}

export interface PoseOptions {
  /** Seconds since the hop for finished work (or a wardrobe change) began. */
  cheer?: number | null;
  /** Reduce Motion: one pose that says the same without moving. */
  still?: boolean;
  /** Without bubbles, as in a notice with no room above, no "!", "?" or "z". */
  bubbles?: boolean;
}

/**
 * The pose `motion` has reached `elapsed` seconds in. `seed` (the costume
 * key's FNV-1a) sets the Clawd's own rhythm, so a row never blinks as one.
 */
export function pose(motion: ClawdMotion, elapsed: number, seedValue: bigint, options: PoseOptions = {}): ClawdPose {
  const result = posed(motion, elapsed, seedValue, options);
  if (options.bubbles === false) result.bubble = null;
  return result;
}

function posed(motion: ClawdMotion, elapsed: number, seedValue: bigint, options: PoseOptions): ClawdPose {
  if (options.still) return stillPose(motion);
  const time = Number.isFinite(elapsed) ? Math.max(0, elapsed) : 0;
  const cheer = options.cheer;
  if (cheer != null && cheer >= 0 && cheer < CHEER_LENGTH) return cheerPose(cheer);
  const step = Math.trunc(time * RATE);
  const p = basePose({ tick: Math.trunc(step / 3) % 2 });
  const rhythm = rhythmOf(seedValue);
  switch (motion) {
    case "rest":
      p.eyes = blinksWith(time, rhythm) ? "closed" : glance(time, rhythm);
      p.rise = breath(time, 2.4);
      break;
    case "content":
      p.eyes = "happy";
      p.rise = breath(time, 2.4);
      break;
    case "doze":
      p.eyes = "closed";
      p.rise = breath(time, 3.2);
      p.bubble = { kind: "sleep", phase: Math.trunc(time / 0.7) % 3 };
      break;
    case "think": {
      p.arms = Math.trunc(step / 2) % 2 === 0 ? "typeLeft" : "typeRight";
      const look = Math.trunc(time / 0.8) % 4;
      p.eyes = blinksWith(time, rhythm) ? "closed" : look === 1 ? "left" : look === 3 ? "right" : "open";
      break;
    }
    case "slow":
      p.arms = Math.trunc(step / 4) % 2 === 0 ? "typeLeft" : "typeRight";
      p.eyes = blinksWith(time, rhythm) ? "closed" : "open";
      break;
    case "work":
      switch (Math.trunc(step / 2) % 4) {
        case 0:
          p.legs = "stepLeft";
          break;
        case 2:
          p.legs = "stepRight";
          break;
        default:
          p.rise = 1;
      }
      p.eyes = blinksWith(time, rhythm) ? "closed" : "open";
      break;
    case "alert":
    case "ask": {
      p.bubble = { kind: motion === "alert" ? "alert" : "question" };
      const cycle = time % 1.6;
      if (cycle < 0.5) {
        p.jump = [0, 2, 3, 3, 2, 0][Math.min(5, Math.trunc(cycle * RATE))];
        p.arms = p.jump > 0 ? "up" : "down";
      } else if (cycle < 1.1) {
        p.arms = Math.trunc((cycle - 0.5) / 0.15) % 2 === 0 ? "waveUp" : "waveOut";
      }
      p.eyes = "open";
      break;
    }
    case "done":
      p.eyes = "happy";
      p.tick = 0;
      break;
  }
  return p;
}

function stillPose(motion: ClawdMotion): ClawdPose {
  switch (motion) {
    case "rest":
      return basePose();
    case "content":
    case "done":
      return basePose({ eyes: "happy" });
    case "doze":
      return basePose({ eyes: "closed", bubble: { kind: "sleep", phase: 1 } });
    case "think":
    case "slow":
      return basePose({ arms: "typeLeft" });
    case "work":
      return basePose({ legs: "stepLeft" });
    case "alert":
      return basePose({ bubble: { kind: "alert" }, arms: "waveUp" });
    case "ask":
      return basePose({ bubble: { kind: "question" }, arms: "waveUp" });
  }
}

/** A crouch, a jump with both arms up, and a landing with smiling eyes. */
function cheerPose(age: number): ClawdPose {
  const p = basePose({ eyes: "happy" });
  const step = Math.trunc(age * RATE);
  const arc = [0, 0, 2, 4, 5, 5, 4, 2, 0];
  if (step < arc.length) {
    p.jump = arc[step];
    p.arms = step < 2 ? "down" : "up";
  } else {
    p.arms = "up";
  }
  return p;
}

interface Rhythm {
  blinkPeriod: number;
  blinkOffset: number;
  glancePeriod: number;
  glanceOffset: number;
}

// The rhythm only depends on the seed; one Clawd asks for it a dozen times a
// second, so the BigInt math is kept for the seeds on screen.
const rhythms = new Map<bigint, Rhythm>();

function rhythmOf(seedValue: bigint): Rhythm {
  let rhythm = rhythms.get(seedValue);
  if (!rhythm) {
    const blinkPeriod = 3.4 + Number(seedValue % 23n) / 10;
    const glancePeriod = 6.1 + Number((seedValue >> 16n) % 31n) / 10;
    rhythm = {
      blinkPeriod,
      blinkOffset: (Number((seedValue >> 8n) % 1000n) / 1000) * blinkPeriod,
      glancePeriod,
      glanceOffset: (Number((seedValue >> 24n) % 1000n) / 1000) * glancePeriod,
    };
    if (rhythms.size > 256) rhythms.clear();
    rhythms.set(seedValue, rhythm);
  }
  return rhythm;
}

function blinksWith(time: number, r: Rhythm): boolean {
  return (time + r.blinkOffset) % r.blinkPeriod < 2 / RATE;
}

/** Eyes shut for two steps every few seconds, at the Clawd's own pace. */
export function blinks(time: number, seedValue: bigint): boolean {
  return blinksWith(time, rhythmOf(seedValue));
}

/** Now and then it looks to one side for a moment. */
function glance(time: number, r: Rhythm): Eyes {
  const shifted = time + r.glanceOffset;
  if (shifted % r.glancePeriod >= 0.8) return "open";
  return Math.trunc(shifted / r.glancePeriod) % 2 === 0 ? "left" : "right";
}

/** Up a pixel for the second half of each breath. */
function breath(time: number, period: number): number {
  return time % period < period / 2 ? 0 : 1;
}

// MARK: Motion per phase (NotchClaudeSessionsSupport.swift:189-234)

export type SessionPhase =
  "generating" | "runningTool" | "background" | "waitingPermission" | "waitingInput" | "finished" | "idle";

/** What a session's Clawd does in each phase; an idle session asleep dozes. */
export function motionForPhase(phase: SessionPhase, dozing = false): ClawdMotion {
  switch (phase) {
    case "generating":
      return "think";
    case "runningTool":
      return "work";
    case "background":
      return "slow";
    case "waitingPermission":
      return "alert";
    case "waitingInput":
      return "ask";
    case "finished":
      return "content";
    case "idle":
      return dozing ? "doze" : "rest";
  }
}

/** A subagent works while it runs and keeps still, smiling, once done. */
export function motionForSubagent(running: boolean): ClawdMotion {
  return running ? "work" : "done";
}

/** A notice's Clawd matches what the notice says (shown without bubbles). */
export function motionForNotice(kind: "finished" | "input" | "permission"): ClawdMotion {
  return kind === "finished" ? "content" : kind === "input" ? "ask" : "alert";
}

/** A sleeping row fades a little and stays readable (`dozingOpacity`). */
export const DOZING_OPACITY = 0.65;
/** A finished subagent's Clawd is drawn at half opacity. */
export const DONE_SUBAGENT_OPACITY = 0.5;

// MARK: Stamps

/** A small drawing, letters standing for colors, placed in the body's own pixels. */
export interface Stamp {
  x: number;
  y: number;
  rows: readonly string[];
}

const s = (x: number, y: number, rows: readonly string[]): Stamp => ({ x, y, rows });

const DOWN_LEFT = s(-2, 4, ["CC", "CC"]);
const DOWN_RIGHT = s(13, 4, ["CC", "CC"]);
const UP_LEFT = s(-3, 0, ["CC.", "CC.", ".CC", ".CC", "..C"]);
const UP_RIGHT = s(13, 0, [".CC", ".CC", "CC.", "CC.", "C.."]);

export const ARM_STAMPS: Readonly<Record<Arms, readonly Stamp[]>> = {
  down: [DOWN_LEFT, DOWN_RIGHT],
  up: [UP_LEFT, UP_RIGHT],
  typeLeft: [s(-2, 5, ["CC", "CC"]), DOWN_RIGHT],
  typeRight: [DOWN_LEFT, s(13, 5, ["CC", "CC"])],
  waveUp: [DOWN_LEFT, UP_RIGHT],
  waveOut: [DOWN_LEFT, s(13, 2, ["..C", ".CC", "CC.", "C.."])],
};

export function eyeStamp(eyes: Eyes, x: number): Stamp {
  switch (eyes) {
    case "open":
      return s(x, 2, ["E", "E"]);
    case "closed":
      return s(x - 1, 3, ["EEE"]);
    case "happy":
      return s(x - 1, 2, [".E.", "E.E"]);
    case "left":
      return s(x - 1, 2, ["E", "E"]);
    case "right":
      return s(x + 1, 2, ["E", "E"]);
  }
}

export const EYEWEAR_STAMPS: Readonly<Record<Exclude<ClawdEyewear, "none">, Stamp>> = {
  glasses: s(0, 1, [".KKKKK.KKKKK.", "KK...KKK...KK", ".K...K.K...K.", ".KKKKK.KKKKK."]),
  shades: s(0, 1, ["KKKKKKKKKKKKK", ".LlLLL.LlLLL.", ".LLLLL.LLLLL.", "..LLL...LLL.."]),
  round: s(1, 1, [".KKK...KKK.", "K...KKK...K", "K...K.K...K", ".KKK...KKK."]),
  monocle: s(7, 1, [".YYY.", "Y...Y", "Y...Y", ".YYY.", "....y", "...y.", "...y."]),
  anaglyph: s(0, 1, ["WWWWWWWWWWWWW", "WRRRRRWTTTTTW", "WRRRRRWTTTTTW", ".WWWWW.WWWWW."]),
  batmanMask: s(0, -3, [
    "H...........H",
    "KH.........HK",
    "KKHHHHHHHHHKK",
    "KKKKKKKKKKKKK",
    "KKKKKKKKKKKKK",
    "KKWWWKKKWWWKK",
    "KKKKKKKKKKKKK",
    "KKKK.....KKKK",
    "KKK.......KKK",
    ".KK.......KK.",
  ]),
  ironManMask: s(0, 0, [
    ".RRRYYYYYRRR.",
    "RYYYYYYYYYYYR",
    "RYYTTYYYTTYYR",
    "RYYYYYYYYYYYR",
    "RRYYYYYYYYYRR",
    "RRYYYsssYYYRR",
    ".RRYYYYYYYRR.",
    "..RRRRRRRRR..",
  ]),
  spiderManMask: s(0, -1, [
    ".RRRRRRRRRRR.",
    "RRRRRRKRRRRRR",
    "RKWWWKRKWWWKR",
    "RKKWWKRKWWKKR",
    "RRKWWKRKWWKRR",
    ".BBRRRKRRRBB.",
  ]),
  doomMask: s(0, 0, [
    ".sSSSSSSSSSs.",
    "sSSSSSSSSSSSs",
    "sKKKKSsSKKKKs",
    "sSSSSSsSSSSSs",
    "DSsSSSsSSSsSD",
    "sSKKKKKKKKKSs",
    ".sSKsKsKsKSs.",
    "..ssSSSSSss..",
  ]),
  wolverineMask: s(0, -3, [
    "H...........H",
    "KH.........HK",
    "KKH.......HKK",
    "KKKKYYYYYKKKK",
    "KKKKYYYYYKKKK",
    "KKWWWYYYWWWKK",
    "KKKYYYYYYYKKK",
    "YYYY.....YYYY",
    ".YYY.....YYY.",
    "..YY.....YY..",
  ]),
  flashMask: s(-2, -1, [
    "....RRRRRRRRR....",
    "YY.RRRRRRRRRRR.YY",
    ".YYRRRRRRRRRRRYY.",
    "..RRWWWRRRWWWRR..",
    "..RRRRRRRRRRRRR..",
    "..RRRR.....RRRR..",
    "..RRR.......RRR..",
  ]),
  captainMask: s(0, -2, [
    "...BBBWBBB...",
    ".BBBBWBWBBBB.",
    "WBBBWWWWWBBBW",
    "WBBBWBBBWBBBW",
    "BWWWBBBBBWWWB",
    "BBBBBBBBBBBBB",
    "BBBB.....BBBB",
    ".BB.......BB.",
  ]),
  deadpoolMask: s(0, 0, [
    ".RRRRRRRRRRR.",
    "RRKKRRRRRKKRR",
    "RKKKKRRRKKKKR",
    "RKWWKRRRKWWKR",
    "RKKKKRRRKKKKR",
    "RRKKRRRRRKKRR",
    ".RRRRRKRRRRR.",
    "..RRRRRRRRR..",
  ]),
  venomMask: s(0, 0, [
    ".HHHHHHHHHHH.",
    "KWKKKKKKKKKWK",
    "KWWKKKKKKKWWK",
    "KWWWKKKKKWWWK",
    "KKKKKKKKKKKKK",
    "KWKWKWKWKWKWK",
    ".KWKWKWKWKWK.",
    "..KKRRRRRKK..",
  ]),
  blackPantherMask: s(0, -2, [
    "H...........H",
    "KH.........HK",
    "KKHHHHHHHHHKK",
    "KsKKKKsKKKKsK",
    "KKsWWKsKWWsKK",
    "KKKKKsKsKKKKK",
    "KKsKKKsKKKsKK",
    "KKKsKKsKKsKKK",
    ".KKKsKsKsKKK.",
    "..KKKsssKKK..",
  ]),
  daredevilMask: s(0, -2, [
    ".R.........R.",
    ".RR.......RR.",
    "RRRRRRRRRRRRR",
    "RRRRRRRRRRRRR",
    "RRKKKRRRKKKRR",
    "RRRKRRRRRKRRR",
    "RRRR.....RRRR",
    ".RRR.....RRR.",
  ]),
  starLordMask: s(0, 0, [
    ".sSSSSSSSSSs.",
    "sSSsssSsssSSs",
    "SsRRRRsRRRRsS",
    "SssRRRsRRRssS",
    "DSsssSSSsssSD",
    "DSSSsKKKsSSSD",
    ".sSSsKsKsSSs.",
    "..sSSSSSSSs..",
  ]),
  greenLanternMask: s(0, 1, ["Q.QQQQ.QQQQ.Q", "QQWWWQQQWWWQQ", ".QQWQQ.QQWQQ."]),
  robinMask: s(0, 1, ["K.KKKK.KKKK.K", "KKWWWKKKWWWKK", ".KKWKK.KKWKK."]),
};

export function hatStamps(hat: ClawdHat, tick: number): Stamp[] {
  switch (hat) {
    case "none":
      return [];
    case "beanie":
      return [s(0, -4, [".....WWW.....", "...AAAAAAA...", ".AAAAAAAAAAA.", "AAAAAAAAAAAAA", "aAaAaAaAaAaAa"])];
    case "cap":
      return [s(0, -3, ["...AAAAAAA......", ".AAAAAWAAAAA....", "AAAAAAAAAAAAaaaa"])];
    case "beret":
      return [s(-1, -4, [".........a....", "...AAAAAAAAA..", ".AAAAAAAAAAAAA", "AAAAAAAAAAAAAA", ".aaaaaaaaaaa.."])];
    case "topHat":
      return [s(1, -6, ["..DDDDDDD..", "..DdDDDDD..", "..DdDDDDD..", "..RRRRRRR..", "..DDDDDDD..", "DDDDDDDDDDD"])];
    case "crown":
      return [s(1, -4, ["Y....Y....Y", "YY..YYY..YY", "YYYYYRYYYYY", "yyyyyyyyyyy"])];
    case "party":
      return [s(3, -7, ["...Y...", "...A...", "..WWW..", "..AAA..", ".WWWWW.", ".AAAAA.", "WWWWWWW"])];
    case "propeller":
      return [
        s(1, -5, [
          tick === 0 ? "RRRR.s.BBBB" : "..RR.s.BB..",
          ".....s.....",
          "...RRYBB...",
          ".RRRRYBBBB.",
          "RRRRRYBBBBB",
        ]),
      ];
    case "chef":
      return [s(1, -6, ["..WWW.WWW..", ".WWWWWWWWW.", "WWWWWWWWWWW", ".WWWWWWWWW.", "..WWWWWWW..", "..wwwwwww.."])];
    case "doomHood":
      return [
        s(-1, -3, [
          ".....QQQQQ.....",
          "...QQQQQQQQQ...",
          ".QQQQqqqqqQQQQ.",
          ".QQqq.....qqQQ.",
          "QQq.........qQQ",
          "QQ...........QQ",
          "QQ...........QQ",
          "Qq...........qQ",
          "q.............q",
        ]),
      ];
    case "thorHelmet":
      return [
        s(-2, -5, [
          "W...............W",
          "WW.............WW",
          ".WW..SSSSSSS..WW.",
          "..WWSSSSSSSSSWW..",
          "...SSSSSSSSSSS...",
        ]),
      ];
    case "lokiHorns":
      return [
        s(0, -7, [
          "Y...........Y",
          "YY.........YY",
          ".Y.........Y.",
          ".YY.......YY.",
          "..YY.....YY..",
          "...YYYYYYY...",
          "...QQQYQQQ...",
        ]),
      ];
    case "wonderTiara":
      return [s(0, -2, ["Y.....Y.....Y", "YYYYYYYYYYYYY", ".YYYYRYYYYYY."])];
  }
}

/** Where the right hand is in each pose, for what it holds. */
function rightHand(arms: Arms, character: Character = "clawd"): [number, number] {
  if (character === "codex") {
    switch (arms) {
      case "down":
      case "typeLeft":
        return [14, 3];
      case "typeRight":
        return [14, 4];
      case "up":
      case "waveUp":
        return [14, -3];
      case "waveOut":
        return [15, -1];
    }
  }
  switch (arms) {
    case "down":
    case "typeLeft":
      return [15, 3];
    case "typeRight":
      return [15, 4];
    case "up":
    case "waveUp":
      return [15, -2];
    case "waveOut":
      return [16, 0];
  }
}

// Capes hang from the shoulders to the ground behind the legs and arms,
// in shadow, edged in a bright color so they stand out on the island's
// black and on a shell of their own color. Clasps are drawn in front, with
// the other extras.
const CAPE_ROWS = [
  "...eFFFFFFFFFFFFFe...",
  "...eFFFFFFFFFFFFFe...",
  "..eFFFFFFFFFFFFFFFe..",
  "..eFFFFFFFFFFFFFFFe..",
  ".eFFFFFFFFFFFFFFFFFe.",
  ".eFFFFFFFFFFFFFFFFFe.",
  "eFFFFFFFFFFFFFFFFFFFe",
  "eFFFFFFFFFFFFFFFFFFFe",
  "eFFFFFFFFFFFFFFFFFFFe",
];
const CAPE_HEMS = ["eFF.FFFF.FFF.FFFF.FFe", "eF.FFFF.FFFFF.FFFF.Fe"];
const BAT_CAPE_HEM = "eF.FF.FF.FFF.FF.FF.Fe";

export function capeStamps(extra: ClawdExtra, tick: number): Stamp[] {
  let fill: string, edge: string;
  switch (extra) {
    case "doomCape":
      [fill, edge] = ["q", "Q"];
      break;
    case "strangeCape":
      [fill, edge] = ["r", "Y"];
      break;
    case "thorCape":
    case "supermanCape":
      [fill, edge] = ["r", "R"];
      break;
    case "batmanCape":
      [fill, edge] = ["K", "H"];
      break;
    default:
      return [];
  }
  const hem = extra === "batmanCape" ? BAT_CAPE_HEM : CAPE_HEMS[tick === 0 ? 0 : 1];
  const drawn = [...CAPE_ROWS, hem].map((row) => row.replace(/F/g, fill).replace(/e/g, edge));
  const stamps = [s(-4, 0, drawn)];
  if (extra === "strangeCape") {
    // The Cloak of Levitation's high collar stands up beside the head.
    stamps.push(s(-2, -3, ["y..", "Ry.", "RRy"]), s(12, -3, ["..y", ".yR", "yRR"]));
  }
  return stamps;
}

export function extraStamps(extra: ClawdExtra, arms: Arms, tick: number, character: Character = "clawd"): Stamp[] {
  switch (extra) {
    case "none":
      return [];
    case "bow":
      return [s(9, -2, ["AA.AA", "AAaAA", "AA.AA"])];
    case "scarf":
      return [s(0, 4, ["AAAAAAAAAAAAA", "aaaaaaaaaaaaa", ".........AA..", ".........aA.."])];
    case "bowTie":
      return [s(4, 4, ["KK.KK", "KKsKK", "KK.KK"])];
    case "flower":
      return [s(-1, -2, [".W.", "WYW", ".W."])];
    case "headphones":
      return [
        s(-2, -3, [
          "....SSSSSSSSS....",
          "..SS.........SS..",
          ".S.............S.",
          ".S.............S.",
          "AA.............AA",
          "AA.............AA",
          "AA.............AA",
        ]),
      ];
    case "coffee": {
      const [x, y] = rightHand(arms, character);
      const steam = tick === 0 ? [".w.", "w.."] : ["w..", ".w."];
      return [s(x, y - 2, [...steam, "NNN", "WWW", "WWW"])];
    }
    case "doomCape":
      return [s(0, 0, ["QQY.......YQQ"])];
    case "strangeCape":
      return [s(0, 0, ["RRR.......RRR", "RRRY.....YRRR"])];
    case "thorCape":
      return [s(0, 0, ["SS.........SS", "SS.........SS"])];
    case "supermanCape":
      return [s(0, 0, ["RR.........RR"])];
    case "batmanCape":
      return [s(0, 0, ["HK.........KH"])];
    case "captainShield": {
      const [, y] = rightHand(arms, character);
      return [
        s(10, y - 4, [
          "..RRRRR..",
          ".RWWWWWR.",
          "RWRRRRRWR",
          "RWRBWBRWR",
          "RWRWWWRWR",
          "RWRBWBRWR",
          "RWRRRRRWR",
          ".RWWWWWR.",
          "..RRRRR..",
        ]),
      ];
    }
    case "mjolnir": {
      const [, y] = rightHand(arms, character);
      return [
        s(10, y - 4, ["SSSSSSSSS", "SwwwwwwwS", "SSSSSSSSS", "....N....", "....N....", "....N....", "....Y...."]),
      ];
    }
    case "aquamanTrident": {
      const [, y] = rightHand(arms, character);
      return [
        s(12, y - 4, [
          "Y..Y..Y",
          "Y..Y..Y",
          ".YYYYY.",
          "...Y...",
          "...Y...",
          "...Y...",
          "...Y...",
          "...Y...",
          "...Y...",
        ]),
      ];
    }
    case "lanternRing": {
      const [, y] = rightHand(arms, character);
      return [s(12, y - 4, ["..QQQ..", ".QWWWQ.", "QWWWWWQ", "QWQQQWQ", "QWQWQWQ", ".QWWWQ.", "..QQQ.."])];
    }
    case "wolverineClaws": {
      const [, y] = rightHand(arms, character);
      return [s(10, y - 4, ["S.S.S", "S.S.S", "S.S.S", "S.S.S", ".SSS.", ".NNN."])];
    }
    case "batarang": {
      const [, y] = rightHand(arms, character);
      return [s(10, y - 4, ["d..d.d..d", "Dd.DDD.dD", "DDDDDDDDD", ".DDDDDDD.", "..D.D.D.."])];
    }
    case "hawkeyeBow": {
      const [, y] = rightHand(arms, character);
      return [
        s(12, y - 4, [
          "wP.....",
          "w.P....",
          "w..P...",
          "w..P.W.",
          "SSSSSSW",
          "w..P.W.",
          "w..P...",
          "w.P....",
          "wP.....",
        ]),
      ];
    }
    case "strangeSpell": {
      const [, y] = rightHand(arms, character);
      return [s(10, y - 4, ["..YYY..", ".Y...Y.", "Y.YYY.Y", "Y.Y.Y.Y", "Y.YYY.Y", ".Y...Y.", "..YYY.."])];
    }
    case "webShooter": {
      const [, y] = rightHand(arms, character);
      return [s(10, y - 4, ["W..W..W", ".W.W.W.", "..WWW..", "WWWWWWW", "..WWW..", ".W.W.W.", "W..W..W"])];
    }
    case "ironReactor":
      return [s(4, 4, [".TTT.", "TWWWT", "TWWWT", ".TTT."])];
    case "supermanEmblem":
      return [s(4, 3, ["YYYYY", "YRRRY", "YRYYY", "YYYRY", "YRRRY"])];
    case "flashEmblem":
      return [s(4, 3, [".WWW.", "WWWYW", "WYYYW", "WYWWW", ".WWW."])];
    case "lanternEmblem":
      return [s(4, 3, [".WWW.", "WQQQW", "WQWQW", "WQQQW", ".WWW."])];
  }
}

export function bubbleStamps(bubble: Bubble): Stamp[] {
  switch (bubble.kind) {
    case "alert":
      return [s(15, -7, ["XX", "XX", "XX", "..", "XX"])];
    case "question":
      return [s(15, -7, ["XXX", "..X", ".X.", "...", ".X."])];
    case "sleep": {
      const small = s(13, -3, ["ZZZZ", "..Z.", ".Z..", "ZZZZ"]);
      const large = s(14, -9, ["ZZZZZ", "...Z.", "..Z..", ".Z...", "ZZZZZ"]);
      const phase = ((bubble.phase % 3) + 3) % 3;
      return phase === 0 ? [small] : phase === 1 ? [small, large] : [large];
    }
  }
}

// MARK: Codex's robot

/**
 * Codex's figure fills Clawd's 13 x 8 footprint, so every hat, pair of
 * glasses and extra sits on it unchanged: a terminal with rounded corners,
 * eyes glowing on its dark screen (V), a status light (G), an antenna, two
 * stubby feet and thin arms.
 */
const CODEX_BODY = [
  ".CCCCCCCCCCC.",
  "CCVVVVVVVVVCC",
  "CCVVVVVVVVVCC",
  "CCVVVVVVVVVCC",
  "CCVVVVVVVVVCC",
  "CCCCCCCCCCCCC",
  "CCCCCCCCCGcCC",
  ".ccccccccccc.",
];

const CODEX_DOWN_LEFT = s(-1, 3, ["C", "C", "c"]);
const CODEX_DOWN_RIGHT = s(13, 3, ["C", "C", "c"]);
const CODEX_UP_LEFT = s(-1, -2, ["c", "C", "C", "C"]);
const CODEX_UP_RIGHT = s(13, -2, ["c", "C", "C", "C"]);

export const CODEX_ARM_STAMPS: Readonly<Record<Arms, readonly Stamp[]>> = {
  down: [CODEX_DOWN_LEFT, CODEX_DOWN_RIGHT],
  up: [CODEX_UP_LEFT, CODEX_UP_RIGHT],
  typeLeft: [s(-2, 4, [".C", "cC"]), CODEX_DOWN_RIGHT],
  typeRight: [CODEX_DOWN_LEFT, s(13, 4, ["C.", "Cc"])],
  waveUp: [CODEX_DOWN_LEFT, CODEX_UP_RIGHT],
  waveOut: [CODEX_DOWN_LEFT, s(13, 0, [".c", "C.", "C."])],
};

/** The antenna, drawn under any hat so a hat simply covers it. */
const CODEX_ANTENNA = [s(6, -2, ["G"]), s(6, -1, ["c"])];

// MARK: Compositing

const BODY_ROWS = Array.from({ length: 8 }, () => "C".repeat(13));

const LEGS: Readonly<Record<Character, readonly (readonly [number, boolean])[]>> = {
  clawd: [
    [0, true],
    [2, true],
    [10, false],
    [12, false],
  ],
  codex: [
    [3, true],
    [4, true],
    [8, false],
    [9, false],
  ],
};

function compose(
  costume: ClawdCostume,
  p: ClawdPose,
  character: Character = "clawd",
): { canvas: (string | null)[]; clipped: number } {
  const canvas: (string | null)[] = new Array(WIDTH * HEIGHT).fill(null);
  let clipped = 0;
  const lift = p.rise + p.jump;
  const put = (stamp: Stamp, dy = -lift) => {
    stamp.rows.forEach((line, row) => {
      for (let column = 0; column < line.length; column++) {
        const letter = line[column];
        if (letter === ".") continue;
        const x = BODY_X + stamp.x + column;
        const y = BODY_Y + stamp.y + row + dy;
        if (x < 0 || x >= WIDTH || y < 0 || y >= HEIGHT) {
          clipped++;
          continue;
        }
        canvas[y * WIDTH + x] = letter;
      }
    });
  };
  capeStamps(costume.extra, p.tick).forEach((stamp) => put(stamp));
  // Legs reach the ground under a rising body, and tuck in off it.
  for (const [x, left] of LEGS[character]) {
    const lifted = (p.legs === "stepLeft" && left) || (p.legs === "stepRight" && !left);
    const top = 8 - p.rise;
    const bottom = p.jump > 0 || lifted ? 8 : 9;
    put(
      s(
        x,
        top,
        Array.from({ length: Math.max(1, bottom - top + 1) }, () => "c"),
      ),
      -p.jump,
    );
  }
  if (character === "codex") {
    CODEX_ANTENNA.forEach((stamp) => put(stamp));
    put(s(0, 0, CODEX_BODY));
    CODEX_ARM_STAMPS[p.arms].forEach((stamp) => put(stamp));
  } else {
    put(s(0, 0, BODY_ROWS));
    ARM_STAMPS[p.arms].forEach((stamp) => put(stamp));
  }
  put(eyeStamp(p.eyes, 3));
  put(eyeStamp(p.eyes, 9));
  if (costume.eyewear !== "none") put(EYEWEAR_STAMPS[costume.eyewear]);
  hatStamps(costume.hat, p.tick).forEach((stamp) => put(stamp));
  extraStamps(costume.extra, p.arms, p.tick, character).forEach((stamp) => put(stamp));
  if (p.bubble) bubbleStamps(p.bubble).forEach((stamp) => put(stamp));
  return { canvas, clipped };
}

/** One letter per canvas pixel, row-major from the top, null where nothing is drawn. */
export function letters(costume: ClawdCostume, p: ClawdPose, character: Character = "clawd"): (string | null)[] {
  return compose(costume, p, character).canvas;
}

/** How many pixels fall off the canvas: none, for a drawing that fits. */
export function clippedPixels(costume: ClawdCostume, p: ClawdPose, character: Character = "clawd"): number {
  return compose(costume, p, character).clipped;
}

// MARK: Colors

/** The color hats, scarves and headphones come in on each shell: far enough from it to stand out. */
export const ACCENT: Readonly<Record<ClawdShell, ClawdShell>> = {
  midnight: "sky",
  clay: "sky",
  coral: "teal",
  amber: "iris",
  moss: "coral",
  mint: "orchid",
  teal: "amber",
  sky: "coral",
  iris: "amber",
  orchid: "mint",
  rose: "sky",
};

/** The fixed letters; C, c, A and a come from the costume. */
export const PALETTE: Readonly<Record<string, Rgb>> = {
  Q: [44, 142, 76],
  q: [26, 84, 48],
  // A blue sheen that keeps black parts from vanishing on the island.
  H: [64, 80, 124],
  r: [150, 36, 44],
  P: [150, 84, 200],
  E: [24, 22, 26],
  K: [32, 32, 38],
  D: [66, 66, 78],
  d: [108, 108, 124],
  L: [30, 30, 40],
  l: [150, 170, 205],
  W: [246, 244, 238],
  w: [196, 200, 210],
  R: [226, 64, 64],
  Y: [247, 198, 70],
  y: [196, 142, 44],
  T: [64, 204, 232],
  N: [124, 76, 44],
  B: [72, 132, 232],
  S: [156, 158, 170],
  s: [98, 100, 112],
  X: [255, 199, 71],
  Z: [196, 206, 236],
};

/** 78%, in integers as the Mac does it. */
const shaded = (c: Rgb): Rgb => [
  Math.trunc((c[0] * 78) / 100),
  Math.trunc((c[1] * 78) / 100),
  Math.trunc((c[2] * 78) / 100),
];

/** Codex's screen and the light its eyes and status lamp give off. */
const CODEX_SCREEN: Rgb = [16, 22, 28];
const CODEX_GLOW: Rgb = [124, 240, 204];

/** sRGB out of 255 for a letter on this costume, or null for one with no color. */
export function color(letter: string, costume: ClawdCostume, character: Character = "clawd"): Rgb | null {
  if (character === "codex") {
    if (letter === "V") return CODEX_SCREEN;
    // Eyes glow on the dark screen instead of being dark on the shell.
    if (letter === "E" || letter === "G") return CODEX_GLOW;
  }
  const shell = SHELL_RGB[costume.shell];
  const accent = SHELL_RGB[ACCENT[costume.shell]];
  switch (letter) {
    case "C":
      return shell;
    case "c":
      return shaded(shell);
    case "A":
      return accent;
    case "a":
      return shaded(accent);
    default:
      return Object.hasOwn(PALETTE, letter) ? PALETTE[letter] : null;
  }
}

const hex = (c: Rgb) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");

/** The drawing as rows of CSS colors (`#rrggbb`), null where it is clear. */
export function grid(costume: ClawdCostume, p: ClawdPose, character: Character = "clawd"): (string | null)[][] {
  const canvas = letters(costume, p, character);
  const rows: (string | null)[][] = [];
  for (let y = 0; y < HEIGHT; y++) {
    const row: (string | null)[] = [];
    for (let x = 0; x < WIDTH; x++) {
      const letter = canvas[y * WIDTH + x];
      const rgb = letter ? color(letter, costume, character) : null;
      row.push(rgb ? hex(rgb) : null);
    }
    rows.push(row);
  }
  return rows;
}

/** Straight RGBA bytes, ready for ImageData; every pixel opaque or clear. */
export function pixels(
  costume: ClawdCostume,
  p: ClawdPose,
  character: Character = "clawd",
): Uint8ClampedArray<ArrayBuffer> {
  const bytes = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
  letters(costume, p, character).forEach((letter, index) => {
    const rgb = letter ? color(letter, costume, character) : null;
    if (!rgb) return;
    bytes[index * 4] = rgb[0];
    bytes[index * 4 + 1] = rgb[1];
    bytes[index * 4 + 2] = rgb[2];
    bytes[index * 4 + 3] = 255;
  });
  return bytes;
}

// MARK: Fitting

/** Swift's `.rounded()`: halves go away from zero (Math.round sends -0.5 up). */
const roundHalfAway = (v: number) => Math.sign(v) * Math.round(Math.abs(v));

/**
 * How big a canvas pixel is in CSS px, and where the canvas's top-left goes
 * in a box measured from its top-left: as many whole device pixels per
 * canvas pixel as fit the frame, at least one, frame centred, every edge on
 * a device pixel.
 */
export function layout(
  box: { width: number; height: number },
  scale: number,
): { pixel: number; devicePixels: number; x: number; y: number } {
  const sc = Math.max(1, Number.isFinite(scale) ? scale : 1);
  const w = Number.isFinite(box.width) ? Math.max(0, box.width) : 0;
  const h = Number.isFinite(box.height) ? Math.max(0, box.height) : 0;
  const devicePixels = Math.max(1, Math.floor(Math.min((w * sc) / FRAME.columns, (h * sc) / FRAME.rows)));
  const pixel = devicePixels / sc;
  const midX = FRAME.left + FRAME.columns / 2;
  const midY = FRAME.top + FRAME.rows / 2;
  const x = (w / 2 - midX * pixel) * sc;
  const y = (h / 2 - midY * pixel) * sc;
  return { pixel, devicePixels, x: roundHalfAway(x) / sc, y: roundHalfAway(y) / sc };
}

// MARK: Sizes (spec §5.6), in CSS px

export const CLAWD_SIZES = {
  listRow: { width: 32, height: 30 },
  pageHeader: { width: 40, height: 36 },
  largeHeader: { width: 48, height: 42 },
  subagentRow: { width: 18, height: 14 },
  subagentRowLarge: { width: 22, height: 17 },
  notice: { width: 18, height: 18 },
  settingsSample: { width: 30, height: 28 },
  wardrobeTile: { width: 34, height: 30 },
} as const;
