// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { create } from "zustand";
import {
  ArrowDownToLine,
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileText,
  Film,
  HardDrive,
} from "lucide-react";
import { edgeInset } from "../island/geometry";
import { bridge } from "../shared/bridge";
import { modulesOf, useIsland } from "../island/store";
import type { ActivityViewProps } from "../island/views/Activity";
import type { DownloadItem, DownloadSnapshot } from "./types";
import "./downloads.css";

export const useDownloads = create<DownloadSnapshot>(() => ({ items: [], folders: [] }));
export const bytes = (n: number) => {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
};
export const progress = (item: DownloadItem) =>
  item.total && item.total > 0 ? Math.min(100, Math.floor((item.received / item.total) * 100)) : null;
const active = (items: DownloadItem[]) => items.filter((i) => i.status === "downloading");
const shown = () => {
  const island = useIsland.getState();
  return island.settings.enabled && island.settings.downloadProgress && modulesOf(island).includes("downloads");
};
function syncActivity() {
  if (!shown() && useIsland.getState().notice?.kind === "downloads") useIsland.getState().dismissNotice();
  const count = active(useDownloads.getState().items).length;
  useIsland
    .getState()
    .setActivity("downloads", count && shown() ? { wing: 36, capsuleContent: 68, opens: "downloads" } : null);
}
export async function subscribeDownloads() {
  // Listen first: an initial fetch must not overwrite a newer event.
  let version = 0;
  const receive = (next: DownloadSnapshot) => {
    useDownloads.setState(next);
    syncActivity();
  };
  await bridge.downloads.onSnapshot((snapshot) => {
    version++;
    receive(snapshot);
  });
  const at = version;
  const first = await bridge.downloads.snapshot();
  if (first && version === at) {
    useDownloads.setState(first);
    syncActivity();
  }
}
useIsland.subscribe((s, previous) => {
  if (s.settings !== previous.settings) syncActivity();
});

export function DownloadStrip({ geometry, size }: ActivityViewProps) {
  const items = useDownloads((s) => s.items);
  const running = active(items);
  const first = running[0];
  const pct = first ? progress(first) : null;
  const padding = geometry.floatingGap === null ? edgeInset(size, 14, 3, 5, geometry.input) : 12;
  return (
    <button
      className="dl-strip"
      style={{ width: size.width, height: size.height, paddingInline: padding }}
      onClick={() => useIsland.getState().activateActivity("downloads")}
      aria-label={first ? `Downloads: ${first.name}` : "Downloads"}
      title={first?.name}
    >
      <span>
        <ArrowDownToLine size={15} />
        {running.length > 1 ? running.length : ""}
      </span>
      <span className="dl-strip-name">{first?.name}</span>
      <span
        className={`dl-mini-progress${pct === null ? " is-indeterminate" : ""}`}
        role="progressbar"
        aria-label="Download progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct ?? undefined}
      >
        <span style={pct === null ? undefined : { width: `${pct}%` }} />
      </span>
    </button>
  );
}
export function DownloadsPage() {
  const { items } = useDownloads();
  return (
    <div className="dl-page">
      <h2>Downloads</h2>
      {items.length === 0 ? (
        <p className="dl-empty">New downloads and file transfers appear here automatically.</p>
      ) : null}
      {items.map((item) => {
        const Icon = fileIcon(item.name);
        const pct = progress(item);
        return (
          <div key={item.id} className="dl-item">
            <span className={`dl-file-icon is-${item.status}`}>
              <Icon size={30} aria-hidden="true" />
            </span>
            <div className="dl-info">
              <strong title={item.path}>{item.name}</strong>
              <span>
                {item.source} ·{" "}
                {item.status === "completed"
                  ? "Saved"
                  : item.status === "interrupted"
                    ? "Stopped"
                    : item.speed
                      ? `${bytes(item.speed)}/s`
                      : "Downloading"}
              </span>
              {item.status === "downloading" ? (
                <progress max={100} value={pct ?? undefined} aria-label={`${item.name}: download progress`} />
              ) : null}
            </div>
            <div className="dl-size">
              <HardDrive size={18} aria-hidden="true" />
              <strong>{(item.received / (1024 * 1024)).toFixed(1)} MB</strong>
              <span title={item.total === null ? "The app has not provided the total file size" : undefined}>
                / {item.total === null ? "?" : (item.total / (1024 * 1024)).toFixed(1)} MB
              </span>
              {item.status === "downloading" && pct !== null ? <span>{pct}%</span> : null}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function fileIcon(name: string) {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  if (["zip", "rar", "7z", "tar", "gz", "iso"].includes(extension)) return FileArchive;
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "avif"].includes(extension)) return FileImage;
  if (["mp3", "wav", "flac", "ogg", "m4a"].includes(extension)) return FileAudio;
  if (["mp4", "mkv", "webm", "avi", "mov"].includes(extension)) return Film;
  if (["pdf", "txt", "doc", "docx", "epub"].includes(extension)) return FileText;
  if (["js", "ts", "py", "rs", "html", "json"].includes(extension)) return FileCode;
  return File;
}
