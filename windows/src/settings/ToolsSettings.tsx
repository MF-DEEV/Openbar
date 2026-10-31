// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Tools and Scratchpad options on the Content tab: the colour picker's
// format (QuickLauncherView options card), which Quick toggles show and in
// what order (QuickTogglesList's edit mode), the disks "Eject all disks"
// skips (diskEjectExcludedVolumes), and the pad's text size and retention.

import { t, tf, useLocale } from "../i18n";

import { ArrowDown, ArrowUp } from "lucide-react";
import { useState } from "react";
import type { ColorPickerFormat, ScratchpadRetention } from "../island/settings";
import { TEXT_SIZE, sanitizedTextSize } from "../scratchpad/model";
import { externals } from "../tools/externals";
import { toggleOrder } from "../tools/model";
import { S as ToolStrings } from "../tools/strings";
import type { ToggleId } from "../tools/types";
import { useSettings } from "./state";
import { T } from "./strings";
import { Select, SubRow, Toggle } from "./ui";

const TOGGLE_TITLES: Record<ToggleId, string> = {
  get darkMode() {
    return ToolStrings.darkMode;
  },
  get micMute() {
    return ToolStrings.micMute;
  },
  get emptyTrash() {
    return ToolStrings.emptyTrashTitle;
  },
  get ejectDisks() {
    return ToolStrings.ejectTitle;
  },
  get hiddenFiles() {
    return ToolStrings.hiddenFiles;
  },
  get desktopIcons() {
    return ToolStrings.desktopIcons;
  },
  get lockScreen() {
    return ToolStrings.lockScreenTitle;
  },
  get displayOff() {
    return ToolStrings.displayOffTitle;
  },
  get screenSaver() {
    return ToolStrings.screenSaverTitle;
  },
};

function ToggleRows() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const patch = useSettings((s) => s.patch);
  const order = toggleOrder(island).filter((id) => id !== "micMute" || !!externals().mic);
  const move = (id: ToggleId, by: -1 | 1) => {
    const next = [...order];
    const from = next.indexOf(id);
    const to = from + by;
    if (to < 0 || to >= next.length) return;
    [next[from], next[to]] = [next[to]!, next[from]!];
    patch({ island: { togglesOrder: next } });
  };
  const setShown = (id: ToggleId, on: boolean) => {
    const hidden = island.togglesHidden.filter((h) => h !== id);
    patch({ island: { togglesHidden: on ? hidden : [...hidden, id].sort() } });
  };
  return (
    <>
      {order.map((id, index) => (
        <SubRow key={id} title={TOGGLE_TITLES[id]}>
          <button
            type="button"
            className="st-icon-button"
            aria-label={`${TOGGLE_TITLES[id]}: ${T.content.moveUp}`}
            title={T.content.moveUp}
            disabled={index === 0}
            onClick={() => move(id, -1)}
          >
            <ArrowUp size={16} aria-hidden />
          </button>
          <button
            type="button"
            className="st-icon-button"
            aria-label={`${TOGGLE_TITLES[id]}: ${T.content.moveDown}`}
            title={T.content.moveDown}
            disabled={index === order.length - 1}
            onClick={() => move(id, 1)}
          >
            <ArrowDown size={16} aria-hidden />
          </button>
          <Toggle
            label={`${TOGGLE_TITLES[id]}: ${T.content.show}`}
            checked={!island.togglesHidden.includes(id)}
            onChange={(on) => setShown(id, on)}
          />
        </SubRow>
      ))}
    </>
  );
}

function EjectExclusions() {
  useLocale();
  const excluded = useSettings((s) => s.prefs.island.ejectExcluded);
  const set = useSettings((s) => s.island);
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <div className="st-download-folders">
      <label htmlFor="eject-excluded">{T.content.ejectExcluded}</label>
      <p>{T.content.ejectExcludedCaption}</p>
      <textarea
        id="eject-excluded"
        rows={2}
        spellCheck={false}
        placeholder={t("windows.settings.backupE")}
        value={draft ?? excluded.join("\n")}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          if (draft === null) return;
          const lines = draft
            .split(/\r?\n/)
            .map((s) => s.trim())
            .filter(Boolean);
          set("ejectExcluded", [...new Set(lines)]);
          setDraft(null);
        }}
      />
    </div>
  );
}

export function ToolsOptions() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  return (
    <>
      <p className="st-note">{T.content.toolsGridCaption}</p>
      <SubRow title={T.content.captureControls} caption={T.content.captureControlsCaption}>
        <Toggle
          label={T.content.captureControls}
          checked={island.captureControls}
          onChange={(on) => set("captureControls", on)}
        />
      </SubRow>
      <SubRow
        title={t("windows.settings.screenshotPreviewDuration")}
        caption={t("windows.settings.pausesWhileHoveredNeverAffectsPinnedScreenshots")}
      >
        <Select<string>
          label={t("windows.settings.screenshotPreviewDuration")}
          value={String(island.screenshotPreviewDuration)}
          options={[0, 3, 5, 8, 15, 30, 60].map((seconds) => ({
            value: String(seconds),
            label: seconds === 0 ? t("windows.settings.label") : tf("windows.settings.label74957d", seconds),
          }))}
          onChange={(value) => set("screenshotPreviewDuration", Number(value))}
        />
      </SubRow>
      <SubRow
        title={t("windows.settings.dismissScreenshotPreviewOnFocusLoss")}
        caption={t("windows.settings.afterClickingThePreviewSwitchingToAnotherApp")}
      >
        <Toggle
          label={t("windows.settings.dismissScreenshotPreviewOnFocusLoss")}
          checked={island.screenshotPreviewDismissOnBlur}
          onChange={(on) => set("screenshotPreviewDismissOnBlur", on)}
        />
      </SubRow>
      <SubRow title={T.content.colorFormat} caption={T.content.colorFormatCaption}>
        <Select<ColorPickerFormat>
          label={T.content.colorFormat}
          value={island.colorPickerFormat}
          options={[
            { value: "hex", label: "HEX" },
            { value: "rgb", label: "RGB" },
            { value: "hsl", label: "HSL" },
          ]}
          onChange={(v) => set("colorPickerFormat", v)}
        />
      </SubRow>
      {island.colorPickerFormat === "hex" ? (
        <SubRow title={T.content.bareHex}>
          <Toggle
            label={T.content.bareHex}
            checked={island.colorPickerBareHex}
            onChange={(on) => set("colorPickerBareHex", on)}
          />
        </SubRow>
      ) : null}
      <SubRow title={T.content.quickToggles} caption={T.content.quickTogglesCaption} />
      <ToggleRows />
      <EjectExclusions />
    </>
  );
}

export function ScratchpadOptions() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const sizes: { value: string; label: string }[] = [];
  for (let size = TEXT_SIZE.min; size <= TEXT_SIZE.max; size++) sizes.push({ value: String(size), label: `${size}` });
  return (
    <>
      <SubRow title={T.content.textSize} caption={T.content.textSizeCaption}>
        <Select<string>
          label={T.content.textSize}
          value={String(sanitizedTextSize(island.scratchpadTextSize))}
          options={sizes}
          onChange={(v) => set("scratchpadTextSize", Number(v))}
        />
      </SubRow>
      <SubRow title={T.content.retention} caption={T.content.retentionCaption}>
        <Select<ScratchpadRetention>
          label={T.content.retention}
          value={island.scratchpadRetention}
          options={[
            { value: "never", label: T.content.retentionNever },
            { value: "day", label: T.content.retentionDay },
            { value: "week", label: T.content.retentionWeek },
            { value: "month", label: T.content.retentionMonth },
          ]}
          onChange={(v) => set("scratchpadRetention", v)}
        />
      </SubRow>
    </>
  );
}
