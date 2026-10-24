// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's state machine (NotchService.swift; spec 01-shell §2). The
// store holds what the island presents; `presentation` turns that into the
// one thing drawn and its size, and the surface animates between them.

import { create } from "zustand";
import {
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
}

export interface IslandState {
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
  /** Bumped by each open so the window asks for keyboard focus. */
  focusRequest: number;

  setScreen(screen: Size, devicePixelRatio: number): void;
  setSettings(patch: Partial<IslandSettings>): void;
  open(module?: ModuleId, opts?: { takeFocus?: boolean; sections?: boolean; byHover?: boolean }): void;
  collapse(): void;
  toggle(): void;
  clickClosed(): void;
  pointerEnter(): void;
  pointerExit(): void;
  select(module: ModuleId): void;
  toggleSections(): void;
  setSectionQuery(query: string): void;
  stepBack(): void;
  setPinned(pinned: boolean): void;
  showNotice(notice: Notice): void;
  dismissNotice(id?: string): void;
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
  selected: "controls",
  sessionsEnlarged: false,
  inside: false,
  hoverEmphasized: false,
  hoverSuppressed: false,
  notice: null,
  noticeExpanded: false,
  focusRequest: 0,

  setScreen: (screen, devicePixelRatio) => set({ screen, devicePixelRatio }),
  setSettings: (patch) => set((s) => ({ settings: { ...s.settings, ...patch } })),

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
      peeking: false,
      selected,
      showingSections: sections,
      sectionQuery: sections ? s.sectionQuery : "",
      openedByHover: s.expanded ? s.openedByHover : (opts.byHover ?? false),
      clickedSinceOpening: s.expanded ? s.clickedSinceOpening : false,
      hoverEmphasized: false,
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
    get().open();
  },

  pointerEnter() {
    const s = get();
    clearTimeout(exitTimer);
    exitTimer = undefined;
    if (s.inside) return;
    set({ inside: true });
    if (s.expanded || s.pinned) return;
    if (s.hoverSuppressed) return;
    if (!opensOnHover(s.settings)) {
      // Click to open: the closed island only answers with a small pulse.
      if (!s.notice) set({ hoverEmphasized: true });
      return;
    }
    if (s.notice && s.settings.opening !== "hiddenUntilHover") return;
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      const now = get();
      if (!now.inside || now.expanded) return;
      if (hoverExpands(now.settings)) now.open(undefined, { takeFocus: false, byHover: true });
      else set({ peeking: true });
    }, s.settings.hoverDelay * 1000);
  },

  pointerExit() {
    const s = get();
    clearTimeout(hoverTimer);
    hoverTimer = undefined;
    set({ inside: false, hoverEmphasized: false, hoverSuppressed: false });
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

  dismissNotice(id) {
    const s = get();
    if (!s.notice || (id && s.notice.id !== id)) return;
    clearTimeout(noticeTimer);
    set({ notice: null, noticeExpanded: false });
  },
}));

// MARK: Presentation

export type PresentationKind = "resting" | "notice" | "peek" | "expanded";

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
      ? expandedHeight(geometry, { hasRow: true, content: galleryHeight(modulesOf(s).length, width), bottomBubbles })
      : expandedHeight(geometry, { hasRow, enlarged, bottomBubbles });
    return {
      kind: "expanded",
      key: s.showingSections ? "expanded:sections" : `expanded:${s.selected}${enlarged ? ":large" : ""}`,
      size: { width, height },
      hasRow,
    };
  }
  if (s.notice) {
    const rest = restingSize(geometry);
    const size = measuredNotice ?? { width: Math.max(rest.width, 260), height: rest.height };
    return { kind: "notice", key: `notice:${s.notice.kind}`, size, hasRow: false };
  }
  if (s.peeking) return { kind: "peek", key: "peek", size: peekSize(geometry), hasRow: false };
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
  if (to.kind === "notice" || to.kind === "peek") return "reveal";
  if (from.kind === "peek") return "dismiss";
  return "replace";
}
