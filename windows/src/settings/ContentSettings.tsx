// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Content (NotchSettings.swift:317-538): every section in the
// island's order, each a card with its switch and, inside, its position and
// its own options. The Mac's checkbox writes notchHiddenModules and, for the
// sections that have one, their own notch*Enabled key; this does the same.

import { ArrowDown, ArrowUp } from "lucide-react";
import { useState } from "react";
import { t, type StringKey } from "../i18n";
import { MODULES, type ModuleId, visibleModules } from "../island/modules";
import { sectionEnabled } from "../island/settings";
import { QuickAccessSettings } from "./QuickAccessSettings";
import { SessionsOptions } from "./SessionsSettings";
import { SystemOptions } from "./SystemSettings";
import { CalendarOptions } from "./CalendarSettings";
import { ControlsOptions, MixerOptions } from "./ControlsSettings";
import { ScratchpadOptions, ToolsOptions } from "./ToolsSettings";
import { ClipboardOptions } from "./ClipboardSettings";
import { AgentsOptions } from "./AgentsSettings";
import { useSettings } from "./state";
import { T } from "./strings";
import { Expander, Group, Page, SubRow, Toggle } from "./ui";

/** Sections with their own switch on the Mac (notchTimerEnabled and so on). */
const OWN_SWITCH: ModuleId[] = ["timer", "camera", "calendar", "notifications", "agents", "sessions", "watch"];

/** What the Windows island already has; the rest show "coming to Windows". */
const ON_WINDOWS: ModuleId[] = [
  "music",
  "notifications",
  "sessions",
  "downloads",
  "timer",
  "system",
  "calendar",
  "controls",
  "mixer",
  "tools",
  "scratchpad",
  "clipboard",
  "files",
  "agents",
  "watch",
];

function DownloadOptions() {
  const folders = useSettings((s) => s.prefs.island.downloadFolders);
  const set = useSettings((s) => s.island);
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <div className="st-download-folders">
      <label htmlFor="download-folders">{T.content.downloadFolders}</label>
      <p>{T.content.downloadFoldersCaption}</p>
      <textarea
        id="download-folders"
        rows={3}
        spellCheck={false}
        placeholder={"D:\\Downloads\nD:\\Games\\Downloads"}
        value={draft ?? folders.join("\n")}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          if (draft !== null) {
            set("downloadFolders", [
              ...new Set(
                draft
                  .split(/\r?\n/)
                  .map((s) => s.trim())
                  .filter(Boolean),
              ),
            ]);
            setDraft(null);
          }
        }}
      />
    </div>
  );
}

function Options({ id }: { id: ModuleId }) {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  if (!ON_WINDOWS.includes(id)) return <p className="st-note">{T.content.comingSoon}</p>;
  switch (id) {
    case "downloads":
      return <DownloadOptions />;
    case "sessions":
      return <SessionsOptions />;
    case "system":
      return <SystemOptions />;
    case "tools":
      return <ToolsOptions />;
    case "scratchpad":
      return <ScratchpadOptions />;
    case "clipboard":
      return <ClipboardOptions />;
    case "agents":
      return <AgentsOptions />;
    case "music":
      return (
        <>
          <SubRow title={T.content.showPlayingMusic} caption={T.content.showPlayingMusicCaption}>
            <Toggle
              label={T.content.showPlayingMusic}
              checked={island.showPlayingMusic}
              onChange={(on) => set("showPlayingMusic", on)}
            />
          </SubRow>
        </>
      );
    case "notifications":
      return <p className="st-note">{T.content.notificationsCaption}</p>;
    case "timer":
      return (
        <>
          <SubRow title={T.content.hideTimerCountdown} caption={T.content.hideTimerCountdownCaption}>
            <Toggle
              label={T.content.hideTimerCountdown}
              checked={island.hideTimerCountdown}
              onChange={(on) => set("hideTimerCountdown", on)}
            />
          </SubRow>
          <SubRow title={T.content.timerSound} caption={T.content.timerSoundCaption}>
            <Toggle
              label={T.content.timerSound}
              checked={island.timerSoundEnabled}
              onChange={(on) => set("timerSoundEnabled", on)}
            />
          </SubRow>
        </>
      );
    case "files":
      return (
        <>
          <SubRow title={T.content.dragReveal} caption={T.content.dragRevealCaption}>
            <Toggle label={T.content.dragReveal} checked={island.dragReveal} onChange={(on) => set("dragReveal", on)} />
          </SubRow>
          <SubRow title={T.content.shelfRemoveAfterDrop} caption={T.content.shelfRemoveAfterDropCaption}>
            <Toggle
              label={T.content.shelfRemoveAfterDrop}
              checked={island.shelfRemoveAfterDrop}
              onChange={(on) => set("shelfRemoveAfterDrop", on)}
            />
          </SubRow>
          <SubRow title={T.content.shelfCloseAfterDrop} caption={T.content.shelfCloseAfterDropCaption}>
            <Toggle
              label={T.content.shelfCloseAfterDrop}
              checked={island.shelfCloseAfterDrop}
              onChange={(on) => set("shelfCloseAfterDrop", on)}
            />
          </SubRow>
        </>
      );
    case "calendar":
      return <CalendarOptions />;
    case "watch":
      return (
        <SubRow title={T.content.watchSound} caption={T.content.watchSoundCaption}>
          <Toggle label={T.content.watchSound} checked={island.watchSound} onChange={(on) => set("watchSound", on)} />
        </SubRow>
      );
    case "controls":
      return <ControlsOptions />;
    case "mixer":
      return <MixerOptions />;
    default:
      return <p className="st-note">{T.content.noOptions}</p>;
  }
}

export function ContentSettings() {
  const island = useSettings((s) => s.prefs.island);
  const patch = useSettings((s) => s.patch);
  // Every section in order, shown or not.
  const order = visibleModules(island.moduleOrder, [], () => true);

  const shown = (id: ModuleId) => !island.hiddenModules.includes(id) && sectionEnabled(island, id);
  const setShown = (id: ModuleId, on: boolean) => {
    const hidden = island.hiddenModules.filter((m) => m !== id);
    patch({
      island: {
        hiddenModules: on ? hidden : [...hidden, id],
        // A section with its own switch keeps it in step; any other gets a
        // stale "off" (an older or hand-edited file) cleared when switched on.
        ...(OWN_SWITCH.includes(id)
          ? { sections: { ...island.sections, [id]: on } }
          : on && island.sections[id] === false
            ? { sections: { ...island.sections, [id]: true } }
            : {}),
      },
    });
  };
  const move = (id: ModuleId, by: -1 | 1) => {
    const next = [...order];
    const from = next.indexOf(id);
    const to = from + by;
    if (to < 0 || to >= next.length) return;
    [next[from], next[to]] = [next[to]!, next[from]!];
    patch({ island: { moduleOrder: next } });
  };

  return (
    <Page title={T.nav.content} lead={T.content.lead}>
      <QuickAccessSettings />
      <Group title={T.content.sections}>
        {order.map((id, index) => {
          const info = MODULES.find((m) => m.id === id)!;
          const title = t(`module.${id}` as StringKey);
          const on = shown(id);
          return (
            <Expander
              key={id}
              icon={info.icon}
              title={title}
              caption={T.summaries[id]}
              defaultOpen={id === "sessions"}
              control={<Toggle label={`${title}: ${T.content.show}`} checked={on} onChange={(v) => setShown(id, v)} />}
            >
              <SubRow title={T.content.position} caption={T.content.positionCaption(index + 1, order.length)}>
                <button
                  type="button"
                  className="st-icon-button"
                  aria-label={`${title}: ${T.content.moveUp}`}
                  title={T.content.moveUp}
                  disabled={index === 0}
                  onClick={() => move(id, -1)}
                >
                  <ArrowUp size={16} aria-hidden />
                </button>
                <button
                  type="button"
                  className="st-icon-button"
                  aria-label={`${title}: ${T.content.moveDown}`}
                  title={T.content.moveDown}
                  disabled={index === order.length - 1}
                  onClick={() => move(id, 1)}
                >
                  <ArrowDown size={16} aria-hidden />
                </button>
              </SubRow>
              <Options id={id} />
            </Expander>
          );
        })}
      </Group>
    </Page>
  );
}
