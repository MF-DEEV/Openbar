// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// One shelf tile (ShelfTileView, STV:320-700; spec 05 §6.1): 78 × 88,
// radius 10, a 64 × 50 icon well with a thumbnail or the file's icon, and a
// two-line 10 pt title cut in the middle. A pile shows two backplates, its
// count and an open/close chevron. Click selects, Shift+click extends,
// double-click opens a pile, a drag of more than 4 pt takes the tiles out,
// right-click opens the menu, and hovering shows the remove button and,
// after a moment, a tooltip.

import {
  Check,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  File,
  FileText,
  FolderOpen,
  Link,
  Pin,
  PinOff,
  Share,
  X,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { bridge } from "../shared/bridge";
import { type MenuEntry, useMenu } from "../shared/ui/Menu";
import { dragOut } from "./actions";
import { findItem, holdsFile, isImage, leafCount, tileTargets, tooltipText } from "./model";
import { useFiles } from "./store";
import { S } from "./strings";
import type { ShelfItem } from "./types";

export const TILE = { width: 78, height: 88, spacing: 10, inset: 4 };

/** The picture a tile shows: a pile wears its first item's. */
function face(item: ShelfItem): ShelfItem {
  return item.kind === "batch" && item.children?.[0] ? face(item.children[0]) : item;
}

const VIDEO = /\.(mp4|mov|m4v|mkv|webm|avi|wmv)$/i;

/** A real thumbnail for pictures and videos, the file's icon otherwise. */
function useTileImage(item: ShelfItem): { src: string | null; thumbnail: boolean } {
  const shown = face(item);
  const path = shown.kind === "file" ? shown.path : undefined;
  const thumbnail = !!path && (isImage(shown) || VIDEO.test(path));
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    setSrc(null);
    if (!path) return;
    let live = true;
    const size = Math.round(64 * Math.max(1, window.devicePixelRatio || 1));
    void bridge.shell.icon(path, size, thumbnail).then((url) => {
      if (live) setSrc(url ?? null);
    });
    return () => {
      live = false;
    };
  }, [path, thumbnail]);
  return { src, thumbnail: thumbnail && src !== null };
}

let measure: CanvasRenderingContext2D | null = null;
const fits = (text: string, width: number) => {
  measure ??= document.createElement("canvas").getContext("2d");
  if (!measure) return text.length <= 24;
  measure.font = "10px 'Inter Variable', Inter, 'Segoe UI', sans-serif";
  return measure.measureText(text).width <= width;
};

/** Cut in the middle so the start and the extension both stay (byTruncatingMiddle, two lines). */
export function middleTruncated(title: string, room = 72 * 2 - 12, fit = fits): string {
  if (fit(title, room)) return title;
  let keep = title.length;
  while (keep > 2) {
    keep--;
    const head = Math.ceil(keep / 2);
    const cut = `${title.slice(0, head)}…${title.slice(title.length - (keep - head))}`;
    if (fit(cut, room)) return cut;
  }
  return "…";
}

function TileGlyph({ item }: { item: ShelfItem }) {
  useLocale();
  const shown = face(item);
  const Icon = shown.kind === "text" ? FileText : shown.kind === "link" ? Link : File;
  return <Icon size={26} strokeWidth={1.5} aria-hidden="true" />;
}

export function Tile({ item }: { item: ShelfItem }) {
  useLocale();
  const selected = useFiles((s) => s.selection.ids.has(item.id));
  const expanded = useFiles((s) => s.expanded.has(item.id));
  const image = useTileImage(item);
  const menu = useMenu();
  const tooltip = useTooltip(item);
  const press = useRef<{ x: number; y: number; pointer: number } | null>(null);
  const dragged = useRef(false);
  const pile = item.kind === "batch";

  const targets = () => tileTargets(useFiles.getState().items, useFiles.getState().selection.ids, item.id);

  const onPointerDown = (e: React.PointerEvent) => {
    tooltip.hide();
    if (e.button !== 0) return;
    press.current = { x: e.clientX, y: e.clientY, pointer: e.pointerId };
    dragged.current = false;
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const start = press.current;
    if (!start || start.pointer !== e.pointerId || dragged.current) return;
    if (Math.hypot(e.clientX - start.x, e.clientY - start.y) <= 4) return;
    dragged.current = true;
    press.current = null;
    void dragOut(targets());
  };
  const onPointerUp = () => {
    press.current = null;
  };
  const onClick = (e: React.MouseEvent) => {
    if (dragged.current) {
      dragged.current = false;
      return;
    }
    if (pile && e.detail >= 2) useFiles.getState().togglePile(item.id);
    else useFiles.getState().click(item.id, e.shiftKey);
  };

  const onContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    tooltip.hide();
    const anchor = { x: e.clientX, y: e.clientY };
    const ids = targets();
    const items = useFiles.getState().items;
    const files = holdsFile(ids.map((id) => findItem(items, id)).filter((found) => found !== undefined));
    const entries = (apps: MenuEntry[] | null): MenuEntry[] => [
      {
        label: item.pinned ? S.unpin : S.pin,
        icon: item.pinned ? PinOff : Pin,
        onSelect: () => void bridge.files.togglePin(item.id),
      },
      // Notes and links have nothing to open or reveal.
      ...(files
        ? ([
            { kind: "separator" },
            { label: S.open, icon: ExternalLink, onSelect: () => void bridge.files.open(ids) },
            {
              label: S.openWith,
              disabled: apps !== null && apps.length === 0,
              submenu: apps === null ? [{ label: "…", disabled: true }] : apps.length ? apps : undefined,
            },
            { label: S.share, icon: Share, onSelect: () => void bridge.files.share(ids) },
            { kind: "separator" },
            { label: S.showInExplorer, icon: FolderOpen, onSelect: () => void bridge.files.reveal(ids) },
          ] satisfies MenuEntry[])
        : []),
    ];
    menu.open(anchor, entries(null));
    if (!files) return;
    void bridge.files.openWithApps(ids).then((apps) => {
      const list = (apps ?? []).map((app): MenuEntry => ({
        label: app.name,
        image: app.icon ?? undefined,
        onSelect: () => void bridge.files.openWith(ids, app.id),
      }));
      menu.open(anchor, entries(list));
    });
  };

  // The menu and tooltip render beside the tile, not in it: React events
  // from a portal still bubble to their parent, and the tile acts on clicks.
  return (
    <>
      <div
        className={`fl-tile${selected ? " is-selected" : ""}${pile ? " is-pile" : ""}`}
        style={{ width: TILE.width, height: TILE.height }}
        role="option"
        aria-selected={selected}
        aria-label={item.title}
        ref={tooltip.anchor}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onClick={onClick}
        onContextMenu={onContextMenu}
        onPointerEnter={tooltip.schedule}
        onPointerLeave={tooltip.hide}
      >
        {pile ? (
          <>
            <span className="fl-plate" style={{ left: 13, top: 12, background: "rgba(255,255,255,0.035)" }} />
            <span className="fl-plate" style={{ left: 10, top: 9, background: "rgba(255,255,255,0.055)" }} />
          </>
        ) : null}
        <span className="fl-well">
          {image.src ? (
            <img className={image.thumbnail ? "is-thumbnail" : "is-icon"} src={image.src} alt="" draggable={false} />
          ) : (
            <TileGlyph item={item} />
          )}
        </span>
        {pile ? (
          <>
            <span className="fl-count" aria-hidden="true">
              {leafCount(item)}
            </span>
            <button
              type="button"
              className="fl-expand"
              aria-label={expanded ? S.collapse : S.expand}
              title={expanded ? S.collapse : S.expand}
              aria-expanded={expanded}
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                useFiles.getState().togglePile(item.id);
              }}
            >
              {expanded ? <ChevronDown size={11} strokeWidth={3} /> : <ChevronRight size={11} strokeWidth={3} />}
            </button>
          </>
        ) : null}
        <span className="fl-title">{middleTruncated(item.title)}</span>
        {selected ? (
          <span className="fl-check" style={{ top: pile ? 22 : 4 }} aria-hidden="true">
            <Check size={10} strokeWidth={3.5} />
          </span>
        ) : null}
        {item.pinned ? (
          <span className="fl-pin" aria-label={S.pinned}>
            <Pin size={9} strokeWidth={3} />
          </span>
        ) : null}
        <button
          type="button"
          className="fl-remove"
          aria-label={S.remove}
          title={S.remove}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            tooltip.hide();
            void bridge.files.remove([item.id]);
          }}
        >
          <X size={10} strokeWidth={3} />
        </button>
      </div>
      {menu.menu}
      {tooltip.view}
    </>
  );
}

/** The hover popover (ShelfTooltipPopover): shows after a moment, below the tile when it fits. */
function useTooltip(item: ShelfItem) {
  const anchor = useRef<HTMLDivElement>(null);
  const [text, setText] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const hide = () => {
    clearTimeout(timer.current);
    setText(null);
  };
  const schedule = () => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      if (item.kind === "file" && item.path) {
        void bridge.files.kind(item.path).then((kind) => setText(tooltipText(item, kind)));
      } else setText(tooltipText(item));
    }, 600);
  };
  useEffect(() => () => clearTimeout(timer.current), []);
  const view = text ? <Tooltip text={text} anchor={anchor.current} /> : null;
  return { anchor, schedule, hide, view };
}

function Tooltip(props: { text: string; anchor: HTMLElement | null }) {
  useLocale();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    const at = props.anchor?.getBoundingClientRect();
    if (!el || !at) return;
    const frame = document.querySelector(".island-frame")?.getBoundingClientRect();
    const bounds = frame ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight);
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    let top = at.bottom + 6;
    if (top + h > bounds.bottom - 6) top = Math.max(bounds.top + 6, at.top - 6 - h);
    const left = Math.min(Math.max(bounds.left + 6, at.left + at.width / 2 - w / 2), bounds.right - 6 - w);
    setPos({ left, top });
  }, [props.text, props.anchor]);
  return createPortal(
    <div
      ref={ref}
      className="fl-tooltip"
      role="tooltip"
      style={pos ? { left: pos.left, top: pos.top } : { left: -9999, top: -9999 }}
    >
      {props.text}
    </div>,
    document.body,
  );
}
