// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// Quick toggles hosted in the Tools page (QuickTogglesList,
// QuickTogglesSection.swift; spec 05 §7.3): one row per action in the
// person's order, each showing what a click will do, and running or
// failed while that lasts. State is read when the list opens, never polled.

import {
  Eject,
  Eye,
  EyeOff,
  LoaderCircle,
  Lock,
  type LucideIcon,
  Mic,
  MicOff,
  Monitor,
  MonitorOff,
  Moon,
  Sparkles,
  Sun,
  Trash,
} from "lucide-react";
import { useEffect } from "react";
import { useIsland } from "../island/store";
import { Sheet } from "./Sheet";
import { S } from "./strings";
import { confirmTrash, refreshToggles, runToggle, useTools, visibleToggles } from "./store";
import type { RunState, ToggleId, ToggleState } from "./types";

interface Row {
  icon: LucideIcon;
  title: string;
  caption: string;
  /** The caption is news (nothing to eject, ejecting, a failure) rather than a description. */
  notice?: boolean;
}

function row(id: ToggleId, info: ToggleState | null, run: RunState | undefined, micMuted: boolean): Row {
  const failed = run === "failed";
  const caption = (idle: string) => (failed ? S.actionFailed : idle);
  switch (id) {
    case "darkMode":
      return {
        icon: info?.dark ? Sun : Moon,
        title: info?.dark ? S.darkModeToLight : S.darkModeToDark,
        caption: caption(S.darkModeCaption),
      };
    case "micMute":
      return { icon: micMuted ? MicOff : Mic, title: micMuted ? S.micUnmute : S.micMute, caption: S.micMuteCaption };
    case "emptyTrash":
      return { icon: Trash, title: S.emptyTrashTitle, caption: caption(S.emptyTrashCaption) };
    case "ejectDisks": {
      const text =
        run === "running"
          ? S.ejecting
          : failed
            ? S.ejectFailed
            : info && info.ejectable === 0
              ? S.noExternalDisk
              : S.ejectCaption;
      return { icon: Eject, title: S.ejectTitle, caption: text, notice: text !== S.ejectCaption };
    }
    case "hiddenFiles":
      return {
        icon: info?.hiddenFilesShown ? EyeOff : Eye,
        title: info?.hiddenFilesShown ? S.hiddenFilesHide : S.hiddenFilesShow,
        caption: caption(S.explorerCaption),
      };
    case "desktopIcons":
      return {
        icon: Monitor,
        title: info?.desktopIconsShown === false ? S.desktopIconsShow : S.desktopIconsHide,
        caption: caption(S.desktopCaption),
      };
    case "lockScreen":
      return { icon: Lock, title: S.lockScreenTitle, caption: caption(S.lockScreenCaption) };
    case "displayOff":
      return { icon: MonitorOff, title: S.displayOffTitle, caption: caption(S.displayOffCaption) };
    case "screenSaver":
      return { icon: Sparkles, title: S.screenSaverTitle, caption: caption(S.screenSaverCaption) };
  }
}

export function QuickToggles({ height }: { height: number }) {
  useLocale();
  const settings = useIsland((s) => s.settings);
  const info = useTools((s) => s.toggleInfo);
  const runs = useTools((s) => s.toggleRuns);
  const micMuted = useTools((s) => s.micMuted);
  const confirming = useTools((s) => s.confirmingTrash);

  useEffect(() => {
    void refreshToggles();
  }, []);

  return (
    <div className="tl-page tl-toggles" style={{ height }}>
      <div className="tl-toggles-scroll">
        <div className="tl-toggle-group">
          {visibleToggles(settings, info).map((id) => {
            const run = runs[id];
            const r = row(id, info, run, micMuted);
            const Icon = run === "running" ? LoaderCircle : r.icon;
            return (
              <button
                key={id}
                type="button"
                className={`tl-toggle${run ? ` is-${run}` : ""}`}
                disabled={run === "running"}
                aria-busy={run === "running"}
                onClick={() => runToggle(id)}
              >
                <span className="tl-toggle-icon">
                  <Icon size={14} strokeWidth={2} aria-hidden />
                </span>
                <span className="tl-toggle-text">
                  <span className="tl-toggle-title">{r.title}</span>
                  <span className={`tl-toggle-caption${run === "failed" || r.notice ? " is-news" : ""}`}>
                    {r.caption}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </div>
      {confirming ? (
        <Sheet
          title={S.emptyTrashConfirmTitle}
          message={S.emptyTrashConfirmMessage}
          confirm={S.emptyTrashConfirmButton}
          cancel={S.cancel}
          destructive
          onConfirm={() => confirmTrash(true)}
          onCancel={() => confirmTrash(false)}
        />
      ) : null}
    </div>
  );
}
