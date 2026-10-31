// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the island does with the companion: when it rests, visits, hides and
// reacts (the Companion part of NotchService.swift; spec 05 §12). One timer is
// set up at a time, minutes ahead, and the strolls themselves are the
// browser's to draw.

import { create } from "zustand";
import { useBattery } from "../island/battery";
import { activitySize, type Geometry, restingWingWidth } from "../island/geometry";
import { visibleIdleContent } from "../island/idle";
import type { IslandSettings } from "../island/settings";
import { geometryOf, type IslandState, useIsland, visibleActivity } from "../island/store";
import { useMusic } from "../music/store";
import {
  AGENT_START_INTERVAL,
  GROOVE_DELAY,
  GROOVE_INTERVAL,
  HIDE_DELAY,
  HIDE_RETRY,
  type MascotLook,
  type MascotMood,
  type MascotReaction,
  type MascotReactionEvent,
  nextVisitDelay,
  randomGreeting,
  ReactionGate,
  type MascotSide,
  WELCOME_DELAY,
} from "./model";
import {
  activityTrack,
  CAMEO_ARRIVAL,
  cameoHold,
  endsOutOfSight,
  handBack,
  type MascotVisit,
  type VisitKind,
  visitDuration,
  visitReaction,
} from "./motion";

/** The island is on and the companion switched on. */
export const mascotEnabled = (s: IslandSettings) => s.enabled && s.mascotEnabled;
export const mascotLook = (s: IslandSettings): MascotLook => ({
  style: s.mascotStyle,
  shape: s.mascotShape,
  palette: s.mascotPalette,
});

export interface MascotState {
  /** Switched on, as the island last synced it. */
  on: boolean;
  /** The side it rests on, as last synced, so a change moves it only together with the stroll across. */
  side: MascotSide;
  visit: MascotVisit | null;
  /** What it walks over steps aside while it visits. */
  stepsAside: boolean;
  /** It went into the island after a quiet while, and comes out only to visit or react. */
  tucked: boolean;
  reaction: MascotReactionEvent | null;
  /** Keep Awake holds the PC up: it stays wide awake. */
  keepAwake: boolean;
  /** Energy saver: no strolls of its own, and it blinks half as often. */
  lowPower: boolean;
}

export const useMascot = create<MascotState>(() => ({
  on: false,
  side: "left",
  visit: null,
  stepsAside: false,
  tucked: false,
  reaction: null,
  keepAwake: false,
  lowPower: false,
}));

const now = () => performance.now() / 1000;
const reducedMotion = () => globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

// MARK: What the island shows

function idleContent(s: IslandState) {
  return visibleIdleContent(
    s.settings,
    useBattery.getState().battery,
    useMusic.getState().playback?.isPlaying ?? false,
  );
}

/** It rests in the closed island when nothing else is there, unless it hides in the island. */
export function mascotAtRest(s: IslandState, m: MascotState = useMascot.getState()): boolean {
  return m.on && idleContent(s) === "none" && !m.tucked;
}

/** It rests in the closed island now, rather than only visiting it. */
function restsInView(s: IslandState, m: MascotState = useMascot.getState()): boolean {
  return mascotAtRest(s, m) && visibleActivity(s) === null;
}

/** Whether the closed island draws it: resting, or strolling through, where its wings fit. */
export function mascotShows(s: IslandState, geometry: Geometry, m: MascotState = useMascot.getState()): boolean {
  return (restsInView(s, m) || m.visit !== null) && (geometry.floatingGap !== null || restingWingWidth(geometry) > 0);
}

/** The face it keeps at rest: wide awake while Keep Awake holds the PC up. */
export const restingMood = (m: Pick<MascotState, "keepAwake">): MascotMood => (m.keepAwake ? "alert" : "idle");

/** A visit needs the closed island on screen with room for it, whatever it shows. */
function canHostVisit(s: IslandState): boolean {
  const hiddenUntilHover = s.settings.opening === "hiddenUntilHover" && !s.inside;
  const hiddenInFullscreen = s.attentionFullscreen && s.settings.hideInFullscreen;
  return (
    s.settings.enabled &&
    !hiddenUntilHover &&
    !hiddenInFullscreen &&
    !s.expanded &&
    !s.peeking &&
    !s.dropHint &&
    !s.notice &&
    !s.activityPickerOpen &&
    hasRoom(s)
  );
}

function hasRoom(s: IslandState): boolean {
  const geometry = geometryOf(s);
  const activity = visibleActivity(s);
  if (!activity) return geometry.floatingGap !== null || restingWingWidth(geometry) > 0;
  const spec = s.activities[activity]!;
  const width = activitySize(geometry, spec.wing, spec.capsuleContent).width;
  return activityTrack(geometry, width, useMascot.getState().side) !== null;
}

// MARK: Timers

let nextVisitTimer: ReturnType<typeof setTimeout> | undefined;
let visitTimer: ReturnType<typeof setTimeout> | undefined;
let stepBackTimer: ReturnType<typeof setTimeout> | undefined;
let tuckTimer: ReturnType<typeof setTimeout> | undefined;
let flushTimer: ReturnType<typeof setTimeout> | undefined;
let grooveTimer: ReturnType<typeof setTimeout> | undefined;

let nextId = 1;
let visitsWereOn = false;
let frequencyAtSync: IslandSettings["mascotVisitFrequency"] | null = null;
let hidesAtSync: boolean | null = null;
let enabledAtSync: boolean | null = null;
let sideSynced = false;
let settingsAtSync: IslandSettings | null = null;
let stirred = -Infinity;
let restedInView = false;
let leftRest = -Infinity;
let backAtRest = -Infinity;
let gate = new ReactionGate();
let pending: { reaction: MascotReaction; deadline: number; notBefore: number } | null = null;
let lastGroove = -Infinity;
let lastAgentStart = -Infinity;
let sawAgents: boolean | null = null;
let sawKeepAwake: boolean | null = null;

const settings = () => useIsland.getState().settings;
const visitsOn = (s: IslandSettings) => mascotEnabled(s) && s.mascotVisits;
export const reactsOn = (s: IslandSettings) => mascotEnabled(s) && s.mascotReactions;
const hidesOn = (s: IslandSettings) => mascotEnabled(s) && s.mascotHidesWhenIdle;
const floats = () => useIsland.getState().settings.silhouette === "capsule";

function makeVisit(kind: VisitKind, greeting: MascotMood, start = now()): MascotVisit {
  return { id: nextId++, kind, greeting, start };
}

function endVisitAfter(seconds: number) {
  clearTimeout(visitTimer);
  visitTimer = setTimeout(endVisit, seconds * 1000);
}

function setVisit(visit: MascotVisit | null) {
  clearTimeout(stepBackTimer);
  stepBackTimer = undefined;
  useMascot.setState({ visit, stepsAside: visit !== null });
  stirred = now();
  // A cameo hands the strip back on its way home, timed from the visit's own start.
  const back = visit && handBack(visit.kind, floats());
  if (visit && back !== null && back !== undefined) {
    stepBackTimer = setTimeout(
      () => {
        stepBackTimer = undefined;
        useMascot.setState({ stepsAside: false });
      },
      Math.max(0, visit.start + back - now()) * 1000,
    );
  }
}

// MARK: Visits

function scheduleVisit(after: number, greeting?: MascotMood) {
  clearTimeout(nextVisitTimer);
  nextVisitTimer = setTimeout(() => beginVisit(greeting ?? null, false), after * 1000);
}

/** `asked` is a hello Settings asked for, which comes whatever the visits. */
function beginVisit(greeting: MascotMood | null, asked: boolean) {
  nextVisitTimer = undefined;
  const s = useIsland.getState();
  if (!asked && !visitsOn(s.settings)) return;
  const m = useMascot.getState();
  const later = () => scheduleVisit(nextVisitDelay(s.settings.mascotVisitFrequency));
  // Busy, hidden or out of room: it tries again on its next visit.
  if (!canHostVisit(s) || m.visit) return later();
  // Energy saver keeps it at rest, and a pending visit simply comes later.
  if (!asked && m.lowPower) return later();
  // Without motion a stroll over an activity hides its strip for seconds: it
  // waits for the island to rest, and a hello asked for plays in its own wing.
  const coversNothing = restsInView(s) || (m.tucked && idleContent(s) === "none" && visibleActivity(s) === null);
  if (reducedMotion() && !coversNothing) {
    if (asked) beginCameo("celebrate");
    else later();
    return;
  }
  const visit = makeVisit({ type: restsInView(s) ? "lap" : "pass" }, greeting ?? randomGreeting());
  setVisit(visit);
  endVisitAfter(visitDuration(visit.kind));
}

function endVisit() {
  clearTimeout(visitTimer);
  visitTimer = undefined;
  const ended = useMascot.getState().visit;
  if (!ended) return;
  const s = useIsland.getState();
  // Gone behind the camera from a reaction over an activity that left
  // meanwhile, it comes back out to its place rather than appear there.
  if (
    visitReaction(ended.kind) &&
    restsInView(s) &&
    now() >= ended.start + visitDuration(ended.kind) - 0.05 &&
    !reducedMotion()
  ) {
    const back = makeVisit({ type: "arrive" }, "idle", Math.max(ended.start + visitDuration(ended.kind), now() - 0.1));
    setVisit(back);
    endVisitAfter(visitDuration(back.kind));
    return;
  }
  // Switched off while it strolled to its place: it says goodbye from there rather than vanish.
  if (
    s.settings.enabled &&
    !mascotEnabled(s.settings) &&
    !endsOutOfSight(ended.kind) &&
    ended.kind.type !== "farewell" &&
    !reducedMotion() &&
    idleContent(s) === "none" &&
    visibleActivity(s) === null &&
    canHostVisit(s)
  ) {
    const goodbye = makeVisit({ type: "farewell" }, "happy");
    setVisit(goodbye);
    endVisitAfter(visitDuration(goodbye.kind));
    return;
  }
  setVisit(null);
  // A side chosen while it was out takes effect now.
  if (useMascot.getState().side !== s.settings.mascotSide) {
    syncSide();
    if (useMascot.getState().visit) return;
  }
  if (visitsOn(s.settings)) scheduleVisit(nextVisitDelay(s.settings.mascotVisitFrequency));
}

/** Back at rest while it reacted over an activity that has gone: it stays once its reaction is over. */
function returnedToRest() {
  const visit = useMascot.getState().visit;
  const reaction = visit && visitReaction(visit.kind);
  if (!visit || !reaction) return;
  const landed = visit.start + (visit.kind.type === "linger" ? 0 : CAMEO_ARRIVAL);
  // Already on its way behind the camera, it comes back out once gone.
  if (now() >= landed + cameoHold(reaction) - 0.05) return;
  clearTimeout(visitTimer);
  visitTimer = setTimeout(
    () => {
      if (useMascot.getState().visit?.id !== visit.id) return;
      if (restsInView(useIsland.getState())) return endVisit();
      // Something took its place again: it leaves as it came for.
      endVisitAfter(Math.max(0, visit.start + visitDuration(visit.kind) - now()));
    },
    Math.max(0, landed + 0.05 - now()) * 1000,
  );
}

/** Hops out as the wings open when switched on; turned off, a glad hop and away, and the wings fold once it's gone. */
function stageEntrance(arriving: boolean) {
  const s = useIsland.getState();
  const m = useMascot.getState();
  if (
    reducedMotion() ||
    idleContent(s) !== "none" ||
    visibleActivity(s) !== null ||
    (!arriving && m.tucked) ||
    !canHostVisit(s) ||
    (arriving ? m.visit !== null && m.visit.kind.type !== "farewell" : m.visit !== null)
  )
    return;
  const visit = makeVisit({ type: arriving ? "arrive" : "farewell" }, arriving ? "wink" : "happy");
  clearTimeout(stepBackTimer);
  stepBackTimer = undefined;
  useMascot.setState({ visit, stepsAside: false });
  stirred = now();
  endVisitAfter(visitDuration(visit.kind));
}

function syncVisits() {
  const s = settings();
  const on = visitsOn(s);
  const frequency = s.mascotVisitFrequency;
  const wereOn = visitsWereOn;
  const before = frequencyAtSync;
  visitsWereOn = on;
  frequencyAtSync = frequency;
  if (!on) {
    clearTimeout(nextVisitTimer);
    nextVisitTimer = undefined;
    // Visits turned off only stop coming: what plays now plays out. A stopped island ends everything.
    const visit = useMascot.getState().visit;
    if (visit && visit.kind.type !== "farewell" && !s.enabled) endVisit();
    return;
  }
  // Turned on just now: it says hello almost at once. Coming out as it's switched on is that hello already.
  if (!wereOn) {
    if (useMascot.getState().visit?.kind.type !== "arrive") scheduleVisit(WELCOME_DELAY, "wink");
  } else if (!useMascot.getState().visit && (nextVisitTimer === undefined || before !== frequency)) {
    // A new pace takes effect now, not after the visit already planned.
    scheduleVisit(nextVisitDelay(frequency));
  }
}

function syncSide() {
  const side = settings().mascotSide;
  const m = useMascot.getState();
  const previous = sideSynced ? m.side : null;
  // A visit under way finishes on the side it set out on; the side chosen since follows once it's over.
  if (previous !== null && m.visit) return;
  sideSynced = true;
  if (previous === side) return;
  useMascot.setState({ side });
  // Across, behind the camera, where it rests closed.
  const s = useIsland.getState();
  if (previous === null || floats() || reducedMotion() || !restsInView(s) || !canHostVisit(s)) return;
  const cross = makeVisit({ type: "cross" }, "wink");
  useMascot.setState({ visit: cross });
  endVisitAfter(visitDuration(cross.kind) + 0.1);
}

// MARK: Hiding when idle

/** True when that changes what the closed island holds. */
function syncHiding(switchedOn: boolean) {
  const hides = hidesOn(settings());
  const previous = hidesAtSync;
  hidesAtSync = hides;
  const m = useMascot.getState();
  if (previous === null) {
    useMascot.setState({ tucked: hides });
  } else if (switchedOn) {
    useMascot.setState({ tucked: false });
  } else if (previous !== hides && m.on) {
    if (!hides) untuck();
    else if (!stageTuck()) scheduleTuck(HIDE_RETRY, true);
  }
  if (!hides) {
    clearTimeout(tuckTimer);
    tuckTimer = undefined;
  } else if (!useMascot.getState().tucked && tuckTimer === undefined) {
    // Out of the island, a quiet while is always counting.
    scheduleTuck(HIDE_DELAY);
  }
}

/** `soon` hides it as soon as it's free, without waiting for quiet. */
function scheduleTuck(after: number, soon = false) {
  clearTimeout(tuckTimer);
  tuckTimer = setTimeout(() => tuckIfQuiet(soon), after * 1000);
}

function tuckIfQuiet(soon: boolean) {
  tuckTimer = undefined;
  const s = useIsland.getState();
  if (!s.settings.enabled || hidesAtSync !== true || useMascot.getState().tucked) return;
  const quiet = now() - stirred;
  if (!soon && quiet < HIDE_DELAY - 0.05) return scheduleTuck(HIDE_DELAY - quiet);
  // The pointer on it looks at it or pets it, and it stays meanwhile.
  if (!soon && restsInView(s) && s.inside) return scheduleTuck(HIDE_RETRY);
  if (!stageTuck()) scheduleTuck(HIDE_RETRY, soon);
}

/** A yawn and into the island where it shows at rest; anywhere else it's simply in there when the island rests again. */
function stageTuck(): boolean {
  const reacting = pending ? now() <= pending.deadline : false;
  const m = useMascot.getState();
  if (m.visit || reacting) return false;
  const s = useIsland.getState();
  const shown = restsInView(s) && canHostVisit(s);
  useMascot.setState({ tucked: true });
  clearTimeout(tuckTimer);
  tuckTimer = undefined;
  if (!shown || reducedMotion()) return true;
  const tuck = makeVisit({ type: "linger", reaction: "yawn" }, "idle");
  setVisit(tuck);
  endVisitAfter(visitDuration(tuck.kind));
  return true;
}

function untuck() {
  if (!useMascot.getState().tucked) return;
  useMascot.setState({ tucked: false });
  if (!useIsland.getState().expanded) stageEntrance(true);
}

// MARK: Reactions

/** It comes out for what happens. `patience` is how long it may wait for the island to show it. */
export function reactMascot(reaction: MascotReaction, patience = ReactionGate.PATIENCE, after = 0) {
  const s = useIsland.getState();
  if (!reactsOn(s.settings)) return;
  // A notice on screen that shows the companion plays it there already.
  if (s.notice?.mascot === reaction && !s.expanded) {
    gate.admits(reaction, now());
    return;
  }
  const at = now();
  pending = { reaction, deadline: at + after + patience, notBefore: at + after };
  flush();
}

function flush() {
  const m = useMascot.getState();
  const s = useIsland.getState();
  // A countdown it watches ends once the timer's strip is no longer the one the island shows.
  if (m.visit && m.visit.kind.type === "countdown" && visibleActivity(s) !== "timer") endMascotCountdown(false);
  if (!pending) return;
  const at = now();
  if (at > pending.deadline || !reactsOn(s.settings)) {
    pending = null;
    return;
  }
  if (at < pending.notBefore) {
    clearTimeout(flushTimer);
    flushTimer = setTimeout(flush, (pending.notBefore - at) * 1000);
    return;
  }
  if (useMascot.getState().visit || !canHostVisit(s)) return;
  const resting = restsInView(s);
  if (resting) {
    // Back at rest as the activity leaves, the reaction waits until the island draws it there.
    if (!restedInView) return;
    if (at < backAtRest + 0.2) {
      pending = { ...pending, deadline: Math.max(pending.deadline, backAtRest + 0.7), notBefore: backAtRest + 0.2 };
      flush();
      return;
    }
  }
  const { reaction } = pending;
  pending = null;
  if (!gate.admits(reaction, at)) return;
  // Its wait for the next song counts from a groove it played, not one that never came.
  if (reaction === "groove") lastGroove = at;
  if (resting) {
    stirred = at;
    useMascot.setState({ reaction: { id: nextId++, reaction, start: at } });
  } else {
    beginCameo(reaction, at - leftRest < 0.35 && visibleActivity(s) !== null);
  }
}

function beginCameo(reaction: MascotReaction, lingering = false) {
  const cameo = makeVisit({ type: lingering ? "linger" : "cameo", reaction }, "idle");
  setVisit(cameo);
  // Its last frame puts it behind the camera, and what it covered comes back.
  endVisitAfter(visitDuration(cameo.kind));
}

/** The last seconds of a countdown the closed island shows: it comes out to watch. A capsule has no camera. */
export function watchMascotCountdown(remaining: number) {
  const s = useIsland.getState();
  if (
    !reactsOn(s.settings) ||
    remaining <= 1 ||
    useMascot.getState().visit ||
    visibleActivity(s) !== "timer" ||
    floats() ||
    !canHostVisit(s)
  )
    return;
  const watch = makeVisit({ type: "countdown", total: remaining }, "idle");
  setVisit(watch);
  endVisitAfter(visitDuration(watch.kind));
}

/** The countdown it watches ran out, stopped or went away. Paused, with its strip still there, it goes back. */
export function endMascotCountdown(retreating: boolean) {
  const visit = useMascot.getState().visit;
  if (visit?.kind.type !== "countdown") return;
  if (!retreating || visibleActivity(useIsland.getState()) !== "timer" || floats()) return endVisit();
  const retreat = makeVisit({ type: "retreat" }, "idle");
  setVisit(retreat);
  endVisitAfter(visitDuration(retreat.kind));
}

/** Music started: once the song's strip has settled, it comes out to bob along, at most once in a while. */
export function mascotHearsMusic() {
  if (!reactsOn(settings()) || now() - lastGroove <= GROOVE_INTERVAL) return;
  clearTimeout(grooveTimer);
  grooveTimer = setTimeout(() => {
    if (useMusic.getState().playback?.isPlaying) reactMascot("groove", 3);
  }, GROOVE_DELAY * 1000);
}

/** An AI agent getting to work where it rests: it stays with a ready face as the strip takes its place. */
export function mascotSeesAgents(working: boolean) {
  const saw = sawAgents;
  sawAgents = working;
  if (saw !== false || !working || !restedInView || !reactsOn(settings())) return;
  const at = now();
  if (at - lastAgentStart <= AGENT_START_INTERVAL) return;
  lastAgentStart = at;
  reactMascot("ready", 1);
}

export function setMascotKeepAwake(active: boolean) {
  const saw = sawKeepAwake;
  sawKeepAwake = active;
  useMascot.setState({ keepAwake: active });
  // The value it starts with is no news.
  if (saw === null || saw === active) return;
  reactMascot(active ? "perk" : "yawn");
}

export function setMascotLowPower(lowPower: boolean) {
  useMascot.setState({ lowPower });
}

/** Settings asks it to say hello now. False when the island has no room for it at the moment. */
export function greetMascot(): boolean {
  const s = useIsland.getState();
  if (!mascotEnabled(s.settings) || useMascot.getState().visit || !canHostVisit(s)) return false;
  clearTimeout(nextVisitTimer);
  beginVisit("wink", true);
  return useMascot.getState().visit !== null;
}

// MARK: Syncing

/** Picks the settings up, as the Mac's syncWithPreferences. */
function syncSettings() {
  const s = settings();
  const enabled = mascotEnabled(s);
  const toggled = enabledAtSync !== null && enabledAtSync !== enabled;
  enabledAtSync = enabled;
  useMascot.setState({ on: enabled });
  syncHiding(toggled && enabled);
  if (toggled && !useIsland.getState().expanded) stageEntrance(enabled);
  syncVisits();
  syncSide();
}

/** Whatever the island shows changed: is it back at rest, and can a waiting reaction play? */
function refresh() {
  const s = useIsland.getState();
  if (s.settings !== settingsAtSync) {
    settingsAtSync = s.settings;
    syncSettings();
  }
  const resting = restsInView(s) && !s.expanded && !(s.settings.opening === "hiddenUntilHover" && !s.inside);
  if (restedInView !== resting) {
    restedInView = resting;
    if (resting) {
      backAtRest = now();
      stirred = backAtRest;
      returnedToRest();
    } else {
      leftRest = now();
    }
  }
  flush();
}

/** Starts following the island. */
export function startMascot() {
  useIsland.subscribe(refresh);
  useMusic.subscribe(refresh);
  useBattery.subscribe(refresh);
  useMascot.subscribe((state, previous) => {
    if (state.tucked !== previous.tucked || state.visit !== previous.visit) refresh();
  });
  refresh();
}

/** For tests: back to a fresh island. */
export function resetMascot() {
  for (const timer of [nextVisitTimer, visitTimer, stepBackTimer, tuckTimer, flushTimer, grooveTimer]) {
    clearTimeout(timer);
  }
  nextVisitTimer = visitTimer = stepBackTimer = tuckTimer = flushTimer = grooveTimer = undefined;
  visitsWereOn = false;
  frequencyAtSync = null;
  hidesAtSync = null;
  enabledAtSync = null;
  sideSynced = false;
  settingsAtSync = null;
  stirred = -Infinity;
  restedInView = false;
  leftRest = -Infinity;
  backAtRest = -Infinity;
  gate = new ReactionGate();
  pending = null;
  lastGroove = -Infinity;
  lastAgentStart = -Infinity;
  sawAgents = null;
  sawKeepAwake = null;
  useMascot.setState({
    on: false,
    side: "left",
    visit: null,
    stepsAside: false,
    tucked: false,
    reaction: null,
    keepAwake: false,
    lowPower: false,
  });
}

/** For tests: runs what a change in the island would. */
export const refreshMascot = refresh;
