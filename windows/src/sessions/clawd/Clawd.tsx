// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../../i18n/index";

// A Clawd on screen (NotchClawdView.swift, NotchClawdBuddy.swift; spec
// 02-sessions §5.4-5.6). Each one is a 24×22 canvas, one canvas pixel per
// sprite pixel, scaled up by CSS to a whole number of device pixels with
// `image-rendering: pixelated`, so the compositor does the nearest-neighbour
// scaling and a pose change is a 528-pixel putImageData. One shared clock
// steps every visible Clawd a dozen times a second and a canvas is touched
// only when its pose changes, which keeps ten of them in the island cheap.

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { bridge } from "../../shared/bridge";
import { costumeCode, seed as seedOf, type Character, type ClawdCostume } from "./costume";
import {
  HEIGHT,
  layout,
  pixels,
  pose as poseAt,
  poseKey,
  RATE,
  WIDTH,
  basePose,
  type ClawdMotion,
  type ClawdPose,
} from "./sprite";

// MARK: Shared clock

const listeners = new Set<() => void>();
let timer: ReturnType<typeof setTimeout> | undefined;
let lowPower = false;

const hidden = () => typeof document !== "undefined" && document.visibilityState === "hidden";

/** Milliseconds between the clock's steps. */
export const clockPeriod = () => 1000 / (lowPower ? RATE / 2 : RATE);

function schedule() {
  if (timer !== undefined || listeners.size === 0 || hidden()) return;
  const period = clockPeriod();
  timer = setTimeout(
    () => {
      timer = undefined;
      listeners.forEach((listener) => listener());
      schedule();
    },
    period - (performance.now() % period),
  );
}

if (typeof document !== "undefined") {
  // A hidden window stops the clock; on its return every Clawd catches up at once.
  document.addEventListener("visibilitychange", () => {
    if (hidden()) {
      clearTimeout(timer);
      timer = undefined;
    } else {
      listeners.forEach((listener) => listener());
      schedule();
    }
  });
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  schedule();
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      clearTimeout(timer);
      timer = undefined;
    }
  };
}

/** Energy saver (Battery Saver) halves the steps, as Low Power Mode on the Mac; poses keep their timing. */
export function setClawdLowPower(on: boolean) {
  if (lowPower === on) return;
  lowPower = on;
  clearTimeout(timer);
  timer = undefined;
  schedule();
}

// The window showing Clawds (the island) follows it. The event is heard
// before the first reading is asked for, and once one is heard the reading,
// which may answer later, is older and ignored. Without either, the Clawds
// keep the full rate.
let heard = false;
void bridge.power
  .onSaver((on) => {
    heard = true;
    setClawdLowPower(on);
  })
  .then(() => bridge.power.saver())
  .then((on) => on !== undefined && !heard && setClawdLowPower(on))
  .catch(() => {});

// MARK: Images

// Made once per costume and pose, as the Mac's 512-entry cache.
const images = new Map<string, ImageData>();

function imageFor(costume: ClawdCostume, pose: ClawdPose, key: string, character: Character): ImageData {
  let image = images.get(key);
  if (image) {
    // Most recently used goes to the end, so the oldest is evicted first.
    images.delete(key);
  } else {
    image = new ImageData(pixels(costume, pose, character), WIDTH, HEIGHT);
    if (images.size >= 512) images.delete(images.keys().next().value!);
  }
  images.set(key, image);
  return image;
}

// MARK: Environment

function useMediaQuery(query: string): boolean {
  const get = () => typeof matchMedia === "function" && matchMedia(query).matches;
  const [matches, setMatches] = useState(get);
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const list = matchMedia(query);
    const update = () => setMatches(list.matches);
    update();
    list.addEventListener("change", update);
    return () => list.removeEventListener("change", update);
  }, [query]);
  return matches;
}

export function useReducedMotion(): boolean {
  return useMediaQuery("(prefers-reduced-motion: reduce)");
}

/** The display's scale, followed when the window moves to another monitor. */
export function useDevicePixelRatio(): number {
  const [ratio, setRatio] = useState(() => (typeof window === "undefined" ? 1 : window.devicePixelRatio || 1));
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const list = matchMedia(`(resolution: ${ratio}dppx)`);
    const update = () => setRatio(window.devicePixelRatio || 1);
    list.addEventListener("change", update);
    return () => list.removeEventListener("change", update);
  }, [ratio]);
  return ratio;
}

const ms = (value: number | Date | null | undefined) =>
  value == null ? null : typeof value === "number" ? value : value.getTime();

// MARK: Components

/** The sprite canvas placed in its box: whole device pixels, frame centred, overflow shown. */
function Canvas(props: {
  width: number;
  height: number;
  ratio: number;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  className?: string;
  style?: CSSProperties;
}) {
  useLocale();
  const fit = layout({ width: props.width, height: props.height }, props.ratio);
  return (
    <div
      className={props.className}
      aria-hidden="true"
      style={{
        position: "relative",
        width: props.width,
        height: props.height,
        flex: "none",
        pointerEvents: "none",
        ...props.style,
      }}
    >
      <canvas
        ref={props.canvasRef}
        width={WIDTH}
        height={HEIGHT}
        style={{
          position: "absolute",
          left: fit.x,
          top: fit.y,
          width: WIDTH * fit.pixel,
          height: HEIGHT * fit.pixel,
          imageRendering: "pixelated",
        }}
      />
    </div>
  );
}

export interface ClawdProps {
  costume: ClawdCostume;
  /** Clawd for Claude Code (the default), the terminal robot for Codex. */
  character?: Character;
  /** What it does; see motionForPhase. Defaults to `rest`. */
  motion?: ClawdMotion;
  /** Its own rhythm: the costume key (session id or `sessionId/agentId`) or its FNV-1a seed. */
  seed?: string | bigint;
  /**
   * When what it does began (the session's `phaseSince`), so every Clawd of
   * one session moves as one. Without it the motion starts when it changes.
   */
  since?: number | Date | null;
  /** When the hop began: the finished phase's start, or the moment the costume changed. */
  cheer?: number | Date | null;
  /** False where there is no room above (notices): no "!", "?" or "z". */
  bubbles?: boolean;
  /** The box, in CSS px; see CLAWD_SIZES. */
  width: number;
  height: number;
  /** Forces one still pose; by default it follows prefers-reduced-motion. */
  still?: boolean;
  className?: string;
  style?: CSSProperties;
}

/** A Clawd fitted to a box, moving unless Reduce Motion is on. Decoration only: aria-hidden. */
export function Clawd(props: ClawdProps) {
  useLocale();
  const { costume, motion = "rest", bubbles = true, width, height, character = "clawd" } = props;
  const reduce = useReducedMotion();
  const still = props.still ?? reduce;
  const ratio = useDevicePixelRatio();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const seedValue = useMemo(
    () => (typeof props.seed === "bigint" ? props.seed : seedOf(props.seed ?? "")),
    [props.seed],
  );
  const sinceMs = ms(props.since);
  const cheerMs = ms(props.cheer);
  // Back-dated to `since`, or the moment the motion changed; an update that
  // changes nothing keeps it, so the motion never jumps back.
  const start = useMemo(() => sinceMs ?? Date.now(), [motion, sinceMs]);
  const code = costumeCode(costume);
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    const element = canvasRef.current?.parentElement;
    if (!element || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => setVisible(entries.some((e) => e.isIntersecting)));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const context = canvasRef.current?.getContext("2d");
    if (!context) return;
    let shown = "";
    const draw = () => {
      const now = Date.now();
      const next = poseAt(motion, (now - start) / 1000, seedValue, {
        cheer: cheerMs == null ? null : (now - cheerMs) / 1000,
        still,
        bubbles,
      });
      const key = `${character}|${code}|${poseKey(next)}`;
      if (key === shown) return;
      shown = key;
      context.putImageData(imageFor(costume, next, key, character), 0, 0);
    };
    draw();
    // A canvas the size of the sprite is redrawn after it remounts or its size changes.
    if (still || !visible) return;
    return subscribe(draw);
    // `costume` is captured through `code`, which names it.
  }, [code, character, motion, start, seedValue, cheerMs, still, bubbles, visible, width, height, ratio]);

  return (
    <Canvas
      width={width}
      height={height}
      ratio={ratio}
      canvasRef={canvasRef}
      className={props.className}
      style={props.style}
    />
  );
}

/** A Clawd standing still in one pose, for the wardrobe's tiles: dozens at once, none moving. */
export function ClawdPicture(props: {
  costume: ClawdCostume;
  character?: Character;
  pose?: ClawdPose;
  width: number;
  height: number;
  className?: string;
  style?: CSSProperties;
}) {
  useLocale();
  const ratio = useDevicePixelRatio();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const pose = props.pose ?? basePose();
  const character = props.character ?? "clawd";
  const key = `${character}|${costumeCode(props.costume)}|${poseKey(pose)}`;
  useEffect(() => {
    canvasRef.current?.getContext("2d")?.putImageData(imageFor(props.costume, pose, key, character), 0, 0);
  }, [key, props.width, props.height, ratio]);
  return (
    <Canvas
      width={props.width}
      height={props.height}
      ratio={ratio}
      canvasRef={canvasRef}
      className={props.className}
      style={props.style}
    />
  );
}
