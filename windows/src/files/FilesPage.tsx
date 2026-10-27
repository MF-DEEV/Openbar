// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Files page (NotchFilesView.swift; spec 05 §6.1). Tiles fill the
// page's height in rows and continue sideways; an empty shelf shows a
// dashed drop area. The top row holds the hint or "n selected" (or the ZIP's
// progress) on the left, and Create ZIP, Share and the trash on the right.
// How the last ZIP ended shows under the tiles.

import { CircleCheck, FileArchive, Inbox, Share, Trash, Trash2, WandSparkles, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { usePageLayer } from "../island/pageLayer";
import { EmptyState, IconButton } from "../island/views/Controls";
import { bridge } from "../shared/bridge";
import { MenuButton } from "../shared/ui/Menu";
import { createZip } from "./actions";
import { bandTargets, holdsFile, visibleItems } from "./model";
import { useFiles } from "./store";
import { S } from "./strings";
import { TILE, Tile } from "./Tile";
import "./files.css";

export function FilesPage() {
  const items = useFiles((s) => s.items);
  const selection = useFiles((s) => s.selection.ids);
  // Escape lets go of the selection before it closes the island.
  usePageLayer(selection.size > 0, () => useFiles.getState().clearSelection());
  return (
    <div className="fl-page">
      {items.length === 0 ? (
        <div className="fl-empty">
          <svg className="fl-empty-outline" aria-hidden="true">
            <rect />
          </svg>
          <EmptyState icon={Inbox} title={S.dropHint} />
        </div>
      ) : (
        <Rail />
      )}
      <ArchiveStatus />
    </div>
  );
}

/** Rows fill the height; columns run on sideways and scroll (ShelfTileLayout sideways). */
function Rail() {
  const items = useFiles((s) => s.items);
  const expanded = useFiles((s) => s.expanded);
  const arrivals = useFiles((s) => s.arrivals);
  const tiles = visibleItems(items, expanded);
  const ref = useRef<HTMLDivElement>(null);
  const [rows, setRows] = useState(1);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const fit = () =>
      setRows(
        Math.max(1, Math.floor((el.clientHeight - 2 * TILE.inset + TILE.spacing) / (TILE.height + TILE.spacing))),
      );
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Something new arrives at the end: bring it into view.
  useEffect(() => {
    const el = ref.current;
    if (!el || arrivals === 0) return;
    const smooth = !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    el.scrollTo({ left: el.scrollWidth, behavior: smooth ? "smooth" : "auto" });
  }, [arrivals]);

  return (
    <div
      ref={ref}
      className="fl-rail"
      role="listbox"
      aria-multiselectable="true"
      style={{ gridTemplateRows: `repeat(${rows}, ${TILE.height}px)` }}
      onWheel={(e) => {
        // A mouse wheel scrolls the strip sideways.
        if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) e.currentTarget.scrollLeft += e.deltaY;
      }}
    >
      {tiles.map((item) => (
        <Tile key={item.id} item={item} />
      ))}
    </div>
  );
}

/** How the last ZIP ended (NotchFilesView.archiveStatus). */
function ArchiveStatus() {
  const zip = useFiles((s) => s.zip);
  if (zip.running) return null;
  if (zip.failure && zip.failure.kind !== "cancelled") {
    return <p className="fl-status is-failure">{S.failure(zip.failure)}</p>;
  }
  if (zip.outputs.length > 0) {
    return (
      <div className="fl-status">
        <span className="fl-saved">
          <CircleCheck size={12} strokeWidth={2.5} aria-hidden="true" />
          {S.saved}
        </span>
        <button type="button" className="fl-status-button" onClick={() => void bridge.files.revealOutputs()}>
          {S.showInExplorer}
        </button>
      </div>
    );
  }
  if (zip.cancelled) return <p className="fl-status">{S.cancelled}</p>;
  return null;
}

/** The band's left: the ZIP's progress, else guidance or how many are selected. */
export function FilesLeading() {
  const zip = useFiles((s) => s.zip);
  const count = useFiles((s) => s.selection.ids.size);
  const empty = useFiles((s) => s.items.length === 0);
  if (zip.running) {
    const progress = `${S.archive} · ${S.progress(zip.completed, zip.total)}`;
    return (
      <div className="fl-progress" role="status" aria-label={progress}>
        <span className="fl-spinner" aria-hidden="true" />
        <span className="fl-progress-text">{progress}</span>
        <IconButton icon={X} label={S.cancel} disabled={zip.cancelled} onClick={() => void bridge.files.cancelZip()} />
      </div>
    );
  }
  if (empty) return null;
  return count > 0 ? (
    <span className="fl-selected">{S.selected(count)}</span>
  ) : (
    <span className="fl-hint" title={S.hint}>
      {S.hint}
    </span>
  );
}

/** The band's right: Create ZIP, Share, then the trash (NotchFilesView.shelfActions). */
export function FilesTrailing() {
  const items = useFiles((s) => s.items);
  const selection = useFiles((s) => s.selection.ids);
  const running = useFiles((s) => s.zip.running);
  const [wand, setWand] = useState<HTMLElement | null>(null);
  if (items.length === 0) return null;
  const targets = () => bandTargets(useFiles.getState().items, useFiles.getState().selection.ids).map((i) => i.id);
  const files = holdsFile(bandTargets(items, selection));
  return (
    <div className="fl-band">
      <span className="fl-wand">
        <IconButton
          icon={WandSparkles}
          label={S.archive}
          selected={wand !== null}
          disabled={!files || running}
          onClick={(e) => setWand(wand ? null : e.currentTarget)}
        />
      </span>
      {wand ? <ZipPopover anchor={wand} onClose={() => setWand(null)} onZip={() => void createZip(targets())} /> : null}
      <IconButton icon={Share} label={S.share} disabled={!files} onClick={() => void bridge.files.share(targets())} />
      {selection.size === 0 ? (
        <MenuButton
          className="n-icon-button"
          label={S.clearAll}
          disabled={running}
          entries={() => [
            { label: S.clearAll, destructive: true, onSelect: () => void bridge.files.clear() },
            { label: S.cancel },
          ]}
        >
          <Trash size={14} strokeWidth={2} />
        </MenuButton>
      ) : (
        <IconButton
          icon={Trash2}
          label={S.removeSelected}
          disabled={running}
          onClick={() => void bridge.files.remove([...useFiles.getState().selection.ids])}
        />
      )}
    </div>
  );
}

/** The wand's popover: Create ZIP and what it does (270 wide, 14 in). */
function ZipPopover(props: { anchor: HTMLElement; onClose(): void; onZip(): void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  usePageLayer(true, props.onClose);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const at = props.anchor.getBoundingClientRect();
    const frame = document.querySelector(".island-frame")?.getBoundingClientRect();
    const right = (frame?.right ?? window.innerWidth) - 8;
    setPos({ left: Math.min(at.left + at.width / 2 - el.offsetWidth / 2, right - el.offsetWidth), top: at.bottom + 6 });
  }, [props.anchor]);
  useEffect(() => {
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!ref.current?.contains(target) && !props.anchor.contains(target)) props.onClose();
    };
    document.addEventListener("pointerdown", outside, true);
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [props.anchor]);
  return createPortal(
    <div
      ref={ref}
      className="u-menu fl-popover"
      role="dialog"
      aria-label={S.archive}
      style={pos ?? { left: -9999, top: -9999 }}
    >
      <button
        type="button"
        className="fl-popover-action"
        onClick={() => {
          props.onClose();
          props.onZip();
        }}
      >
        <FileArchive size={14} strokeWidth={2} aria-hidden="true" />
        {S.archive}
      </button>
      <p>{S.archiveHint}</p>
    </div>,
    document.body,
  );
}
