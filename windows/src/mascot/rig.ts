// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The companion, drawn with nested layers and an SVG, each motion a Web
// Animation (NotchMascotRig). The app says where it goes once and the browser
// draws the way there, with no timer or redraw here: even the blinks chain
// through their animations, each starting from the end of the last.
//
// The layers nest as the Mac's do: root (where it stands) › hopper › squasher
// › tilter › body and item sprites, face window (what it wears covers the eyes
// through) › attention (what its eyes are drawn to) › turner (the eyes move
// with the head) › face (the look its mood keeps) › eyes. The body and what it
// wears are sprites rendered in Blender, one per turn of its head, stacked so
// a turn fades from one into the next. What the Mac adds to a layer's transform goes in the
// separate translate, rotate and scale properties here, which stack on top of
// `transform` by themselves.

import {
  blinkInterval,
  blinksBeforeSleep,
  EXPRESSIONS,
  faceOf,
  GLANCE_CHANCE,
  bodySprite,
  CREAM,
  creamEyes,
  eyeSprite,
  eyeWindow,
  INK,
  itemSprite,
  type MascotEye,
  type MascotExpression,
  type MascotLook,
  type MascotMood,
  type MascotReaction,
  moodBlinks,
  REACTION_LENGTH,
  sameLook,
  STANDARD_LOOK,
  turnOffset,
  yawForGaze,
} from "./model";
import { ART } from "./art";
import type { MascotPath } from "./motion";
import { eyePath, spriteFrame } from "./shapes";
import { spriteUrl } from "./sprites";

const SVG = "http://www.w3.org/2000/svg";
const SETTLE = "cubic-bezier(0.2, 0.8, 0.3, 1)";
const EASE_IN_OUT = "ease-in-out";

/** Seconds on the clock every animation and visit is timed against. */
export const mediaTime = () => performance.now() / 1000;

/** A cue is a one-off motion over its face: results turning up, or eyes darting about. */
export type MascotCue = "celebrate" | "glance";

const div = (className: string) => {
  const element = document.createElement("div");
  element.className = className;
  return element;
};
const svgPath = () => document.createElementNS(SVG, "path");

/** A damped spring as an easing, with how long it takes to settle. */
export function springEasing(damping: number, stiffness: number): { easing: string; duration: number } {
  const omega = Math.sqrt(stiffness);
  const zeta = damping / (2 * omega);
  const at = (t: number) => {
    if (zeta >= 1) return 1 - Math.exp(-omega * t) * (1 + omega * t);
    const damped = omega * Math.sqrt(1 - zeta * zeta);
    return 1 - Math.exp(-zeta * omega * t) * (Math.cos(damped * t) + ((zeta * omega) / damped) * Math.sin(damped * t));
  };
  let settle = 0.1;
  for (let t = 3; t > 0.1; t -= 0.02) {
    if (Math.abs(at(t) - 1) > 0.002) {
      settle = t + 0.02;
      break;
    }
  }
  const points = Array.from({ length: 41 }, (_, index) => at((index / 40) * settle).toFixed(4));
  points[points.length - 1] = "1";
  return { easing: `linear(${points.join(", ")})`, duration: settle };
}

const SPRING_GAZE = springEasing(16, 260);
const SPRING_TILT = springEasing(12, 200);

interface Play {
  key: string;
  /** Seconds. */
  duration: number;
  /** When it starts, on the media clock; now if left out. */
  begin?: number;
  easing?: string;
  fill?: FillMode;
}

export class MascotRig {
  readonly root = div("mr-root");
  look: MascotLook = STANDARD_LOOK;
  size = 0;
  mood: MascotMood = "idle";
  /** Whether it blinks now and then and, after a long rest, grows sleepy. */
  idles = false;
  reduceMotion = false;
  /** Energy saver: it blinks half as often. */
  lowPower = false;

  private readonly hopper = div("mr-layer");
  private readonly squasher = div("mr-layer");
  private readonly tilter = div("mr-layer");
  private readonly attention = div("mr-layer");
  private readonly faceLayer = div("mr-layer");
  /** The body and what it wears: one image per turn of the head, the one showing at full opacity. */
  private readonly body = div("mr-sprites");
  private readonly item = div("mr-sprites");
  private readonly bodyFrames = ART.yaws.map(() => this.makeFrame());
  private readonly itemFrames = ART.yaws.map(() => this.makeFrame());
  /** Where the eyes may show: what it wears covers them as the head turns. */
  private readonly faceWindow = div("mr-layer");
  /** Moves the eyes with the head as it turns. */
  private readonly turner = div("mr-layer");
  /** Which way its head is turned, an index into `ART.yaws`. */
  yaw: number = ART.frontYaw;
  /** Where the eyes look beyond the face it keeps, held until told otherwise. */
  private heldAttention = 0;
  private readonly leftEye = this.makeEye();
  private readonly rightEye = this.makeEye();
  private readonly running = new Map<Element, Map<string, Animation>>();
  private blinks = 0;
  /** It grew sleepy on its own after a long rest, and anything that happens wakes it. */
  private dozed = false;
  /** When its eyes last went after something. */
  private attended = -Infinity;
  private visitEnds = 0;
  private blinksResume = 0;
  private blinkAnimation: Animation | null = null;
  private configured = false;
  private static nextPattern = 0;

  constructor() {
    this.body.append(...this.bodyFrames);
    this.item.append(...this.itemFrames);
    this.root.append(this.hopper);
    this.hopper.append(this.squasher);
    this.squasher.append(this.tilter);
    this.tilter.append(this.body, this.item, this.faceWindow);
    this.faceWindow.append(this.attention);
    this.attention.append(this.turner);
    this.turner.append(this.faceLayer);
    this.faceLayer.append(this.leftEye.box, this.rightEye.box);
  }

  private makeFrame() {
    const image = document.createElement("img");
    image.className = "mr-frame";
    image.alt = "";
    image.decoding = "async";
    image.draggable = false;
    return image;
  }

  /** An eye: its outline, filled with the fabric it is stitched from. */
  private makeEye() {
    const box = div("mr-eye");
    const svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("aria-hidden", "true");
    const defs = document.createElementNS(SVG, "defs");
    const pattern = document.createElementNS(SVG, "pattern");
    pattern.id = `mascot-eye-${MascotRig.nextPattern++}`;
    pattern.setAttribute("patternUnits", "userSpaceOnUse");
    const fabric = document.createElementNS(SVG, "image");
    fabric.setAttribute("preserveAspectRatio", "none");
    pattern.append(fabric);
    defs.append(pattern);
    const path = svgPath();
    svg.append(defs, path);
    box.append(svg);
    return { box, svg, path, pattern, fabric };
  }

  // MARK: Animations

  /** Starts `frames` on `element` in place of the animation under `key`, as Core Animation's add(forKey:). */
  private play(element: Element, frames: Keyframe[], options: Play): Animation {
    const delay = options.begin === undefined ? 0 : (options.begin - mediaTime()) * 1000;
    const animation = element.animate(frames, {
      duration: Math.max(1, options.duration * 1000),
      delay,
      easing: options.easing ?? "linear",
      fill: options.fill ?? "none",
    });
    let keys = this.running.get(element);
    if (!keys) this.running.set(element, (keys = new Map()));
    keys.get(options.key)?.cancel();
    keys.set(options.key, animation);
    const held = (options.fill ?? "none") !== "none";
    animation.onfinish = () => {
      if (!held && keys.get(options.key) === animation) keys.delete(options.key);
    };
    return animation;
  }

  private stop(element: Element, key: string) {
    const keys = this.running.get(element);
    keys?.get(key)?.cancel();
    keys?.delete(key);
  }

  private stopAll(element: Element) {
    this.running.get(element)?.forEach((animation) => animation.cancel());
    this.running.delete(element);
  }

  private current(element: Element, property: "transform" | "translate" | "fill"): string {
    return getComputedStyle(element)[property] || "none";
  }

  // MARK: Look

  /** Draws it at `size` pixels with `look`. Nothing moves, and a new look at the same size fades in. */
  configure(look: MascotLook, size: number) {
    if (this.configured && sameLook(look, this.look) && size === this.size) return;
    const restyled = this.configured && !sameLook(look, this.look) && size === this.size && !this.reduceMotion;
    this.configured = true;
    this.look = look;
    this.size = size;
    for (const layer of [this.root, this.hopper, this.squasher, this.tilter, this.attention, this.faceLayer]) {
      layer.style.width = layer.style.height = `${size}px`;
    }
    for (const layer of [this.faceWindow, this.turner]) layer.style.width = layer.style.height = `${size}px`;
    const frame = spriteFrame(size);
    for (const layer of [this.body, this.item]) {
      layer.style.left = `${frame.x}px`;
      layer.style.top = `${frame.y}px`;
      layer.style.width = layer.style.height = `${frame.width}px`;
    }
    ART.yaws.forEach((_, yaw) => {
      this.bodyFrames[yaw]!.src = spriteUrl(bodySprite(look, yaw)) ?? "";
      const item = spriteUrl(itemSprite(look, yaw));
      const frame = this.itemFrames[yaw]!;
      if (item) frame.src = item;
      else frame.removeAttribute("src");
      // Nothing on its head: no image, not even a broken one.
      frame.style.display = item ? "" : "none";
    });
    const face = faceOf();
    const side = size * 0.4;
    const fabric = spriteUrl(eyeSprite(look));
    const ink = creamEyes(look.palette) ? CREAM : INK;
    const place = (eye: ReturnType<MascotRig["makeEye"]>, at: { x: number; y: number }) => {
      eye.box.style.left = `${at.x * size - side / 2}px`;
      eye.box.style.top = `${at.y * size - side / 2}px`;
      eye.box.style.width = eye.box.style.height = `${side}px`;
      eye.svg.setAttribute("width", String(side));
      eye.svg.setAttribute("height", String(side));
      eye.svg.setAttribute("viewBox", `0 0 ${side} ${side}`);
      for (const node of [eye.pattern, eye.fabric]) {
        node.setAttribute("width", String(side));
        node.setAttribute("height", String(side));
      }
      if (fabric) eye.fabric.setAttribute("href", fabric);
      // Without its sprites it still has eyes.
      eye.path.setAttribute("fill", fabric ? `url(#${eye.pattern.id})` : ink);
    };
    place(this.leftEye, face.leftEye);
    place(this.rightEye, face.rightEye);
    this.showYaw(this.yaw);
    if (restyled) this.play(this.tilter, [{ opacity: 0.2 }, { opacity: 1 }], { key: "look", duration: 0.24 });
    this.apply(EXPRESSIONS[this.mood], false);
  }

  // MARK: Faces

  setMood(mood: MascotMood, animated: boolean) {
    if (mood === this.mood) return;
    const previous = this.mood;
    this.mood = mood;
    this.dozed = false;
    if (mood !== "sleepy") this.blinks = 0;
    const moving = animated && !this.reduceMotion;
    this.apply(EXPRESSIONS[mood], moving);
    if (animated && !this.reduceMotion && this.size > 0) {
      if (mood === "surprised") this.pop();
      else if (mood === "confused") this.wobble();
      else if (mood === "love") this.heartbeat();
      else if (mood === "idle" && previous === "sleepy") this.pop();
    }
    this.syncBlinking();
  }

  private eyeD(eye: MascotEye, right: boolean): string {
    const face = faceOf();
    const side = this.size * 0.4;
    return eyePath(eye, face.eye.width * this.size, face.eye.height * this.size, { x: side / 2, y: side / 2 }, right);
  }

  private gazeTransform(gaze: { x: number; y: number }) {
    return `translate(${gaze.x * this.size}px, ${gaze.y * this.size}px)`;
  }

  private apply(expression: MascotExpression, animated: boolean) {
    if (this.size <= 0) return;
    const left = this.eyeD(expression.left, false);
    const right = this.eyeD(expression.right, true);
    const gaze = this.gazeTransform(expression.gaze);
    const tilt = `rotate(${expression.tilt}rad)`;
    if (animated) {
      for (const [eye, d] of [
        [this.leftEye, left],
        [this.rightEye, right],
      ] as const) {
        this.play(eye.path, [{ d: this.pathValue(eye.path) }, { d: `path("${d}")` }], {
          key: "morph",
          duration: 0.22,
          easing: SETTLE,
        });
      }
      this.play(this.faceLayer, [{ transform: this.current(this.faceLayer, "transform") }, { transform: gaze }], {
        key: "gaze",
        duration: SPRING_GAZE.duration,
        easing: SPRING_GAZE.easing,
      });
      this.play(this.tilter, [{ transform: this.current(this.tilter, "transform") }, { transform: tilt }], {
        key: "tilt",
        duration: SPRING_TILT.duration,
        easing: SPRING_TILT.easing,
      });
    }
    this.leftEye.path.setAttribute("d", left);
    this.rightEye.path.setAttribute("d", right);
    this.faceLayer.style.transform = gaze;
    this.tilter.style.transform = tilt;
    const turn = yawForGaze(expression.gaze.x + this.heldAttention);
    if (animated) this.turnHead([turn], 0.22);
    else this.showYaw(turn);
  }

  // MARK: Turning

  /** The sprites and the eyes for the head turned to `yaw`, at once. */
  private showYaw(yaw: number) {
    this.yaw = yaw;
    ART.yaws.forEach((_, index) => {
      this.bodyFrames[index]!.style.opacity = index === yaw ? "1" : "0";
      this.itemFrames[index]!.style.opacity = index === yaw ? "1" : "0";
    });
    const window = spriteUrl(eyeWindow(this.look, yaw));
    this.faceWindow.style.maskImage = window ? `url("${window}")` : "";
    this.faceWindow.style.webkitMaskImage = this.faceWindow.style.maskImage;
    const frame = spriteFrame(this.size);
    const maskBox = `${frame.width}px ${frame.height}px`;
    this.faceWindow.style.maskSize = this.faceWindow.style.webkitMaskSize = maskBox;
    this.faceWindow.style.maskPosition = this.faceWindow.style.webkitMaskPosition = `${frame.x}px ${frame.y}px`;
    this.faceWindow.style.maskRepeat = this.faceWindow.style.webkitMaskRepeat = "no-repeat";
    this.turner.style.translate = this.turnTranslate(yaw);
  }

  private turnTranslate(yaw: number) {
    const shift = turnOffset(yaw);
    return `${shift.x * this.size}px ${shift.y * this.size}px`;
  }

  /**
   * Turns the head through `yaws` over `duration`, each sprite fading into the
   * next as the eyes move with it. A single turn is where it stays; a run of
   * them (with `offsets`) plays and leaves it where it was.
   */
  private turnHead(yaws: number[], duration: number, offsets?: number[], begin?: number, key = "turn") {
    if (this.size <= 0 || yaws.length === 0) return;
    const steady = offsets === undefined;
    const last = yaws[yaws.length - 1]!;
    if (steady && last === this.yaw) return;
    const values = steady ? [this.yaw, last] : yaws;
    const at = offsets ?? [0, 1];
    ART.yaws.forEach((_, index) => {
      const frames = values.map((yaw, step): Keyframe => ({ opacity: yaw === index ? 1 : 0, offset: at[step]! }));
      for (const frame of [this.bodyFrames[index]!, this.itemFrames[index]!]) {
        this.play(frame, frames, { key, duration, begin, fill: begin === undefined ? "none" : "backwards" });
      }
    });
    this.play(
      this.turner,
      values.map((yaw, step): Keyframe => ({ translate: this.turnTranslate(yaw), offset: at[step]! })),
      { key, duration, begin, fill: begin === undefined ? "none" : "backwards" },
    );
    this.showYaw(last);
  }

  /** Where the head turns for the face it keeps and what holds its eyes. */
  private restingYaw() {
    return yawForGaze(EXPRESSIONS[this.mood].gaze.x + this.heldAttention);
  }

  /** The eye as drawn now, heavy from a doze or partway through a morph. */
  private pathValue(path: SVGPathElement): string {
    const computed = getComputedStyle(path).getPropertyValue("d");
    return computed && computed !== "none" ? computed : `path("${path.getAttribute("d") ?? ""}")`;
  }

  /** Back to its own face at rest, with every motion taken off. */
  reset(mood: MascotMood) {
    this.stopBlinking();
    for (const layer of [
      this.root,
      this.hopper,
      this.squasher,
      this.tilter,
      this.attention,
      this.turner,
      this.faceLayer,
      ...this.bodyFrames,
      ...this.itemFrames,
      this.leftEye.box,
      this.rightEye.box,
      this.leftEye.path,
      this.rightEye.path,
    ]) {
      this.stopAll(layer);
    }
    this.attention.style.transform = "";
    this.heldAttention = 0;
    this.mood = mood;
    this.blinks = 0;
    this.dozed = false;
    this.apply(EXPRESSIONS[mood], false);
  }

  /** Takes every animation off, as the view that holds it goes away. */
  dispose() {
    this.idles = false;
    this.stopBlinking();
    this.running.forEach((keys) => keys.forEach((animation) => animation.cancel()));
    this.running.clear();
  }

  // MARK: Attention

  /** Whether a stroll is moving it, with eyes for the way ahead. */
  get isVisiting() {
    return !!this.running.get(this.root)?.has("visit") && mediaTime() < this.visitEnds;
  }

  get isDozing() {
    return this.dozed;
  }

  /** Whether a visit under way has it standing at `x`, where ending the visit leaves it without a jump. */
  standsAt(x: number) {
    const transform = new DOMMatrixReadOnly(this.current(this.root, "transform"));
    return Math.abs(transform.m41 + this.size / 2 - x) < 0.5;
  }

  /** Ends a visit under way where it stands, and it blinks again. */
  endVisit() {
    for (const layer of [
      this.root,
      this.squasher,
      this.faceLayer,
      this.turner,
      ...this.bodyFrames,
      ...this.itemFrames,
    ]) {
      this.stop(layer, "visit");
    }
    this.stop(this.root, "visitFade");
    this.blinksResume = mediaTime();
    this.stopBlinking();
    this.syncBlinking();
  }

  /** Lets go of the last frame a visit that ended out of sight held. */
  clearFinishedVisit() {
    if (!this.running.get(this.root)?.has("visit") || mediaTime() < this.visitEnds) return;
    for (const layer of [
      this.root,
      this.squasher,
      this.faceLayer,
      this.turner,
      ...this.bodyFrames,
      ...this.itemFrames,
    ]) {
      this.stop(layer, "visit");
    }
    this.stop(this.root, "visitFade");
  }

  /**
   * Turns its eyes toward `target`, in shares of its size, on top of the face it
   * keeps. With `hold` they stay that long and come back; without it, they stay.
   * Each turn starts where the eyes are, never where the last one meant to go.
   */
  attend(target: { x: number; y: number }, hold?: number) {
    if (this.size <= 0 || this.reduceMotion) return;
    this.attended = mediaTime();
    const from = this.current(this.attention, "transform");
    const to = this.gazeTransform(target);
    if (hold !== undefined) {
      const move = 0.16;
      const back = 0.34;
      const total = move + Math.max(0, hold) + back;
      this.attention.style.transform = "";
      this.play(
        this.attention,
        [
          { transform: from, offset: 0, easing: SETTLE },
          { transform: to, offset: move / total, easing: "linear" },
          { transform: to, offset: (total - back) / total, easing: EASE_IN_OUT },
          { transform: "translate(0px, 0px)", offset: 1 },
        ],
        { key: "attention", duration: total },
      );
      // The head turns toward it with the eyes, and back.
      this.heldAttention = 0;
      const own = EXPRESSIONS[this.mood].gaze.x;
      const toward = yawForGaze(own + target.x);
      const ahead = yawForGaze(own);
      this.turnHead(
        [this.yaw, toward, toward, ahead],
        total,
        [0, move / total, (total - back) / total, 1],
        undefined,
        "turnAttention",
      );
    } else {
      this.attention.style.transform = to;
      this.play(this.attention, [{ transform: from }, { transform: to }], {
        key: "attention",
        duration: 0.2,
        easing: SETTLE,
      });
      this.heldAttention = target.x;
      this.turnHead([yawForGaze(EXPRESSIONS[this.mood].gaze.x + target.x)], 0.2);
    }
  }

  /** Its eyes come back to the face it keeps, after `delay`. */
  lookAhead(delay = 0) {
    if (this.size <= 0) return;
    const from = this.current(this.attention, "transform");
    this.attention.style.transform = "";
    this.heldAttention = 0;
    const rest = this.restingYaw();
    if (this.reduceMotion || from === "none") {
      this.stop(this.attention, "attention");
      this.showYaw(rest);
      return;
    }
    if (rest !== this.yaw) {
      const turnFrom = this.yaw;
      const wait = Math.max(0, delay) + 0.34;
      this.showYaw(rest);
      this.turnHead([turnFrom, turnFrom, rest], wait, [0, Math.max(0, delay) / wait, 1], undefined, "turnAttention");
    }
    const total = Math.max(0, delay) + 0.34;
    this.play(
      this.attention,
      [
        { transform: from, offset: 0, easing: "linear" },
        { transform: from, offset: Math.max(0, delay) / total, easing: EASE_IN_OUT },
        { transform: "translate(0px, 0px)", offset: 1 },
      ],
      { key: "attention", duration: total },
    );
  }

  // MARK: Cues

  cue(cue: MascotCue) {
    if (this.size <= 0) return;
    if (this.reduceMotion) {
      if (cue === "celebrate") this.flashFace("happy", 1);
      return;
    }
    if (cue === "celebrate") {
      this.hop(this.size * 0.28);
      this.flashFace("happy", 1);
    } else {
      const s = this.size;
      this.play(
        this.faceLayer,
        [
          { translate: "0px", offset: 0 },
          { translate: `${s * 0.07}px`, offset: 0.3 },
          { translate: `${s * -0.06}px`, offset: 0.7 },
          { translate: "0px", offset: 1 },
        ],
        { key: "glance", duration: 0.42, easing: EASE_IN_OUT },
      );
    }
  }

  /** Positive flattens it on the ground, negative stretches it up, and its bottom stays where it was. */
  private squashed(amount: number) {
    const scaleX = 1 + amount * 0.8;
    const scaleY = 1 - amount;
    return `translate(0px, ${this.size * 0.42 * (1 - scaleY)}px) scale(${scaleX}, ${scaleY})`;
  }

  private squashFrames(values: number[], offsets: number[]): Keyframe[] {
    return values.map((value, index) => ({ transform: this.squashed(value), offset: offsets[index]! }));
  }

  private hop(height: number) {
    this.play(
      this.hopper,
      [
        { translate: "0px 0px", offset: 0, easing: "ease-out" },
        { translate: `0px ${-height}px`, offset: 0.32, easing: "ease-in" },
        { translate: "0px 0px", offset: 0.6, easing: "ease-out" },
        { translate: `0px ${-height * 0.3}px`, offset: 0.78, easing: "ease-in" },
        { translate: "0px 0px", offset: 1 },
      ],
      { key: "hop", duration: 0.62 },
    );
    this.play(this.squasher, this.squashFrames([0, -0.12, 0, 0.14, 0, 0.06, 0], [0, 0.12, 0.3, 0.62, 0.72, 0.84, 1]), {
      key: "squash",
      duration: 0.62,
    });
  }

  private pop() {
    this.play(this.squasher, this.squashFrames([0, -0.14, 0.06, 0], [0, 0.35, 0.7, 1]), { key: "pop", duration: 0.34 });
  }

  private wobble() {
    this.play(
      this.tilter,
      [
        { rotate: "0rad", offset: 0 },
        { rotate: "0.12rad", offset: 0.25 },
        { rotate: "-0.1rad", offset: 0.5 },
        { rotate: "0.06rad", offset: 0.75 },
        { rotate: "0rad", offset: 1 },
      ],
      { key: "wobble", duration: 0.5 },
    );
  }

  private heartbeat(begin?: number) {
    for (const eye of [this.leftEye, this.rightEye]) {
      this.play(
        eye.box,
        [
          { scale: "1", offset: 0 },
          { scale: "1.3", offset: 0.2 },
          { scale: "1", offset: 0.45 },
          { scale: "1.22", offset: 0.65 },
          { scale: "1", offset: 1 },
        ],
        { key: "heartbeat", duration: 0.7, begin },
      );
    }
  }

  /** A slow stretch up and back, as a yawn or waking up. */
  private stretch(begin: number, duration: number) {
    const frames = this.squashFrames([0, -0.12, -0.12, 0.05, 0], [0, 0.35, 0.6, 0.82, 1]);
    ["ease-out", "linear", "ease-in", "ease-out"].forEach((easing, index) => {
      frames[index]!.easing = easing;
    });
    this.play(this.squasher, frames, { key: "stretch", duration, begin });
  }

  private eyesScale(key: string, values: string[], offsets: number[], duration: number) {
    for (const eye of [this.leftEye, this.rightEye]) {
      this.play(
        eye.box,
        values.map((scale, index) => ({ scale, offset: offsets[index]! })),
        { key, duration },
      );
    }
  }

  // MARK: Reactions

  /**
   * A short reaction to something the island saw happen, over the face it keeps.
   * `lift` is how high a hop may take it where it stands. With reduced motion it
   * only changes face.
   */
  react(reaction: MascotReaction, lift: number) {
    if (this.size <= 0) return;
    // Whatever it is, it wakes up for it.
    if (this.dozed && reaction !== "wakeUp") this.setMood("idle", !this.reduceMotion);
    this.blinks = 0;
    const now = mediaTime();
    if (this.reduceMotion) {
      if (reaction === "celebrate" || reaction === "wakeUp") this.flashFace("happy", 1);
      else if (reaction === "love") this.flashFace("love", 1.2);
      else if (reaction === "surprised" || reaction === "flash") this.flashFace("surprised", 0.8);
      else if (reaction === "confused") this.flashFace("confused", 1.1);
      else if (reaction === "yawn" || reaction === "hush") this.flashFace("sleepy", 1.1);
      else if (reaction === "perk" || reaction === "bounce") this.flashFace("alert", 1);
      else if (reaction === "ready") this.flashFace("determined", 1);
      else this.flashFace("happy", 1.4);
      return;
    }
    switch (reaction) {
      case "celebrate":
        this.hop(lift);
        this.flashFace("happy", 1);
        break;
      case "love":
        this.flashFace("love", 1.4);
        this.heartbeat(now + 0.22);
        break;
      case "surprised":
        this.flashFace("surprised", 0.9);
        this.hop(lift * 0.7);
        break;
      case "flash":
        // Squeezed shut, then wide open, as a camera's flash goes off.
        this.eyesScale("flashBlink", ["1 1", "1 0.06", "1 0.06", "1 1.18", "1 1"], [0, 0.18, 0.42, 0.7, 1], 0.45);
        this.pop();
        this.pauseBlinking(0.5);
        break;
      case "confused":
        this.flashFace("confused", 1.2);
        this.wobble();
        break;
      case "wakeUp":
        // Heavy eyes, a stretch, and a glad face.
        if (this.dozed) this.setMood("idle", false);
        this.flashFace("sleepy", 0.7);
        this.stretch(now + 0.35, 0.6);
        this.flashFace("happy", 0.9, now + 0.7, "flashAfter");
        break;
      case "yawn":
        this.flashFace("sleepy", 1.2);
        this.stretch(now + 0.1, 0.9);
        break;
      case "perk":
        this.flashFace("alert", 1);
        this.hop(lift * 0.55);
        break;
      case "bounce": {
        // Time to go: two quick hops, the second lower, and a small rebound,
        // squashing as it lands and stretching to leave again.
        this.flashFace("alert", REACTION_LENGTH.bounce);
        const height = Math.max(1, lift * 0.6);
        const heights = [0, -height, 0, -height * 0.7, 0, -height * 0.15, 0];
        const times = [0, 0.14, 0.32, 0.46, 0.63, 0.75, 0.88];
        this.play(
          this.hopper,
          [
            ...heights.map((y, index): Keyframe => ({
              translate: `0px ${y}px`,
              offset: times[index]!,
              easing: index % 2 === 0 ? "ease-out" : "ease-in",
            })),
            { translate: "0px 0px", offset: 1 },
          ],
          { key: "hop", duration: 0.95 },
        );
        this.play(
          this.squasher,
          [
            ...this.squashFrames(
              [0, -0.12, 0, 0.14, -0.1, 0, 0.12, 0, 0.04, 0],
              [0, 0.04, 0.14, 0.32, 0.36, 0.46, 0.63, 0.7, 0.8, 0.88],
            ),
            { transform: this.squashed(0), offset: 1 },
          ],
          { key: "squash", duration: 0.95 },
        );
        break;
      }
      case "hush":
        // Eyes shut for a moment as it ducks, then back.
        this.eyesScale("hush", ["1 1", "1 0.1", "1 0.1", "1 1"], [0, 0.22, 0.78, 1], 0.95);
        this.play(this.squasher, this.squashFrames([0, 0.1, 0.1, -0.03, 0], [0, 0.25, 0.7, 0.88, 1]), {
          key: "duck",
          duration: 0.95,
        });
        this.pauseBlinking(1);
        break;
      case "ready":
        this.flashFace("determined", 1);
        // A nod: down a little, up past where it was, and settled.
        this.play(this.squasher, this.squashFrames([0, 0.1, -0.06, 0], [0, 0.3, 0.65, 1]), {
          key: "nod",
          duration: 0.5,
          begin: now + 0.12,
        });
        break;
      case "groove": {
        // It hears the music: smiling eyes, a bob on every beat and a lean one
        // way and the other, landing a little squashed.
        const length = REACTION_LENGTH.groove;
        this.flashFace("happy", length);
        const beats = 4;
        const height = Math.max(1, lift * 0.45);
        const bob: Keyframe[] = [];
        const sway: Keyframe[] = [];
        const land: Keyframe[] = [];
        for (let step = 0; step <= beats * 2; step++) {
          const offset = step / (beats * 2);
          const up = step % 2 === 1;
          bob.push({ translate: `0px ${up ? -height : 0}px`, offset, easing: step % 2 === 0 ? "ease-out" : "ease-in" });
          sway.push({ rotate: `${up ? (step % 4 === 1 ? 0.11 : -0.11) : 0}rad`, offset });
          land.push({ transform: this.squashed(up ? -0.05 : step === 0 || step === beats * 2 ? 0 : 0.07), offset });
        }
        this.play(this.hopper, bob, { key: "hop", duration: length });
        this.play(this.tilter, sway, { key: "wobble", duration: length, easing: EASE_IN_OUT });
        this.play(this.squasher, land, { key: "squash", duration: length });
        break;
      }
    }
  }

  /** Shows another face for a moment and comes back to its own, without changing the face it keeps. */
  private flashFace(mood: MascotMood, duration: number, begin?: number, key = "flash") {
    const expression = EXPRESSIONS[mood];
    const own = EXPRESSIONS[this.mood];
    for (const [eye, shape, base, right] of [
      [this.leftEye, expression.left, own.left, false],
      [this.rightEye, expression.right, own.right, true],
    ] as const) {
      const back = `path("${this.eyeD(base, right)}")`;
      const to = `path("${this.eyeD(shape, right)}")`;
      // One shown now leaves from the eyes on screen rather than its own face for a frame.
      const from = begin === undefined ? this.pathValue(eye.path) : back;
      const step = this.reduceMotion ? "step-end" : "linear";
      this.play(
        eye.path,
        [
          { d: from, offset: 0, easing: step },
          { d: to, offset: 0.15, easing: step },
          { d: to, offset: 0.85, easing: step },
          { d: back, offset: 1 },
        ],
        { key, duration, begin },
      );
    }
    this.pauseBlinking(duration, begin);
  }

  // MARK: Visits

  /**
   * Plays a stroll that began at `start`, from where it should be now. `baseline`
   * is the height of its centre in its parent. With reduced motion it does not
   * walk, and greets at `stand` instead. `holdsEnd` keeps it where the visit
   * ends, out of sight, until what comes next takes over.
   */
  playVisit(
    path: MascotPath,
    greeting: MascotMood,
    baseline: number,
    stand: { x: number; y: number },
    start: number,
    holdsEnd = false,
    standing: { x: number; y: number } | null = null,
  ) {
    if (this.size <= 0 || path.duration <= 0 || path.x.length !== path.keyTimes.length) return;
    const now = mediaTime();
    // One that already ended out of sight still holds its last frame.
    if (now >= start + path.duration && !(holdsEnd && !this.reduceMotion)) return;
    this.visitEnds = start + path.duration;
    const position = (x: number, y: number) => `translate(${x - this.size / 2}px, ${y - this.size / 2}px)`;
    const greet = path.greeting[1] - path.greeting[0];
    if (this.reduceMotion) {
      // No stroll: it fades in where it would greet, says hello and fades out.
      this.play(this.root, [{ transform: position(stand.x, stand.y) }, { transform: position(stand.x, stand.y) }], {
        key: "visit",
        duration: path.duration,
        begin: start,
      });
      if (!standing || standing.x !== stand.x || standing.y !== stand.y) {
        // Staying on from where it stood, it only fades out at the end.
        const inPlace = Math.abs((path.x[0] ?? Infinity) - stand.x) < 0.5;
        this.play(
          this.root,
          [
            { opacity: inPlace ? 1 : 0, offset: 0 },
            { opacity: 1, offset: 0.12 },
            { opacity: 1, offset: 0.88 },
            { opacity: 0, offset: 1 },
          ],
          { key: "visitFade", duration: path.duration, begin: start },
        );
      }
      if (greet > 0) this.flashFace(greeting, greet, start + path.greeting[0]);
      return;
    }
    const walk: Keyframe[] = path.x.map((x, index) => ({
      transform: position(x, baseline - path.lift[index]!),
      offset: path.keyTimes[index]!,
    }));
    this.play(this.root, walk, {
      key: "visit",
      duration: path.duration,
      begin: start,
      fill: holdsEnd ? "forwards" : "none",
    });
    this.play(
      this.squasher,
      path.squash.map((squash, index) => ({ transform: this.squashed(squash), offset: path.keyTimes[index]! })),
      { key: "visit", duration: path.duration, begin: start },
    );
    this.play(
      this.faceLayer,
      path.gaze.map((gaze, index) => ({ translate: `${gaze * this.size}px`, offset: path.keyTimes[index]! })),
      { key: "visit", duration: path.duration, begin: start },
    );
    // The head turns the way it walks.
    const resting = this.yaw;
    this.turnHead(
      path.gaze.map((gaze) => yawForGaze(gaze)),
      path.duration,
      path.keyTimes,
      start,
      "visit",
    );
    this.showYaw(resting);
    // Its eyes are for the way ahead while it walks.
    this.lookAhead();
    if (greet > 0) this.flashFace(greeting, greet, start + path.greeting[0]);
    // No blink while it walks, as the face it shows is the greeting's.
    this.pauseBlinking(start + path.duration - now);
  }

  // MARK: Blinking

  setIdles(idles: boolean) {
    if (idles === this.idles) return;
    this.idles = idles;
    if (!idles) {
      this.stopBlinking();
      if (this.dozed) this.setMood("idle", false);
    } else this.syncBlinking();
  }

  /** Wakes it from a long rest. A face shown sleepy on purpose stays. */
  wake(animated: boolean) {
    this.blinks = 0;
    if (this.dozed) this.setMood("idle", animated);
  }

  private syncBlinking() {
    if (!this.idles || !moodBlinks(this.mood) || this.size <= 0) {
      this.stopBlinking();
      return;
    }
    if (!this.blinkAnimation) this.scheduleBlink();
  }

  private stopBlinking() {
    if (this.blinkAnimation) this.blinkAnimation.onfinish = null;
    this.blinkAnimation = null;
    this.stop(this.leftEye.box, "blink");
    this.stop(this.rightEye.box, "blink");
    // A glance on its way goes with the blink it led to; one under way
    // finishes, so the eyes never jump.
    const glance = this.running.get(this.attention)?.get("idleGlance");
    if (glance && glance.effect?.getComputedTiming().progress == null) this.stop(this.attention, "idleGlance");
  }

  private pauseBlinking(duration: number, begin?: number) {
    this.blinksResume = Math.max(this.blinksResume, (begin ?? mediaTime()) + Math.max(0, duration));
    if (!this.idles) return;
    this.stopBlinking();
    this.syncBlinking();
  }

  /** One blink, a few seconds from now. It starts the next when it ends, so blinking costs nothing in between. */
  private scheduleBlink() {
    const now = mediaTime();
    const [low, high] = blinkInterval(this.lowPower);
    const delay = Math.max(0, this.blinksResume - now) + low + Math.random() * (high - low);
    const twice = Math.random() < 0.18;
    const values = twice ? ["1 1", "1 0.1", "1 1", "1 1", "1 0.1", "1 1"] : ["1 1", "1 0.1", "1 1"];
    const offsets = twice ? [0, 0.2, 0.4, 0.58, 0.78, 1] : [0, 0.45, 1];
    const begin = now + delay;
    // Reduced motion: the eyes close and open without a motion between.
    const easing = this.reduceMotion ? "step-end" : "linear";
    const frames = values.map((scale, index): Keyframe => ({ scale, offset: offsets[index]!, easing }));
    const duration = twice ? 0.44 : 0.17;
    // Now and then its eyes wander to one side, and the blink comes as they find
    // their way back. Not while they follow something.
    if (!twice && !this.reduceMotion && now - this.attended > 3 && Math.random() < GLANCE_CHANCE) {
      const away = (Math.random() < 0.5 ? 1 : -1) * this.size * 0.12;
      this.play(
        this.attention,
        [
          { translate: "0px", offset: 0, easing: SETTLE },
          { translate: `${away}px`, offset: 0.2, easing: "linear" },
          { translate: `${away}px`, offset: 0.8, easing: EASE_IN_OUT },
          { translate: "0px", offset: 1 },
        ],
        { key: "idleGlance", duration: 1.3, begin: begin - 1.1 },
      );
      const rest = this.yaw;
      const aside = yawForGaze(EXPRESSIONS[this.mood].gaze.x + away / Math.max(1, this.size));
      this.turnHead([rest, aside, aside, rest], 1.3, [0, 0.2, 0.8, 1], begin - 1.1, "turnGlance");
    }
    const blink = this.play(this.leftEye.box, frames, { key: "blink", duration, begin });
    this.play(this.rightEye.box, frames, { key: "blink", duration, begin });
    this.blinkAnimation = blink;
    blink.onfinish = () => this.blinkDidStop(blink);
  }

  private blinkDidStop(animation: Animation) {
    if (animation !== this.blinkAnimation || !this.idles) return;
    this.blinkAnimation = null;
    this.running.get(this.leftEye.box)?.delete("blink");
    this.running.get(this.rightEye.box)?.delete("blink");
    this.blinks++;
    // A long rest makes its eyes heavy, and it stops blinking then.
    if (this.mood === "idle" && this.blinks >= blinksBeforeSleep(new Date().getHours())) {
      this.setMood("sleepy", true);
      this.dozed = true;
      return;
    }
    this.syncBlinking();
  }
}
