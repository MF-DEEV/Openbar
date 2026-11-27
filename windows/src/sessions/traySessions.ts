// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The sessions' tray icon and panel, driven from the island (#331). The
// island's page holds the sessions as the person sees them, so it tells the
// icon what to show and draws its picture, pulses it for a new ask, answers
// what the icon's menu chooses the way its own buttons do, and keeps the
// island and the panel from showing the sessions at the same time.

import { onLocaleChange } from "../i18n";
import { useIsland } from "../island/store";
import { bridge, type SessionsPanelState } from "../shared/bridge";
import { onLinux } from "../shared/platform";
import { usePlacement } from "./placement";
import { presentedSnapshot } from "./presentation";
import { settled, useSessions } from "./store";
import { iconSize, renderTrayIcon } from "./trayIcon";
import { NewAsks, trayModel } from "./trayState";

let panel: SessionsPanelState = { visible: false, focused: null };

/** The session's page, or the list holding it, is open in the tray panel. */
export function panelShows(id: string): boolean {
  return panel.visible && (panel.focused === null || panel.focused === id);
}

export const panelVisible = () => panel.visible;

const reducedMotion = () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/** The icon pulses for a new ask, unless motion is reduced. */
export function pulseTray() {
  if (!usePlacement.getState().inTray || reducedMotion()) return;
  void bridge.tray.sessionsPulse();
}

/** Opens the panel by itself where the person allows it: only on the desktop or in Openbar (#167). */
export async function openPanelByItself(focus: string | null): Promise<boolean> {
  const { inIsland, inTray } = usePlacement.getState();
  if (inIsland || !inTray || !useSessions.getState().prefs.trayOpensPanel || panel.visible) return false;
  const allowed = await bridge.attention
    .current()
    .then((context) => context.canAutoPresent)
    .catch(() => false);
  if (!allowed) return false;
  void bridge.sessionsPanel.show(focus ? { session: focus, request: null } : null, true);
  return true;
}

export function followTraySessions(): () => void {
  const asks = new NewAsks();
  let toldModel = "";
  let drawn: string | null = null;
  let drawing = 0;

  const draw = async (waiting: number) => {
    const size = iconSize(onLinux(), window.devicePixelRatio);
    const key = `${waiting}:${size}`;
    if (key === drawn) return;
    drawn = key;
    const serial = ++drawing;
    const [icon, swollen] = await Promise.all([
      renderTrayIcon(waiting, false, size),
      waiting > 0 ? renderTrayIcon(waiting, true, size) : Promise.resolve(null),
    ]);
    if (serial !== drawing || icon.length === 0) return;
    void bridge.tray.sessionsIcon(icon, swollen && swollen.length ? swollen : null);
  };

  const sync = () => {
    const { inTray } = usePlacement.getState();
    const state = useSessions.getState();
    const model = trayModel(state, inTray);
    const key = JSON.stringify(model);
    if (key !== toldModel) {
      toldModel = key;
      void bridge.tray.sessions(model);
    }
    if (inTray) void draw(model.waiting);
    if (asks.arrived(presentedSnapshot(state).sessions)) pulseTray();
  };

  const offSessions = useSessions.subscribe((state, previous) => {
    if (
      state.snapshot !== previous.snapshot ||
      state.autoAllow !== previous.autoAllow ||
      state.names !== previous.names
    )
      sync();
  });
  const offPlacement = usePlacement.subscribe((state, previous) => {
    if (state.inTray !== previous.inTray) drawn = null;
    sync();
    // Leaving the tray closes its panel.
    if (!state.inTray && panel.visible) void bridge.sessionsPanel.hide();
  });
  const offLocale = onLocaleChange(sync);
  sync();

  // What the icon's menu chose: Allow and Deny as the queue's buttons answer,
  // once the ask has been on screen a moment like theirs.
  const offAction = bridge.tray.onSessionsAction((action) => {
    if (action.kind === "open") {
      void bridge.sessionsPanel.show(null, true);
      return;
    }
    if (action.kind === "openAsk") {
      void bridge.sessionsPanel.show({ session: action.session, request: action.request }, true);
      return;
    }
    const sessions = useSessions.getState();
    const session = sessions.snapshot.sessions.find((s) => s.id === action.session);
    const shown = session?.permission?.requestId === action.request ? session : undefined;
    const target =
      shown ??
      (session && session.laterAsks?.some((ask) => ask.requestId === action.request)
        ? { ...session, permission: session.laterAsks.find((ask) => ask.requestId === action.request) }
        : undefined);
    if (!target || !settled(sessions, target)) return;
    void sessions.decide(target, action.kind === "allow" ? "allow" : "deny");
  });

  // The panel tells what it shows; the island closes its own Sessions page as the panel opens.
  const offPanel = bridge.sessionsPanel.onState((state) => {
    const opened = state.visible && !panel.visible;
    panel = state;
    const island = useIsland.getState();
    if (opened && island.expanded && island.selected === "sessions") island.collapse();
  });
  const offVisible = bridge.sessionsPanel.onVisible((visible) => {
    panel = { ...panel, visible, focused: visible ? panel.focused : null };
  });
  // And the panel closes as the island opens on the sessions.
  const offIsland = useIsland.subscribe((state, previous) => {
    const shows = state.expanded && state.selected === "sessions" && !state.showingSections;
    const showed = previous.expanded && previous.selected === "sessions" && !previous.showingSections;
    if (shows && !showed && panel.visible) void bridge.sessionsPanel.hide();
  });

  return () => {
    offSessions();
    offPlacement();
    offLocale();
    offIsland();
    void offAction.then((f) => f());
    void offPanel.then((f) => f());
    void offVisible.then((f) => f());
  };
}
