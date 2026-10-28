// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's state machine (NotchService.swift; spec 01-shell §2). The
// store holds what the island presents; `presentation` turns that into the
// one thing drawn and its size, and the surface animates between them.

import type { LucideIcon } from "lucide-react";
import { create } from "zustand";
import {
  activitySize,
  activityPickerLayout,
  contentInsets,
  expandedHeight,
  expandedWidth,
  type Geometry,
  hoverPulse,
  makeGeometry,
  peekSize,
  restingSize,
} from "./geometry";
import type { Size } from "./motion";
import { type ModuleId, moduleInfo, visibleModules } from "./modules";
import { DEFAULT_SETTINGS, hoverExpands, type IslandSettings, opensOnHover, sectionEnabled } from "./settings";

export interface Notice {
  id: string;
  /** Same-kind notices replace each other without a new resize (value bursts). */
  kind: string;
  title: string;
  detail?: string;
  /** 3 levels · 2 timer, watch, sessions · 1 battery, notifications, agents · 0 clipboard, downloads, track. */
  priority: number;
  /** Seconds on screen. */
  duration: number;
  opens?: ModuleId;
  /** A picture in the symbol slot (a song's cover). */
  image?: string;
  /** A local source-app icon if Windows' supplied image cannot be decoded. */
  imageFallback?: string;
  /** Downloads: 0..1 for known totals; null for an unknown size. */
  progress?: number | null;
  audio?: { muted: boolean; deafened: boolean; level: number };
  /** A glyph in the symbol slot, and a colour for it and the title (the timer's orange). */
  icon?: LucideIcon;
  tint?: string;
}

/** Live activities in the Mac's priority order (NotchSupport.swift:1791-1799). */
export const ACTIVITY_ORDER = [
  "timer",
  "watch",
  "microphone",
  "downloads",
  "sessions",
  "agents",
  "calendar",
  "music",
  "keepAwake",
] as const;
export type ActivityId = (typeof ACTIVITY_ORDER)[number];

export interface ActivitySpec {
  urgent?: boolean;
  /** Width each wing needs beside the camera, before the section's clamp. */
  wing: number;
  /** The capsule's one-row content width. */
  capsuleContent?: number;
  /** Clicking the strip opens this section. */
  opens: ModuleId;
}

export interface IslandState {
  autoPresentAllowed: boolean;
  attentionFullscreen: boolean;
  settings: IslandSettings;
  screen: Size;
  devicePixelRatio: number;

  expanded: boolean;
  openedByHover: boolean;
  clickedSinceOpening: boolean;
  pinned: boolean;
  peeking: boolean;
  showingSections: boolean;
  sectionQuery: string;
  selected: ModuleId;
  sessionsEnlarged: boolean;
  /** The pointer is over the island. */
  inside: boolean;
  hoverEmphasized: boolean;
  /** Closed with the pointer inside: hover cannot open again until it leaves. */
  hoverSuppressed: boolean;
  notice: Notice | null;
  noticeExpanded: boolean;
  activities: Partial<Record<ActivityId, ActivitySpec>>;
  /** The activity the person picked from the picker, while it lasts. */
  pickedActivity: ActivityId | null;
  activityPickerOpen: boolean;
  /** Bumped by each open so the window asks for keyboard focus. */
  focusRequest: number;
  /** Pages with a height of their own (music, timer, controls) report it here. */
  pageContent: Partial<Record<ModuleId, number>>;
  setPageContent(module: ModuleId, height: number | null): void;
  /** A drag is over the closed island: it shows where to drop (notchDragReveal). */
  dropHint: boolean;
  setDropHint(on: boolean): void;
  /** A dialog of the island's is up: a click outside must not close it (keepFileInteractionOpen). */
  held: boolean;
  setHeld(held: boolean): void;

  setScreen(screen: Size, devicePixelRatio: number): void;
  open(module?: ModuleId, opts?: { takeFocus?: boolean; sections?: boolean; byHover?: boolean }): void;
  collapse(): void;
  toggle(): void;
  clickClosed(): void;
  activateActivity(id: ActivityId): boolean;
  selectCompactActivity(id: ActivityId): void;
  dismissActivityPicker(): void;
  pointerEnter(): void;
  pointerExit(): void;
  select(module: ModuleId): void;
  toggleSections(): void;
  setSectionQuery(query: string): void;
  stepBack(): void;
  setPinned(pinned: boolean): void;
  showNotice(notice: Notice): void;
  dismissNotice(id?: string): void;
  setActivity(id: ActivityId, spec: ActivitySpec | null): void;
}

let hoverTimer: ReturnType<typeof setTimeout> | undefined;
let exitTimer: ReturnType<typeof setTimeout> | undefined;
let noticeTimer: ReturnType<typeof setTimeout> | undefined;

function clearHoverTimers() {
  clearTimeout(hoverTimer);
  clearTimeout(exitTimer);
  hoverTimer = exitTimer = undefined;
}

export function modulesOf(state: Pick<IslandState, "settings">): ModuleId[] {
  const s = state.settings;
  return visibleModules(s.moduleOrder, s.hiddenModules, (id) => sectionEnabled(s, id));
}

export const useIsland = create<IslandState>()((set, get) => ({
  autoPresentAllowed: false,
  attentionFullscreen: false,
  settings: DEFAULT_SETTINGS,
  screen: { width: 1920, height: 1080 },
  devicePixelRatio: 1,

  expanded: false,
  openedByHover: false,
  clickedSinceOpening: false,
  pinned: false,
  peeking: false,
  showingSections: false,
  sectionQuery: "",
  selected: "sessions",
  sessionsEnlarged: false,
  inside: false,
  hoverEmphasized: false,
  hoverSuppressed: false,
  notice: null,
  noticeExpanded: false,
  activities: {},
  pickedActivity: null,
  activityPickerOpen: false,
  focusRequest: 0,
  pageContent: {},
  dropHint: false,
  held: false,

  setPageContent: (module, height) =>
    set((s) => {
      if (s.pageContent[module] === (height ?? undefined)) return s;
      const pageContent = { ...s.pageContent };
      if (height === null) delete pageContent[module];
      else pageContent[module] = height;
      return { pageContent };
    }),

  setScreen: (screen, devicePixelRatio) => set({ screen, devicePixelRatio }),

  setDropHint: (dropHint) => set({ dropHint }),
  setHeld: (held) => set({ held }),

  open(module, opts = {}) {
    const s = get();
    if (!s.settings.enabled) return;
    clearHoverTimers();
    const modules = modulesOf(s);
    let selected = module ?? s.selected;
    if (!module && !s.expanded && s.settings.returnHome && s.settings.homeModule !== "explore") {
      selected = s.settings.homeModule;
    }
    if (!modules.includes(selected)) selected = modules[0] ?? "controls";
    const sections =
      opts.sections ?? (!module && !s.expanded && s.settings.returnHome && s.settings.homeModule === "explore");
    set({
      expanded: true,
      activityPickerOpen: false,
      peeking: false,
      selected,
      showingSections: sections,
      sectionQuery: sections ? s.sectionQuery : "",
      openedByHover: s.expanded ? s.openedByHover : (opts.byHover ?? false),
      clickedSinceOpening: s.expanded ? s.clickedSinceOpening : false,
      hoverEmphasized: false,
      dropHint: false,
      notice: null,
      noticeExpanded: false,
      sessionsEnlarged: selected === "sessions" ? s.sessionsEnlarged : false,
      focusRequest: opts.takeFocus === false ? s.focusRequest : s.focusRequest + 1,
    });
  },

  collapse() {
    clearHoverTimers();
    const s = get();
    set({
      expanded: false,
      activityPickerOpen: false,
      peeking: false,
      pinned: false,
      showingSections: false,
      sectionQuery: "",
      sessionsEnlarged: false,
      noticeExpanded: false,
      hoverEmphasized: false,
      hoverSuppressed: s.inside,
    });
  },

  toggle() {
    if (get().expanded) get().collapse();
    else get().open();
  },

  clickClosed() {
    const s = get();
    if (s.expanded) {
      set({ clickedSinceOpening: true });
      return;
    }
    if (s.notice) {
      const opens = s.notice.opens;
      s.dismissNotice();
      get().open(opens);
      return;
    }
    const choices = compactActivities(s);
    if (choices.length > 1) {
      get().activateActivity(s.pickedActivity && choices.includes(s.pickedActivity) ? s.pickedActivity : choices[0]!);
      return;
    }
    get().open();
  },

  activateActivity(id) {
    const s = get();
    if (!s.settings.enabled) return false;
    const available = compactActivities(s);
    if (available.length > 1 && !s.expanded) {
      const chosen = available.includes(id) ? id : available[0]!;
      if (!s.activityPickerOpen) {
        clearHoverTimers();
        set({
          activityPickerOpen: true,
          pickedActivity: chosen,
          peeking: false,
          hoverEmphasized: false,
          hoverSuppressed: false,
          focusRequest: s.focusRequest + 1,
        });
        return false;
      }
      const current = s.pickedActivity && available.includes(s.pickedActivity) ? s.pickedActivity : chosen;
      get().open(s.activities[current]?.opens);
      return true;
    }
    get().open(s.activities[id]?.opens);
    return true;
  },

  selectCompactActivity(id) {
    const s = get();
    if (!compactActivities(s).includes(id)) return;
    clearHoverTimers();
    set({
      pickedActivity: id,
      activityPickerOpen: true,
      peeking: false,
      hoverEmphasized: false,
      hoverSuppressed: false,
      focusRequest: s.focusRequest + 1,
    });
  },

  dismissActivityPicker() {
    clearHoverTimers();
    set((s) => ({ activityPickerOpen: false, hoverSuppressed: s.inside }));
  },

  pointerEnter() {
    const s = get();
    clearTimeout(exitTimer);
    exitTimer = undefined;
    if (s.inside) return;
    set({ inside: true });
    if (s.expanded || s.pinned) return;
    if (s.hoverSuppressed) return;
    if (s.attentionFullscreen) return;
    if (showsActivityPicker(get())) return;
    if (!s.notice && compactActivities(get()).length > 1) {
      set({ hoverEmphasized: true });
      return;
    }
    if (!s.autoPresentAllowed && visibleActivity(s) === "sessions") return;
    // Download details are opened explicitly by clicking the compact strip.
    if (!s.notice && visibleActivity(s) === "downloads") return;
    if (!opensOnHover(s.settings)) {
      // Click to open: the closed island only answers with a small pulse.
      if (!s.notice) set({ hoverEmphasized: true });
      return;
    }
    if (s.notice && s.settings.opening !== "hiddenUntilHover") return;
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      const now = get();
      if (!now.inside || now.expanded || now.dropHint) return;
      if (showsActivityPicker(now)) return;
      if (!now.notice && compactActivities(now).length > 1) {
        set({ hoverEmphasized: true });
        return;
      }
      if (now.attentionFullscreen || (!now.autoPresentAllowed && visibleActivity(now) === "sessions")) return;
      if (!now.notice && visibleActivity(now) === "downloads") return;
      if (hoverExpands(now.settings)) now.open(undefined, { takeFocus: false, byHover: true });
      else set({ peeking: true });
    }, s.settings.hoverDelay * 1000);
  },

  pointerExit() {
    const s = get();
    clearTimeout(hoverTimer);
    hoverTimer = undefined;
    set({ inside: false, activityPickerOpen: false, hoverEmphasized: false, hoverSuppressed: false });
    const closesOnExit = s.peeking || (s.expanded && s.openedByHover && !s.clickedSinceOpening && !s.pinned);
    if (!closesOnExit) return;
    const delay = s.expanded ? s.settings.closeDelay : 0.12;
    clearTimeout(exitTimer);
    exitTimer = setTimeout(() => {
      const now = get();
      if (now.inside) return;
      if (now.peeking) set({ peeking: false });
      else now.collapse();
    }, delay * 1000);
  },

  select(module) {
    set({ selected: module, showingSections: false, sectionQuery: "", sessionsEnlarged: false });
  },

  toggleSections() {
    const s = get();
    if (!s.expanded) s.open(undefined, { sections: true });
    else set({ showingSections: !s.showingSections, sectionQuery: "" });
  },

  setSectionQuery: (sectionQuery) => set({ sectionQuery }),

  stepBack() {
    const s = get();
    if (s.showingSections) set({ showingSections: false, sectionQuery: "" });
    else if (s.sessionsEnlarged) set({ sessionsEnlarged: false });
    else s.collapse();
  },

  setPinned: (pinned) => set({ pinned }),

  showNotice(incoming) {
    const s = get();
    if (s.notice && incoming.priority < s.notice.priority) return;
    if (s.noticeExpanded && s.notice && incoming.kind !== s.notice.kind && incoming.priority <= s.notice.priority)
      return;
    set({ notice: incoming });
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => get().dismissNotice(incoming.id), incoming.duration * 1000);
  },

  setActivity(id, spec) {
    set((s) => {
      const activities = { ...s.activities };
      if (spec) activities[id] = spec;
      else delete activities[id];
      const left = Object.keys(activities).length > 0;
      const available = compactActivities({ ...s, activities });
      return {
        activities,
        pickedActivity: left && s.pickedActivity && activities[s.pickedActivity] ? s.pickedActivity : null,
        activityPickerOpen: available.length > 1 && s.activityPickerOpen,
      };
    });
  },

  dismissNotice(id) {
    const s = get();
    if (!s.notice || (id && s.notice.id !== id)) return;
    clearTimeout(noticeTimer);
    set({ notice: null, noticeExpanded: false });
  },
}));

// MARK: Presentation

export type PresentationKind = "resting" | "activity" | "notice" | "peek" | "dropHint" | "expanded" | "picker";

/** Microphone status is an indicator, rather than a competing activity on the Mac. */
export function compactActivities(s: Pick<IslandState, "activities" | "settings">): ActivityId[] {
  const modules = modulesOf(s);
  return ACTIVITY_ORDER.filter(
    (id) => id !== "microphone" && s.activities[id] && modules.includes(s.activities[id]!.opens),
  );
}

export function showsActivityPicker(s: IslandState): boolean {
  return (
    !s.expanded &&
    !s.peeking &&
    !s.notice &&
    !s.attentionFullscreen &&
    !s.hoverSuppressed &&
    s.activityPickerOpen &&
    compactActivities(s).length > 1
  );
}

/** The picked activity if it still runs, else the first in priority order. */
export function visibleActivity(s: Pick<IslandState, "activities" | "pickedActivity">): ActivityId | null {
  if (s.pickedActivity && s.activities[s.pickedActivity]) return s.pickedActivity;
  if (s.activities.sessions?.urgent && !s.activities.timer && !s.activities.watch) return "sessions";
  return ACTIVITY_ORDER.find((id) => s.activities[id]) ?? null;
}

export interface Presentation {
  kind: PresentationKind;
  /** Changes whenever different content is drawn. */
  key: string;
  size: Size;
  hasRow: boolean;
}

export function geometryOf(s: Pick<IslandState, "settings" | "screen" | "devicePixelRatio">): Geometry {
  return makeGeometry({
    screen: s.screen,
    devicePixelRatio: s.devicePixelRatio,
    silhouette: s.settings.silhouette,
    layout: s.settings.layout,
    customWidth: s.settings.customWidth,
    customHeight: s.settings.customHeight,
    capsuleFit: s.settings.capsuleFit,
    notchWidth: s.settings.notchWidth,
    notchHeight: s.settings.notchHeight,
    roundness: s.settings.roundness,
    shoulderWidth: s.settings.shoulderWidth,
    shoulderHeight: s.settings.shoulderHeight,
  });
}

/** What the island draws, first match wins (NotchView.surface, spec §2.2). */
export function presentation(s: IslandState, geometry: Geometry, measuredNotice?: Size): Presentation {
  if (s.expanded) {
    const enlarged = s.selected === "sessions" && s.sessionsEnlarged && !s.showingSections;
    const info = moduleInfo(s.selected);
    const hasRow = s.showingSections || info.pageTop === "row";
    const bottomBubbles = s.settings.quickAccess.some((b) => b.side === "bottom");
    const natural = s.showingSections ? undefined : info.naturalWidth;
    const width = expandedWidth(geometry, { natural, enlarged });
    const height = s.showingSections
      ? expandedHeight(geometry, {
          hasRow: true,
          content: galleryHeight(
            modulesOf(s).length,
            width - 2 * (contentInsets(geometry, { width, height: 328 }).horizontal - 28),
          ),
          bottomBubbles,
        })
      : expandedHeight(geometry, { hasRow, enlarged, bottomBubbles, content: s.pageContent[s.selected] });
    return {
      kind: "expanded",
      key: s.showingSections ? "expanded:sections" : `expanded:${s.selected}${enlarged ? ":large" : ""}`,
      size: { width, height },
      hasRow,
    };
  }
  // A drag over the closed island wins over everything closed (NotchView.surface).
  if (s.dropHint) {
    const peek = peekSize(geometry);
    return {
      kind: "dropHint",
      key: "dropHint",
      size: { width: peek.width, height: geometry.safeContentTop + 66 },
      hasRow: false,
    };
  }
  if (s.notice) {
    const rest = restingSize(geometry);
    const size = measuredNotice ?? { width: Math.max(rest.width, 260), height: rest.height };
    return { kind: "notice", key: `notice:${s.notice.kind}`, size, hasRow: false };
  }
  if (s.peeking) return { kind: "peek", key: "peek", size: peekSize(geometry), hasRow: false };
  if (showsActivityPicker(s)) {
    const choices = compactActivities(s);
    const strips = choices.map((id) =>
      activitySize(geometry, s.activities[id]!.wing, s.activities[id]!.capsuleContent),
    );
    return { kind: "picker", key: "picker", size: activityPickerLayout(geometry, strips).size, hasRow: false };
  }
  const activity = visibleActivity(s);
  if (activity) {
    const spec = s.activities[activity]!;
    const size = activitySize(geometry, spec.wing, spec.capsuleContent);
    return {
      kind: "activity",
      key: `activity:${activity}`,
      size: s.hoverEmphasized ? hoverPulse(geometry, size) : size,
      hasRow: false,
    };
  }
  const rest = restingSize(geometry);
  return {
    kind: "resting",
    key: "resting",
    size: s.hoverEmphasized ? hoverPulse(geometry, rest) : rest,
    hasRow: false,
  };
}

/** Tiles 86 tall with 8 between rows, at most three rows in view (spec §5). */
export function galleryColumns(width: number): number {
  const content = width - 56 - 12;
  return Math.max(1, Math.floor((content + 8) / (92 + 8)));
}

/** Only the rows that fit the 320 pt budget count; more rows page (NotchGeometry.sectionRows). */
export function galleryHeight(count: number, width: number): number {
  const rows = Math.min(Math.ceil(count / galleryColumns(width)), Math.floor((320 + 8) / (86 + 8)));
  return Math.min(320, rows * 86 + (rows - 1) * 8);
}

export type ContentTransition = "reveal" | "dismiss" | "replace" | "depart" | "none";

/** The content transition between two presentations (spec §3.2). */
export function transitionBetween(from: Presentation, to: Presentation): ContentTransition {
  if (from.key === to.key) return "none";
  if (to.kind === "expanded") return from.kind === "expanded" ? "replace" : "reveal";
  if (from.kind === "expanded") return "dismiss";
  if (from.kind === "notice") return to.kind === "notice" ? "replace" : "depart";
  if (to.kind === "notice" || to.kind === "peek" || to.kind === "dropHint") return "reveal";
  if (from.kind === "peek" || from.kind === "dropHint") return "dismiss";
  return "replace";
}
