// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t } from "../i18n";

import { Bell, BellOff, ChevronUp, Download, LayoutGrid, Pin, PinOff, Settings } from "lucide-react";
import { useEffect, useRef, useState, type MouseEvent } from "react";

import { bridge } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import { useSessions } from "../sessions/store";
import { muteUntil, soundsMuted } from "../sessions/soundPreferences";
import { soundText, useSoundMute } from "../settings/SessionSounds";
import { Menu, type MenuAnchor, type MenuEntry } from "../shared/ui/Menu";
import { openPrompt } from "../updates/open";
import { useUpdates } from "../updates/store";
import { modulesOf, useIsland } from "./store";

/** Matches NotchView.islandMenu: Sections opens the gallery, rather than a submenu.
 * The Mac's conditional Update action opens the release page here, as Openbar's builds are not signed to install themselves.
 */
export function islandMenuEntries(): MenuEntry[] {
  const state = useIsland.getState();
  const update = useUpdates.getState().latest;
  const prefs = useSessions.getState().prefs;
  const mute = (until: number) => {
    void setPrefs({ sessions: { soundMutedUntil: until } })
      .then(() => {
        if (until > Date.now() / 1000) void bridge.sound.stopSession();
      })
      .catch((error: unknown) => console.error("Could not save session mute", error));
  };
  return [
    ...(modulesOf(state).length
      ? [
          {
            label: t(state.showingSections ? "island.back" : "island.sections"),
            icon: LayoutGrid,
            onSelect: () => useIsland.getState().toggleSections(),
          },
        ]
      : []),
    {
      label: t(state.pinned ? "island.unpin" : "island.pin"),
      icon: state.pinned ? PinOff : Pin,
      onSelect: () => {
        const current = useIsland.getState();
        current.setPinned(!current.pinned);
      },
    },
    {
      label: t("island.settings"),
      icon: Settings,
      onSelect: () => {
        useIsland.getState().collapse();
        void bridge.openSettings();
      },
    },
    { kind: "separator" },
    ...(soundsMuted(prefs)
      ? [
          {
            kind: "header" as const,
            label: `${soundText("muted")} ${new Date(prefs.soundMutedUntil * 1000).toLocaleString()}`,
          },
          { label: soundText("unmute"), icon: Bell, onSelect: () => mute(0) },
        ]
      : []),
    { label: soundText("hour"), icon: BellOff, onSelect: () => mute(muteUntil("hour")) },
    { label: soundText("tomorrow"), icon: BellOff, onSelect: () => mute(muteUntil("tomorrow")) },
    ...(update
      ? [
          {
            label: `${t("s.updateAvailablePrefix")} ${update.version}`,
            icon: Download,
            onSelect: () => openPrompt(update),
          },
        ]
      : []),
    { kind: "separator" },
    { label: t("island.collapse"), icon: ChevronUp, onSelect: () => useIsland.getState().collapse() },
  ];
}

export function useIslandContextMenu() {
  useSoundMute(useSessions((s) => s.prefs.soundMutedUntil));
  const [anchor, setAnchor] = useState<MenuAnchor | null>(null);
  const previousHeld = useRef(false);
  const opened = useRef(false);
  const enabled = useIsland((s) => s.settings.enabled);
  // Rebuild labels when the state changes while the menu is visible.
  useIsland((s) => s.pinned);
  useIsland((s) => s.showingSections);
  useIsland((s) => s.settings);
  useUpdates((s) => s.latest);

  const close = () => {
    if (opened.current) useIsland.getState().setHeld(previousHeld.current);
    opened.current = false;
    setAnchor(null);
  };

  useEffect(() => {
    if (!anchor) return;
    // The transparent native window normally passes clicks outside the island through.
    // Capture them while the menu's backdrop is present, without resizing the island.
    void bridge.setCapture(true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      void bridge.setCapture(false);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [anchor]);

  useEffect(() => {
    if (!enabled) close();
  }, [enabled]);

  useEffect(
    () => () => {
      if (opened.current) useIsland.getState().setHeld(previousHeld.current);
    },
    [],
  );

  return {
    isOpen: anchor !== null,
    onContextMenu(event: MouseEvent) {
      // Page-specific menus (files, mixer, etc.) keep precedence.
      if (event.defaultPrevented || !(event.target as Element).closest(".island-frame, .n-bubble")) return;
      event.preventDefault();
      event.stopPropagation();
      if (!opened.current) previousHeld.current = useIsland.getState().held;
      opened.current = true;
      useIsland.getState().setHeld(true);
      setAnchor({ x: event.clientX, y: event.clientY });
    },
    menu:
      anchor && enabled ? (
        <Menu
          anchor={anchor}
          entries={islandMenuEntries()}
          onClose={close}
          bounds="viewport"
          className="n-context-menu"
        />
      ) : null,
  };
}
