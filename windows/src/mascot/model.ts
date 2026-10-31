// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The companion is a small friend who lives in the island: a coloured shape
// with two ink eyes and no mouth, or a little robot (NotchMascotSupport.swift;
// spec 05 §12). The code calls it the mascot, because "companion" already
// names the second activity sharing the closed island on the Mac.

export type MascotStyle = "minimal" | "robot";
export type MascotShape = "ball" | "egg" | "squircle" | "pill";
export type MascotPalette = "pearl" | "lemon" | "peach" | "rose" | "lilac" | "sky" | "mint";
export type MascotSide = "left" | "right";
export type MascotFrequency = "rare" | "normal" | "frequent";

export const MASCOT_STYLES: readonly MascotStyle[] = ["minimal", "robot"];
export const MASCOT_SHAPES: readonly MascotShape[] = ["ball", "egg", "squircle", "pill"];
/** In the order of the rainbow after white, so the swatches read as one row. */
export const MASCOT_PALETTES: readonly MascotPalette[] = ["pearl", "lemon", "peach", "rose", "lilac", "sky", "mint"];
export const MASCOT_SIDES: readonly MascotSide[] = ["left", "right"];
export const MASCOT_FREQUENCIES: readonly MascotFrequency[] = ["rare", "normal", "frequent"];

type Rgb = readonly [number, number, number];

const css = ([r, g, b]: Rgb, alpha = 1) =>
  `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)}${alpha < 1 ? ` / ${alpha}` : ""})`;

const PALETTES: Record<MascotPalette, { light: Rgb; shade: Rgb }> = {
  pearl: { light: [0.99, 0.98, 0.96], shade: [0.84, 0.85, 0.9] },
  lemon: { light: [1.0, 0.96, 0.64], shade: [0.97, 0.83, 0.3] },
  peach: { light: [1.0, 0.86, 0.74], shade: [0.98, 0.64, 0.5] },
  rose: { light: [1.0, 0.8, 0.87], shade: [0.95, 0.54, 0.68] },
  lilac: { light: [0.88, 0.83, 1.0], shade: [0.69, 0.59, 0.96] },
  sky: { light: [0.77, 0.9, 1.0], shade: [0.43, 0.67, 0.97] },
  mint: { light: [0.72, 0.97, 0.86], shade: [0.4, 0.83, 0.66] },
};

/** The body's lit top and shaded bottom; the shade also colours the robot's antenna and ears. */
export const paletteColors = (palette: MascotPalette) => ({
  light: css(PALETTES[palette].light),
  shade: css(PALETTES[palette].shade),
});

/** The eyes, a soft black that still reads on the black island's edge. */
export const INK = css([0.07, 0.07, 0.08]);
/** The robot's face plate, where its lit eyes sit. */
export const VISOR = css([0.09, 0.09, 0.11]);
export const SHINE = css([1, 1, 1], 0.55);

export interface MascotLook {
  style: MascotStyle;
  shape: MascotShape;
  palette: MascotPalette;
}

export const STANDARD_LOOK: MascotLook = { style: "minimal", shape: "ball", palette: "pearl" };

export const sameLook = (a: MascotLook, b: MascotLook) =>
  a.style === b.style && a.shape === b.shape && a.palette === b.palette;

export type MascotEye = "open" | "closed" | "happy" | "heart" | "sleepy" | "wide" | "squint" | "determined";

export type MascotMood =
  | "idle"
  | "happy"
  | "wink"
  | "love"
  | "sleepy"
  | "determined"
  | "surprised"
  | "searching"
  | "thinking"
  | "confused"
  /** Wide awake, while Keep Awake holds the PC up. */
  | "alert";

export interface MascotExpression {
  left: MascotEye;
  right: MascotEye;
  /** Where the eyes look, in shares of its size. Y grows downward. */
  gaze: { x: number; y: number };
  /** The head's lean in radians, clockwise on screen. */
  tilt: number;
}

const expression = (left: MascotEye, right: MascotEye, x = 0, y = 0, tilt = 0): MascotExpression => ({
  left,
  right,
  gaze: { x, y },
  tilt,
});

export const EXPRESSIONS: Record<MascotMood, MascotExpression> = {
  idle: expression("open", "open"),
  happy: expression("happy", "happy", 0, -0.02),
  wink: expression("open", "happy", 0, 0, -0.08),
  love: expression("heart", "heart"),
  sleepy: expression("sleepy", "sleepy", 0, 0.03, 0.05),
  determined: expression("determined", "determined"),
  surprised: expression("wide", "wide", 0, -0.02),
  searching: expression("open", "open", 0.05, 0),
  thinking: expression("squint", "open", 0.06, -0.06, -0.1),
  confused: expression("open", "squint", -0.03, 0.01, 0.14),
  alert: expression("wide", "wide"),
};

const eyeBlinks = (eye: MascotEye) => eye === "open" || eye === "wide" || eye === "squint";

/** Open eyes blink now and then. Closed, curved or heart eyes do not. */
export const moodBlinks = (mood: MascotMood) => eyeBlinks(EXPRESSIONS[mood].left) && eyeBlinks(EXPRESSIONS[mood].right);

export interface MascotFace {
  leftEye: { x: number; y: number };
  rightEye: { x: number; y: number };
  /** An open eye's width and height. */
  eye: { width: number; height: number };
  /** The soft highlight on the body: its centre, radii and lean. */
  shine: { x: number; y: number; rx: number; ry: number; angle: number };
}

const face = (
  lx: number,
  rx: number,
  y: number,
  width: number,
  height: number,
  sx: number,
  sy: number,
  srx: number,
  sry: number,
  angle: number,
): MascotFace => ({
  leftEye: { x: lx, y },
  rightEye: { x: rx, y },
  eye: { width, height },
  shine: { x: sx, y: sy, rx: srx, ry: sry, angle },
});

/** Where the face sits on each body, in shares of its size from the top left corner. */
export function faceOf(look: Pick<MascotLook, "style" | "shape">): MascotFace {
  if (look.style === "robot") return face(0.36, 0.64, 0.63, 0.085, 0.16, 0.27, 0.34, 0.07, 0.04, -0.3);
  switch (look.shape) {
    case "ball":
      return face(0.36, 0.64, 0.5, 0.1, 0.2, 0.31, 0.29, 0.085, 0.055, -0.5);
    case "egg":
      return face(0.375, 0.625, 0.55, 0.095, 0.19, 0.33, 0.31, 0.075, 0.05, -0.6);
    case "squircle":
      return face(0.35, 0.65, 0.53, 0.1, 0.2, 0.24, 0.28, 0.08, 0.05, -0.55);
    case "pill":
      return face(0.37, 0.63, 0.57, 0.09, 0.19, 0.2, 0.42, 0.07, 0.045, -0.45);
  }
}

// MARK: Reactions

/** A short reaction to something the island saw happen. */
export type MascotReaction =
  /** Something finished well: an agent's task, a download, a file dropped in. */
  | "celebrate"
  /** Plugged in to charge, or petted. */
  | "love"
  /** A timer ran out. */
  | "surprised"
  /** A screenshot: a hard blink, as a camera's flash. */
  | "flash"
  /** Something failed. */
  | "confused"
  /** The PC was unlocked: it wakes up glad to see you. */
  | "wakeUp"
  /** Keep Awake let go: a yawn. */
  | "yawn"
  /** Keep Awake took hold, or the microphone opened: wide eyes and a hop. */
  | "perk"
  /** An event the island counted down to began: two quick hops, time to go. */
  | "bounce"
  /** The microphone went quiet: eyes shut a moment as it ducks. */
  | "hush"
  /** A timer started: a determined look and a little nod. */
  | "ready"
  /** Music started: smiling eyes and a bob to the beat. */
  | "groove";

/** About how long each plays, so it stays out for all of it over an activity. */
export const REACTION_LENGTH: Record<MascotReaction, number> = {
  celebrate: 1.0,
  love: 1.4,
  surprised: 0.9,
  flash: 0.5,
  confused: 1.2,
  wakeUp: 1.6,
  yawn: 1.2,
  perk: 1.0,
  bounce: 1.0,
  hush: 1.0,
  ready: 1.0,
  groove: 1.6,
};

/** One reaction, as the island publishes it for the companion to play. */
export interface MascotReactionEvent {
  id: number;
  reaction: MascotReaction;
  /** When it was published, in seconds on the page's clock. */
  start: number;
}

/** No reaction within `SPACING` of the last, nor the same one within `REPEAT_INTERVAL`. */
export class ReactionGate {
  static readonly SPACING = 2.5;
  static readonly REPEAT_INTERVAL = 30;
  /** How long one the companion can't show yet waits for it. */
  static readonly PATIENCE = 8;

  private last: { reaction: MascotReaction; time: number } | null = null;

  admits(reaction: MascotReaction, time: number): boolean {
    if (this.last) {
      if (time - this.last.time < ReactionGate.SPACING) return false;
      if (this.last.reaction === reaction && time - this.last.time < ReactionGate.REPEAT_INTERVAL) return false;
    }
    this.last = { reaction, time };
    return true;
  }
}

/** What the Settings preview can act out: a visit, and the moments the island reacts to. */
export type MascotMoment =
  | "visit"
  | "timerStarted"
  | "timeIsUp"
  | "music"
  | "agents"
  | "eventStarts"
  | "downloadFinished"
  | "downloadFailed"
  | "screenshot"
  | "micMuted"
  | "keepAwake"
  | "charging"
  | "lowBattery"
  | "unlocked";

export const MASCOT_MOMENTS: readonly MascotMoment[] = [
  "visit",
  "timerStarted",
  "timeIsUp",
  "music",
  "agents",
  "eventStarts",
  "downloadFinished",
  "downloadFailed",
  "screenshot",
  "micMuted",
  "keepAwake",
  "charging",
  "lowBattery",
  "unlocked",
];

/** Null for a visit, which is a stroll rather than a reaction. */
export const MOMENT_REACTION: Record<MascotMoment, MascotReaction | null> = {
  visit: null,
  timerStarted: "ready",
  timeIsUp: "surprised",
  music: "groove",
  agents: "ready",
  eventStarts: "bounce",
  downloadFinished: "celebrate",
  downloadFailed: "confused",
  screenshot: "flash",
  micMuted: "hush",
  keepAwake: "perk",
  charging: "love",
  lowBattery: "yawn",
  unlocked: "wakeUp",
};

/** What it plays after the first reaction, for a moment told in two beats: an AI agent at work, then done. */
export const momentFollowUp = (moment: MascotMoment): MascotReaction | null =>
  moment === "agents" ? "celebrate" : null;

// MARK: Life

/** Turned on in Settings, it says hello almost at once. */
export const WELCOME_DELAY = 1.5;
/** Music starting brings it out to bob along at most this often. */
export const GROOVE_INTERVAL = 600;
/** It waits this long after the music starts, until the song's strip has settled. */
export const GROOVE_DELAY = 1.2;
/** An AI agent getting to work has it hand the island over with a ready face at most this often. */
export const AGENT_START_INTERVAL = 600;
/** Hiding when idle, it goes into the island this long after it last did anything. */
export const HIDE_DELAY = 30;
/** Busy just then, or under the pointer, it tries again this much later. */
export const HIDE_RETRY = 3;
/** Its size in a notice it stands in, in place of the notice's symbol. */
export const NOTICE_SIZE = 18;
/** Room a wing keeps beyond the companion for it to come out in. */
export const WING_ROOM = 10;
/** The share of blinks at rest its eyes wander to one side before. */
export const GLANCE_CHANCE = 0.25;

const VISIT_DELAY: Record<MascotFrequency, readonly [number, number]> = {
  rare: [600, 1200],
  normal: [240, 540],
  frequent: [90, 180],
};

/** Visits come every few minutes, never on a fixed beat. */
export function nextVisitDelay(frequency: MascotFrequency, random = Math.random()): number {
  const [low, high] = VISIT_DELAY[frequency];
  return low + (high - low) * Math.min(1, Math.max(0, random));
}

const GREETINGS: readonly MascotMood[] = ["happy", "wink", "love"];

export function randomGreeting(random = Math.random()): MascotMood {
  return GREETINGS[Math.min(GREETINGS.length - 1, Math.max(0, Math.floor(random * GREETINGS.length)))]!;
}

/** A blink comes this long after the previous one, twice as long in Energy saver. */
export function blinkInterval(lowPower: boolean): readonly [number, number] {
  return lowPower ? [5.2, 12.8] : [2.6, 6.4];
}

/** After this many blinks at rest its eyes grow heavy: about three minutes by day, under a minute late at night. */
export const blinksBeforeSleep = (hour: number) => (hour >= 22 || hour < 6 ? 12 : 36);

/** Where its eyes go for a pointer at `to`, with its centre at `from`: toward it, within its face. */
export function pointerGaze(from: { x: number; y: number }, to: { x: number; y: number }, size: number) {
  if (size <= 0) return { x: 0, y: 0 };
  const dx = (to.x - from.x) / (size * 3);
  const dy = (to.y - from.y) / (size * 2);
  return { x: Math.max(-1, Math.min(1, dx)) * 0.08, y: Math.max(-1, Math.min(1, dy)) * 0.05 };
}

/** Glad when the charger goes in, cheering once the battery is full, tired when it runs low. */
export function powerReaction(pluggedIn: boolean, charged: boolean, low: boolean): MascotReaction | null {
  return pluggedIn ? "love" : charged ? "celebrate" : low ? "yawn" : null;
}

/** Startled by a plain timer's ring, glad at the end of a focus session, ready to go again after a break. */
export function timerReaction(phase: "timer" | "stopwatch" | "focus" | "shortBreak" | "longBreak"): MascotReaction {
  return phase === "focus" ? "celebrate" : phase === "shortBreak" || phase === "longBreak" ? "ready" : "surprised";
}
