// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { forwardRef, useEffect, useState } from "react";
import { create } from "zustand";
import {
  ArrowDownToLine,
  Check,
  ChevronDown,
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileText,
  Film,
  Folder,
  FolderOpen,
  HardDrive,
  Inbox,
  LayoutGrid,
  type LucideIcon,
  type LucideProps,
} from "lucide-react";
import { t } from "../i18n";
import { edgeInset } from "../island/geometry";
import { bridge } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import { MenuButton, type MenuEntry } from "../shared/ui/Menu";
import { modulesOf, useIsland } from "../island/store";
import type { ActivityViewProps } from "../island/views/Activity";
import { IconButton } from "../island/views/Controls";
import { S } from "./strings";
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

/** arrow.down.circle.fill: a filled disc the arrow is cut out of. */
const DownloadedGlyph = forwardRef<SVGSVGElement, LucideProps>(function DownloadedGlyph(
  { size = 14, strokeWidth: _strokeWidth, ...props },
  ref,
) {
  return (
    <svg ref={ref} width={size} height={size} viewBox="0 0 24 24" {...props}>
      <circle cx="12" cy="12" r="11" fill="currentColor" />
      <path
        d="M12 6.5v10.5M7.25 12.5 12 17l4.75-4.5"
        fill="none"
        stroke="#000"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}) as LucideIcon;

/** What was saved since the last snapshot; the first one only says what is already there. */
let saved: Set<string> | null = null;
export function arrival(known: Set<string> | null, items: DownloadItem[]): DownloadItem | null {
  if (!known) return null;
  return items.find((item) => item.status === "completed" && !known.has(item.id)) ?? null;
}
function announce(items: DownloadItem[]) {
  const item = arrival(saved, items);
  saved = new Set(items.filter((i) => i.status === "completed").map((i) => i.id));
  if (!item || !shown()) return;
  useIsland.getState().showNotice({
    id: `download:${item.id}:${Date.now()}`,
    kind: "downloads",
    title: S.completed,
    detail: item.name,
    priority: 0,
    duration: 6,
    opens: "downloads",
    icon: DownloadedGlyph,
    mascot: "celebrate",
  });
}

export async function subscribeDownloads() {
  saved = null;
  // Listen first: an initial fetch must not overwrite a newer event.
  let version = 0;
  const receive = (next: DownloadSnapshot) => {
    useDownloads.setState(next);
    syncActivity();
    announce(next.items);
  };
  await bridge.downloads.onSnapshot((snapshot) => {
    version++;
    receive(snapshot);
  });
  const at = version;
  const first = await bridge.downloads.snapshot();
  if (first && version === at) receive(first);
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
  const items = useDownloads((s) => s.items);
  return (
    <div className="dl-page">
      {items.length === 0 ? <p className="dl-empty">{S.waiting}</p> : null}
      {items.map((item) => (
        <DownloadRow key={item.id} item={item} />
      ))}
    </div>
  );
}

/** Each folder by its name, or its whole path where two share one. */
export function folderNames(folders: string[]): string[] {
  const name = (folder: string) =>
    folder
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() || folder;
  const lowered = folders.map((folder) => name(folder).toLowerCase());
  return folders.map((folder, i) =>
    lowered.indexOf(lowered[i]!) === lowered.lastIndexOf(lowered[i]!) ? name(folder) : folder,
  );
}

/** Watched alongside the rest, unless it already is. The island stays open behind the dialog. */
export async function addFolder(): Promise<void> {
  const island = useIsland.getState();
  island.setHeld(true);
  try {
    const folder = await bridge.downloads.chooseFolder(S.addFolderTitle);
    if (!folder) return;
    const chosen = useIsland.getState().settings.downloadFolders;
    const same = (other: string) => other.toLowerCase() === folder.toLowerCase();
    if (chosen.some(same) || useDownloads.getState().folders.some(same)) return;
    await setPrefs({ island: { downloadFolders: [...chosen, folder] } });
  } finally {
    useIsland.getState().setHeld(false);
  }
}

/**
 * The top row's left: the sections button where no Explore circle stands in
 * for it, then the watched folder by name, or Downloads for several. Each
 * opens in File Explorer; Settings removes them (NotchDownloadsView.folderMenu).
 */
export function DownloadsLeading() {
  const folders = useDownloads((s) => s.folders);
  const hasExplore = useIsland((s) => s.settings.quickAccess.some((b) => b.action.kind === "explore"));
  const names = folderNames(folders);
  const title = folders.length === 1 ? names[0]! : S.title;
  const entries = (): MenuEntry[] => [
    ...folders.map((folder, i): MenuEntry => ({
      label: names[i]!,
      icon: Folder,
      onSelect: () => void bridge.downloads.openFolder(folder),
    })),
    ...(folders.length > 0 ? [{ kind: "separator" } as const] : []),
    { label: S.addFolder, onSelect: () => void addFolder() },
  ];
  return (
    <>
      {hasExplore ? null : (
        <IconButton
          icon={LayoutGrid}
          label={t("island.sections")}
          onClick={() => useIsland.getState().toggleSections()}
        />
      )}
      <MenuButton className="dl-band-menu" label={`${S.title}: ${title}`} alignRight={false} entries={entries}>
        <Folder size={13} strokeWidth={2} aria-hidden="true" />
        <span>{title}</span>
        <ChevronDown size={10} strokeWidth={2.75} aria-hidden="true" />
      </MenuButton>
    </>
  );
}

function DownloadRow({ item }: { item: DownloadItem }) {
  const completed = item.status === "completed";
  const hasShelf = useIsland((s) => modulesOf(s).includes("files"));
  const [added, setAdded] = useState(false);
  // The same path downloaded again is a new file for the shelf.
  useEffect(() => setAdded(false), [item.status]);
  const Icon = fileIcon(item.name);
  const pct = progress(item);
  const body = (
    <>
      <span className={`dl-file-icon is-${item.status}`}>
        <Icon size={30} aria-hidden="true" />
      </span>
      <span className="dl-info">
        <strong title={completed ? undefined : item.path}>{item.name}</strong>
        <span>
          {item.source} ·{" "}
          {completed
            ? S.saved
            : item.status === "interrupted"
              ? S.stopped
              : item.speed
                ? `${bytes(item.speed)}/s`
                : S.inProgress}
        </span>
        {item.status === "downloading" ? (
          <progress max={100} value={pct ?? undefined} aria-label={`${item.name}: download progress`} />
        ) : null}
      </span>
    </>
  );
  return (
    <div className="dl-item">
      {completed ? (
        <button
          type="button"
          className="dl-open"
          title={`${S.open}: ${item.path}`}
          aria-label={`${S.open}: ${item.name}`}
          onClick={() => void bridge.downloads.open(item.id)}
        >
          {body}
        </button>
      ) : (
        <div className="dl-open">{body}</div>
      )}
      {completed ? (
        <div className="dl-actions">
          <IconButton
            icon={FolderOpen}
            label={S.showInExplorer}
            onClick={() => void bridge.downloads.reveal(item.id)}
          />
          {hasShelf ? (
            <IconButton
              icon={added ? Check : Inbox}
              label={added ? S.addedToFiles : S.addToFiles}
              disabled={added}
              onClick={() => {
                void bridge.downloads.addToShelf(item.id).then((done) => {
                  if (done) setAdded(true);
                });
              }}
            />
          ) : null}
        </div>
      ) : null}
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
