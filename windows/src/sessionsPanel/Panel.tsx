// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The sessions' tray panel (#331): the island's Sessions page and its top row
// on the island's black surface, in a window of its own by the taskbar's
// corner. The island's state is set as if it were open on Sessions while the
// panel shows, so the page's cards, keys and pictures work unchanged.

import { useEffect, useRef, useState } from "react";
import { useLocale } from "../i18n";
import { closeTopLayer } from "../island/pageLayer";
import { useIsland } from "../island/store";
import { sessionsPage } from "../sessions/SessionsPage";
import { islandSettings, applyPlacement } from "../sessions/placement";
import { startSharedState } from "../sessions/sharedState";
import { subscribeSessions, useSessions } from "../sessions/store";
import { S } from "../sessions/strings";
import { useVoice } from "../sessions/voice";
import { bridge, inTauri } from "../shared/bridge";
import { loadPrefs, onPrefs, type Prefs } from "../shared/prefs";

/** The page inside, as the island lays out a page with a top row. */
export const PANEL_INSET = 18;
export const PANEL_TOP = 12;
export const PANEL_ROW = 36;
export const PANEL_GAP = 12;
export const PANEL_BOTTOM = 16;
/** The surface's hairline edge, inside the window. */
export const PANEL_EDGE = 1;
export const PANEL_CHROME = PANEL_TOP + PANEL_ROW + PANEL_GAP + PANEL_BOTTOM + PANEL_EDGE * 2;
/** A row's outline reaches a little past the box it is measured by. */
const LIST_SLACK = 6;
/** What the empty list's message needs, and the least a list is given. */
const EMPTY_LIST = 150;
const SHORTEST_LIST = 96;

/**
 * How tall the list stands on its own: from its top to the bottom of its last
 * row, margins and padding included, whatever room the window gives it now
 * (scrolled or not).
 */
export function naturalListHeight(list: HTMLElement): number {
  const children = [...list.children].filter((child) => (child as HTMLElement).offsetParent !== null);
  const last = children[children.length - 1];
  const style = getComputedStyle(list);
  const paddingTop = parseFloat(style.paddingTop) || 0;
  const paddingBottom = parseFloat(style.paddingBottom) || 0;
  if (!last) return paddingTop + paddingBottom;
  const margin = parseFloat(getComputedStyle(last).marginBottom) || 0;
  const top = list.getBoundingClientRect().top;
  return last.getBoundingClientRect().bottom - top + list.scrollTop + margin + paddingBottom;
}

/** The window's height around a list this tall. */
export function fittedHeight(list: number | null): number {
  return Math.ceil(PANEL_CHROME + Math.max(SHORTEST_LIST, list === null ? EMPTY_LIST : list + LIST_SLACK));
}

/** The panel's view of the island: open on Sessions while it shows. */
function applyPrefs(prefs: Prefs) {
  applyPlacement(prefs);
  const settings = islandSettings(prefs);
  useIsland.setState({
    settings: { ...settings, enabled: true, sections: { ...settings.sections, sessions: true } },
  });
  useSessions.setState({ prefs: prefs.sessions });
}

/** What runs as the window starts, before anything is drawn. */
export function startPanel() {
  useIsland.setState({ selected: "sessions", expanded: false, showingSections: false });
  void loadPrefs().then(applyPrefs);
  void onPrefs(applyPrefs);
  subscribeSessions(() => {});
  startSharedState("panel");
  const showing = (visible: boolean) => {
    useIsland.setState({ expanded: visible, selected: "sessions", showingSections: false });
    if (!visible) {
      // Opened again, it starts on the list, as the island does.
      useSessions.getState().focus(null);
      useSessions.setState({ composing: false });
      useIsland.setState({ sessionsEnlarged: false });
      void useVoice.getState().cancel();
    }
  };
  void bridge.sessionsPanel.onVisible(showing);
  void bridge.sessionsPanel.onFocus((focus) => {
    if (focus) useSessions.getState().focus(focus.session, focus.request);
  });
  // The window's first showing went by while this page loaded.
  void bridge.sessionsPanel.current().then((current) => {
    if (!current) return;
    showing(current.visible);
    if (current.visible && current.focus) useSessions.getState().focus(current.focus.session, current.focus.request);
  });
  // The island hears what the panel shows: its sounds stay quiet for a
  // session on screen here, and it closes its own Sessions page.
  let told = "";
  const report = () => {
    const state = { visible: useIsland.getState().expanded, focused: useSessions.getState().focused };
    const key = JSON.stringify(state);
    if (key === told) return;
    told = key;
    void bridge.sessionsPanel.report(state);
  };
  useIsland.subscribe(report);
  useSessions.subscribe(report);
  // A picture being picked keeps the panel open while the dialog has the keyboard.
  useIsland.subscribe((state, previous) => {
    if (state.pinned !== previous.pinned) void bridge.sessionsPanel.setPinned(state.pinned);
    if (state.sessionsEnlarged !== previous.sessionsEnlarged)
      void bridge.sessionsPanel.setEnlarged(state.sessionsEnlarged);
  });
  window.addEventListener("keydown", (event) => {
    if (event.defaultPrevented || event.isComposing || event.key !== "Escape") return;
    // Escape steps back as in the island: a spoken reply, an inner layer,
    // the larger page, then the panel itself.
    event.preventDefault();
    if (useVoice.getState().cancel()) return;
    if (closeTopLayer()) return;
    if (useIsland.getState().sessionsEnlarged) {
      useIsland.setState({ sessionsEnlarged: false });
      return;
    }
    void bridge.sessionsPanel.hide();
  });
}

function useWindowSize() {
  const [size, setSize] = useState(() => ({ width: window.innerWidth, height: window.innerHeight }));
  useEffect(() => {
    const resize = () => setSize({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  return size;
}

export function Panel() {
  useLocale();
  const { width, height } = useWindowSize();
  const page = useRef<HTMLDivElement>(null);
  const fillsPanel = useSessions((s) => s.focused !== null || s.composing || s.away !== null);
  const visible = useIsland((s) => s.expanded);

  // A short list: the window fits it rather than hanging empty below it.
  useEffect(() => {
    if (!visible && inTauri) return;
    if (fillsPanel) {
      void bridge.sessionsPanel.fit(null);
      return;
    }
    let last = -1;
    const measure = () => {
      const list = page.current?.querySelector<HTMLElement>(".s-list");
      const wanted = fittedHeight(list ? naturalListHeight(list) : null);
      if (wanted === last) return;
      last = wanted;
      void bridge.sessionsPanel.fit(wanted);
    };
    measure();
    const timer = setInterval(measure, 250);
    return () => clearInterval(timer);
  }, [fillsPanel, visible]);

  const contentWidth = Math.max(0, width - PANEL_INSET * 2);
  const contentHeight = Math.max(0, height - PANEL_CHROME);
  const { Page, Leading, Trailing } = sessionsPage;
  return (
    <div className="sp-surface" role="dialog" aria-label={S.trayName}>
      <div className="sp-row">
        <div className="n-row-leading">{Leading ? <Leading /> : null}</div>
        <div className="n-row-trailing">{Trailing ? <Trailing /> : null}</div>
      </div>
      <div className="sp-page n-page" ref={page} style={{ width: contentWidth, height: contentHeight }}>
        <Page width={contentWidth} height={contentHeight} />
      </div>
    </div>
  );
}
