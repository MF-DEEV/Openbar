// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Behavior (NotchSettings.swift:641-729): how the island opens,
// where it reopens, the displays it uses and whether captures show it.

import { Camera, Eye, Keyboard, Maximize, Monitor, MousePointerClick, Timer, TimerReset, Undo2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { t, type StringKey } from "../i18n";
import { inTauri } from "../shared/bridge";
import { visibleModules } from "../island/modules";
import { type DisplayChoice, type HomeDestination, type OpeningMode, sectionEnabled } from "../island/settings";
import { useSettings } from "./state";
import { T } from "./strings";
import { Group, Page, Row, Segmented, Select, Slider, Toggle } from "./ui";

const seconds = (v: number) => `${v.toFixed(2)} s`;

export function BehaviorSettings() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const patch = useSettings((s) => s.patch);
  const hover = island.opening !== "click";
  // Asked again after each change: registering can fail when another app holds the keys.
  const [shortcutTaken, setShortcutTaken] = useState(false);
  useEffect(() => {
    if (!inTauri || !island.shortcutEnabled) return setShortcutTaken(false);
    // An answer for an earlier state (defaults before the file loaded, a save
    // that failed) must not land after a newer one.
    let live = true;
    void invoke<boolean>("shortcut_registered").then((ok) => live && setShortcutTaken(!ok));
    return () => {
      live = false;
    };
  }, [island.shortcutEnabled]);
  const expands = island.opening === "hoverExpand" || island.opening === "hiddenUntilHover";

  const shownSections = visibleModules(island.moduleOrder, island.hiddenModules, (id) => sectionEnabled(island, id));
  const reopening = island.returnHome ? island.homeModule : "last";
  const reopenOptions = [
    { value: "last", label: T.behavior.lastPage },
    { value: "explore", label: T.behavior.explore },
    ...shownSections.map((id) => ({ value: id, label: t(`module.${id}` as StringKey) })),
  ];
  // A saved section that is now hidden stays listed so the choice still reads.
  if (island.returnHome && island.homeModule !== "explore" && !shownSections.includes(island.homeModule)) {
    reopenOptions.push({ value: island.homeModule, label: t(`module.${island.homeModule}` as StringKey) });
  }

  return (
    <Page title={T.nav.behavior}>
      <Group title={T.behavior.opening}>
        <Row icon={MousePointerClick} title={T.behavior.opening} caption={T.behavior.openingCaption}>
          <Select<OpeningMode>
            label={T.behavior.opening}
            value={island.opening}
            onChange={(v) => set("opening", v)}
            options={[
              { value: "click", label: T.behavior.click },
              { value: "hoverPreview", label: T.behavior.hoverPreview },
              { value: "hoverExpand", label: T.behavior.hoverExpand },
              { value: "hiddenUntilHover", label: T.behavior.hiddenUntilHover },
            ]}
          />
        </Row>
        {island.opening === "hiddenUntilHover" ? (
          <Row icon={Eye} title={T.behavior.showActivity} caption={T.behavior.showActivityCaption}>
            <Toggle
              label={T.behavior.showActivity}
              checked={island.hiddenShowsActivity}
              onChange={(on) => set("hiddenShowsActivity", on)}
            />
          </Row>
        ) : null}
        {hover ? (
          <Row icon={Timer} title={T.behavior.activation} caption={T.behavior.activationCaption}>
            <Slider
              label={T.behavior.activation}
              min={0.1}
              max={1}
              step={0.05}
              value={island.hoverDelay}
              format={seconds}
              onChange={(v) => set("hoverDelay", Math.round(v * 100) / 100)}
            />
          </Row>
        ) : null}
        {expands ? (
          <Row icon={TimerReset} title={T.behavior.closing} caption={T.behavior.closingCaption}>
            <Slider
              label={T.behavior.closing}
              min={0.1}
              max={2}
              step={0.05}
              value={island.closeDelay}
              format={seconds}
              onChange={(v) => set("closeDelay", Math.round(v * 100) / 100)}
            />
          </Row>
        ) : null}
        <Row
          icon={Keyboard}
          title={T.behavior.shortcut}
          caption={
            shortcutTaken ? <span className="st-error">{T.behavior.shortcutTaken}</span> : T.behavior.shortcutCaption
          }
        >
          <span className="st-keys" aria-hidden>
            <kbd>Ctrl</kbd>
            <kbd>Alt</kbd>
            <kbd>Space</kbd>
          </span>
          <Toggle
            label={T.behavior.shortcut}
            checked={island.shortcutEnabled}
            onChange={(on) => set("shortcutEnabled", on)}
          />
        </Row>
        <Row icon={Undo2} title={T.behavior.reopening} caption={T.behavior.reopeningCaption}>
          <Select
            label={T.behavior.reopening}
            value={reopening}
            options={reopenOptions}
            onChange={(v) =>
              v === "last"
                ? patch({ island: { returnHome: false } })
                : patch({ island: { returnHome: true, homeModule: v as HomeDestination } })
            }
          />
        </Row>
      </Group>

      <Group title={T.behavior.display}>
        <Row icon={Monitor} title={T.behavior.where} caption={T.behavior.whereCaption}>
          <Segmented<DisplayChoice>
            label={T.behavior.where}
            value={island.display}
            onChange={(v) => set("display", v)}
            options={[
              { value: "primary", label: T.behavior.displayPrimary },
              { value: "cursor", label: T.behavior.displayCursor },
            ]}
          />
        </Row>
        <Row icon={Maximize} title={T.behavior.hideFullscreen} caption={T.behavior.hideFullscreenCaption}>
          <Toggle
            label={T.behavior.hideFullscreen}
            checked={island.hideInFullscreen}
            onChange={(on) => set("hideInFullscreen", on)}
          />
        </Row>
      </Group>

      <Group title={T.behavior.privacy}>
        <Row icon={Camera} title={T.behavior.captures} caption={T.behavior.capturesCaption}>
          <Toggle
            label={T.behavior.captures}
            checked={island.showInCaptures}
            onChange={(on) => set("showInCaptures", on)}
          />
        </Row>
      </Group>
    </Page>
  );
}
