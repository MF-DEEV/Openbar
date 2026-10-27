// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Content (NotchSettings.swift:317-538): every section in the
// island's order, each a card with its switch and, inside, its position and
// its own options. The Mac's checkbox writes notchHiddenModules and, for the
// sections that have one, their own notch*Enabled key; this does the same.

import { ArrowDown, ArrowUp, LayoutGrid, Music, Settings } from "lucide-react";
import { useState } from "react";
import { capsulePath, notchPath } from "../island/geometry";
import { t, type StringKey } from "../i18n";
import { MODULES, type ModuleId, visibleModules } from "../island/modules";
import { DEFAULT_QUICK_ACCESS, sectionEnabled } from "../island/settings";
import { SessionsOptions } from "./SessionsSettings";
import { SystemOptions } from "./SystemSettings";
import { CalendarOptions } from "./CalendarSettings";
import { useSettings } from "./state";
import { T } from "./strings";
import { Expander, Group, Page, Row, Select, SubRow, Toggle } from "./ui";

/** The lower side circles; Explore, Settings and the bottom circle keep their places. */
function QuickAccessSettings() {
  const island = useSettings((s) => s.prefs.island);
  const patch = useSettings((s) => s.patch);
  const slots = [
    { fallback: DEFAULT_QUICK_ACCESS[1]!, title: T.content.leftShortcut },
    { fallback: DEFAULT_QUICK_ACCESS[3]!, title: T.content.rightShortcut },
  ];
  const order = visibleModules(island.moduleOrder, [], () => true);
  const options = order.map((id) => ({ value: id, label: t(`module.${id}` as StringKey) }));
  const assign = (slot: number, module: ModuleId) => {
    const fallback = slots[slot]!.fallback;
    const button = island.quickAccess.find((b) => b.id === fallback.id) ?? fallback;
    const replacement = { ...button, action: { kind: "module" as const, module }, label: undefined };
    const exists = island.quickAccess.some((b) => b.id === fallback.id);
    patch({
      island: {
        quickAccess: exists
          ? island.quickAccess.map((b) => (b.id === fallback.id ? replacement : b))
          : [...island.quickAccess, replacement],
        hiddenModules: island.hiddenModules.filter((id) => id !== module),
        sections: { ...island.sections, [module]: true },
      },
    });
  };

  return (
    <Group title={T.content.quickAccess}>
      <div className="st-quick-preview-card">
        <div className="st-quick-preview" role="group" aria-label={T.content.quickPreview}>
          <svg viewBox="0 0 480 248" aria-hidden="true">
            <g transform="translate(84 20)">
              <path
                d={
                  island.silhouette === "notch"
                    ? notchPath(312, 150, island)
                    : capsulePath(312, 150, 0, island.roundness)
                }
                fill="#000"
              />
            </g>
            <rect x="176" y="68" width="128" height="7" rx="3.5" fill="#333" />
            <rect x="200" y="87" width="80" height="5" rx="2.5" fill="#222" />
            <path d="M234 113 L234 137 L252 125 Z" fill="#ddd" />
          </svg>
          <span className="st-quick-preview-circle" style={{ left: "10.83%", top: "21.77%" }} aria-hidden="true">
            <LayoutGrid size={19} />
          </span>
          <span className="st-quick-preview-circle" style={{ left: "89.17%", top: "21.77%" }} aria-hidden="true">
            <Settings size={19} />
          </span>
          <span className="st-quick-preview-circle" style={{ left: "50%", top: "82.26%" }} aria-hidden="true">
            <Music size={19} />
          </span>
          {slots.map(({ fallback, title }, index) => {
            const button = island.quickAccess.find((b) => b.id === fallback.id) ?? fallback;
            const module = button.action.kind === "module" ? button.action.module : "timer";
            const Icon = MODULES.find((m) => m.id === module)!.icon;
            const label = `${title}: ${t(`module.${module}` as StringKey)}`;
            return (
              <span
                key={fallback.id}
                className="st-quick-preview-circle is-editable"
                style={{ left: index === 0 ? "10.83%" : "89.17%", top: "45.16%" }}
                title={label}
              >
                <Icon size={20} aria-hidden="true" />
                <select
                  aria-label={`${title} (${T.content.quickPreview})`}
                  value={module}
                  onChange={(e) => assign(index, e.target.value as ModuleId)}
                >
                  {options.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </span>
            );
          })}
        </div>
        <p>{T.content.quickPreviewCaption}</p>
      </div>
      {slots.map(({ fallback, title }, index) => {
        const button = island.quickAccess.find((b) => b.id === fallback.id) ?? fallback;
        const selected = button.action.kind === "module" ? button.action.module : "timer";
        const info = MODULES.find((m) => m.id === selected)!;
        return (
          <Row key={fallback.id} icon={info.icon} title={title} caption={T.content.shortcutCaption}>
            <Select<ModuleId>
              label={title}
              value={selected}
              options={options}
              onChange={(module) => assign(index, module)}
            />
          </Row>
        );
      })}
    </Group>
  );
}

/** Sections with their own switch on the Mac (notchTimerEnabled and so on). */
const OWN_SWITCH: ModuleId[] = ["timer", "camera", "calendar", "notifications", "agents", "sessions", "watch"];

/** What the Windows island already has; the rest show "coming to Windows". */
const ON_WINDOWS: ModuleId[] = ["music", "notifications", "sessions", "downloads", "timer", "system", "calendar"];

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
    case "calendar":
      return <CalendarOptions />;
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
