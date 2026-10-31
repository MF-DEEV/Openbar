// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// One history entry on the Clipboard page (NotchClipboardView.swift:225-473,
// 597-663): a comfortable card, a compact row, and the full card a compact
// row opens over its neighbours.

import { dateTimeFormat, useLocale, t } from "../i18n";

import { AlignLeft, Check, Copy, File, Folder, Image as ImageIcon, Pin, Trash2 } from "lucide-react";
import {
  type CSSProperties,
  type MouseEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { bridge } from "../shared/bridge";
import { cssColor, parseColor } from "./color";
import { openOffset } from "./keys";
import { dimensions, fileName, highlight, isImageFile } from "./search";
import { C } from "./strings";
import type { ClipboardEntry } from "./types";

/** NotchLayout: card heights and spacing, and an open entry's bounds (NotchClipboardSizeStrings.swift:33-39). */
export const CARD_HEIGHT = 104;
export const COMPACT_HEIGHT = 52;
export const CARD_SPACING = 8;
const OPEN_CHROME = 48;
const OPEN_IMAGE_MIN = 80;
const OPEN_IMAGE_MAX = 170;
/** Physical pixels for thumbnails and icons, fetched once per size from the shell. */
const THUMBNAIL = 384;
const ICON = 32;

export interface CardActions {
  activate(entry: ClipboardEntry): void;
  copy(entry: ClipboardEntry, event: MouseEvent): void;
  pin(entry: ClipboardEntry): void;
  remove(entry: ClipboardEntry): void;
  menu(entry: ClipboardEntry, event: MouseEvent): void;
}

export interface CardProps {
  entry: ClipboardEntry;
  place: number;
  highlighted: boolean;
  /** Under the pointer (compact rows show their buttons then). */
  hovered: boolean;
  copied: boolean;
  /** "Ctrl+n" shows only while the island has the keyboard. */
  keyFocus: boolean;
  tokens: string[];
  actions: CardActions;
}

// MARK: Pictures and icons from the shell

const shellImages = new Map<string, Promise<string | null>>();

/** A file's icon or thumbnail as a data URL: undefined while it loads, null when there is none. */
export function useShellImage(path: string | null, size: number, thumbnail: boolean): string | null | undefined {
  const key = path ? `${thumbnail ? "t" : "i"}|${size}|${path}` : "";
  const [image, setImage] = useState<{ key: string; url: string | null } | null>(null);
  useEffect(() => {
    if (!path) return;
    // Designing in a browser: the demo hands pictures over as data URLs.
    if (path.startsWith("data:")) {
      setImage({ key, url: path });
      return;
    }
    let request = shellImages.get(key);
    if (!request) {
      request = bridge.shell.icon(path, size, thumbnail).then((url) => url ?? null);
      // Entries come and go over days; a full cache starts again.
      if (shellImages.size >= 256) shellImages.clear();
      shellImages.set(key, request);
    }
    let live = true;
    void request.then((url) => live && setImage({ key, url }));
    return () => {
      live = false;
    };
  }, [key]);
  if (!path) return null;
  return image?.key === key ? image.url : undefined;
}

// MARK: Pieces

/** 28 × 28 like the island's icon buttons; the pressed and hover looks come from n-icon-button. */
export function CardButton(props: {
  label: string;
  onClick(event: MouseEvent): void;
  children: ReactNode;
  disabled?: boolean;
  selected?: boolean;
}) {
  useLocale();
  return (
    <button
      type="button"
      className={`n-icon-button cb-button${props.selected ? " is-selected" : ""}`}
      aria-label={props.label}
      title={props.label}
      disabled={props.disabled}
      onClick={(event) => {
        event.stopPropagation();
        props.onClick(event);
      }}
    >
      {props.children}
    </button>
  );
}

function KindGlyph({ entry }: { entry: ClipboardEntry }) {
  useLocale();
  const Icon = entry.kind === "image" ? ImageIcon : entry.kind === "files" ? File : AlignLeft;
  return <Icon className="cb-glyph" size={10} strokeWidth={2} aria-hidden="true" />;
}

/** The app the entry came from, or the kind of entry when that is unknown. */
function SourceBadge({ entry }: { entry: ClipboardEntry }) {
  useLocale();
  const icon = useShellImage(entry.sourceExe, ICON, false);
  if (entry.sourceExe && icon) {
    const name = entry.sourceName ?? fileName(entry.sourceExe);
    return <img className="cb-source" src={icon} alt={name} title={name} />;
  }
  return <KindGlyph entry={entry} />;
}

/** The files' own icon or the kind's glyph, one 14 pt column so the rows' text lines up. */
function LeadingIcon({ entry }: { entry: ClipboardEntry }) {
  useLocale();
  const icon = useShellImage(entry.kind === "files" ? (entry.filePaths[0] ?? null) : null, ICON, false);
  return (
    <span className="cb-leading" aria-hidden="true">
      {icon ? <img src={icon} alt="" /> : <KindGlyph entry={entry} />}
    </span>
  );
}

function Highlighted({ text, tokens }: { text: string; tokens: string[] }) {
  useLocale();
  return (
    <>
      {highlight(text, tokens).map((segment, i) =>
        segment.match ? <b key={i}>{segment.text}</b> : <span key={i}>{segment.text}</span>,
      )}
    </>
  );
}

/** A picture rounded at its own edges, in the room it may take. */
function Thumbnail(props: { path: string; label: string; compact: boolean }) {
  useLocale();
  const url = useShellImage(props.path, THUMBNAIL, true);
  const [failed, setFailed] = useState(false);
  if (url === null || failed) return <span className="cb-text">{props.label}</span>;
  return (
    <span className={`cb-thumb${props.compact ? " is-compact" : ""}`} title={props.label}>
      {url ? <img src={url} alt={props.label} onError={() => setFailed(true)} /> : null}
    </span>
  );
}

/** A long file name keeps its start and its extension, cut in the middle. */
function MiddleName({ name, tokens }: { name: string; tokens: string[] }) {
  useLocale();
  const chars = Array.from(name);
  const tail = chars.slice(-Math.min(10, Math.floor(chars.length / 2))).join("");
  const head = chars.slice(0, chars.length - Array.from(tail).length).join("");
  return (
    <span className="cb-middle">
      <span className="cb-middle-head">
        <Highlighted text={head} tokens={tokens} />
      </span>
      <span className="cb-middle-tail">
        <Highlighted text={tail} tokens={tokens} />
      </span>
    </span>
  );
}

const MAX_OPEN_LINES = 8;

function Preview(props: { entry: ClipboardEntry; compact: boolean; open?: boolean; tokens: string[] }) {
  useLocale();
  const { entry, compact, open, tokens } = props;
  if (entry.kind === "image") {
    const label = `${C.imageEntryLabel} · ${dimensions(entry)}`;
    return entry.imagePath ? (
      <Thumbnail path={entry.imagePath} label={label} compact={compact} />
    ) : (
      <span className="cb-text">
        <Highlighted text={label} tokens={tokens} />
      </span>
    );
  }
  if (entry.kind === "files") {
    const single = entry.filePaths.length === 1 ? entry.filePaths[0]! : null;
    if (single && isImageFile(single)) return <Thumbnail path={single} label={fileName(single)} compact={compact} />;
    // A count of several files is no text the search reads.
    const name = single ? fileName(single) : C.fileCount(entry.filePaths.length);
    const matched = single ? tokens : [];
    return (
      <span className={`cb-files${compact ? " is-compact" : ""}`} title={entry.filePaths.join("\n")}>
        {compact ? null : open ? <LeadingIcon entry={entry} /> : <Folder size={12} aria-hidden="true" />}
        {compact ? (
          <MiddleName name={name} tokens={matched} />
        ) : (
          <span className="cb-files-name">
            <Highlighted text={name} tokens={matched} />
          </span>
        )}
      </span>
    );
  }
  const color = parseColor(entry.text);
  const text = open
    ? Array.from(entry.cardPreview)
        .slice(0, MAX_OPEN_LINES * 120)
        .join("")
    : entry.preview;
  return (
    <span className="cb-text-row">
      {color ? <span className="cb-swatch" style={{ background: cssColor(color) }} aria-hidden="true" /> : null}
      <span className={`cb-text ${compact ? "is-two" : open ? "is-open" : "is-three"}`}>
        <Highlighted text={text} tokens={tokens} />
      </span>
    </span>
  );
}

const dateFormat = () =>
  dateTimeFormat({
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
const timeFormat = () => dateTimeFormat({ hour: "2-digit", minute: "2-digit" });

function Buttons(props: CardProps) {
  useLocale();
  const { entry, copied, actions } = props;
  return (
    <>
      <CardButton label={copied ? C.copied : C.copy} onClick={(e) => actions.copy(entry, e)}>
        {copied ? <Check size={14} strokeWidth={2} /> : <Copy size={14} strokeWidth={2} />}
      </CardButton>
      <CardButton label={entry.pinned ? C.unpin : C.pin} onClick={() => actions.pin(entry)}>
        <Pin size={14} strokeWidth={2} fill={entry.pinned ? "currentColor" : "none"} />
      </CardButton>
      <CardButton label={C.delete} onClick={() => actions.remove(entry)}>
        <Trash2 size={14} strokeWidth={2} />
      </CardButton>
    </>
  );
}

function Badge({ place }: { place: number }) {
  useLocale();
  return (
    <span className="cb-badge" aria-hidden="true">
      {C.shortcut(place + 1)}
    </span>
  );
}

/** The whole entry, its actions in the bottom row. */
function FullContent(props: CardProps & { open?: boolean }) {
  useLocale();
  const { entry, place, keyFocus, actions } = props;
  return (
    <div className="cb-full">
      <button
        type="button"
        className="cb-hit"
        title={C.paste}
        aria-label={`${C.paste}: ${entry.preview.slice(0, 80)}`}
        onClick={() => actions.activate(entry)}
      >
        <Preview entry={entry} compact={false} open={props.open} tokens={props.tokens} />
      </button>
      <div className="cb-actions">
        <SourceBadge entry={entry} />
        <span className="cb-date">{dateFormat().format(entry.copiedAt)}</span>
        <span className="cb-spacer" />
        {place < 9 && keyFocus ? <Badge place={place} /> : null}
        <Buttons {...props} />
      </div>
    </div>
  );
}

function surfaceClass(props: CardProps, extra = "") {
  return `cb-card${props.entry.pinned ? t("windows.clipboard.isPinned") : ""}${props.highlighted ? t("windows.clipboard.isHighlighted") : ""}${extra}`;
}

export function ComfortableCard(props: CardProps) {
  useLocale();
  return (
    <div className={surfaceClass(props)} onContextMenu={(e) => props.actions.menu(props.entry, e)}>
      <FullContent {...props} />
    </div>
  );
}

/** One or two lines, with the buttons only for the entry in use. */
export function CompactCard(props: CardProps & { covered: boolean }) {
  useLocale();
  const { entry, place, keyFocus, actions } = props;
  const active = props.hovered || props.highlighted;
  return (
    <div
      className={surfaceClass(props, " is-compact")}
      aria-hidden={props.covered || undefined}
      onContextMenu={(e) => actions.menu(entry, e)}
    >
      <button
        type="button"
        className="cb-hit cb-compact-hit"
        title={C.paste}
        aria-label={`${C.paste}: ${entry.preview.slice(0, 80)}`}
        onClick={() => actions.activate(entry)}
      >
        <LeadingIcon entry={entry} />
        <Preview entry={entry} compact tokens={props.tokens} />
      </button>
      {active ? (
        <span className="cb-compact-buttons">
          <Buttons {...props} />
        </span>
      ) : (
        <span className="cb-compact-meta">
          {entry.pinned ? <Pin size={9} fill="currentColor" aria-hidden="true" /> : null}
          {place < 9 && keyFocus ? <Badge place={place} /> : null}
          <span>{timeFormat().format(entry.copiedAt)}</span>
        </span>
      )}
    </div>
  );
}

/** An open entry is as tall as its picture asks, within a range; text measures itself. */
export function openHeight(entry: ClipboardEntry, width: number): number | null {
  if (entry.kind === "files") return CARD_HEIGHT;
  if (entry.kind === "image") {
    const aspect = entry.imageWidth && entry.imageHeight ? entry.imageWidth / entry.imageHeight : 1.5;
    const natural = Math.max(1, width - 40) / Math.max(0.2, aspect);
    return Math.min(OPEN_IMAGE_MAX, Math.max(OPEN_IMAGE_MIN, natural)) + OPEN_CHROME;
  }
  return null;
}

/**
 * The open entry, from its row's top and moved only as far as it takes to
 * show all of it in the list, lifted over the rows it covers with a soft
 * shadow. It follows the list's scrolling (NotchSupport.clipboardOpenOffset).
 */
export function OpenCard(
  props: CardProps & { list: HTMLDivElement | null; row: HTMLDivElement | null; width: number },
) {
  useLocale();
  const { list, row } = props;
  const card = useRef<HTMLDivElement>(null);
  const [offset, setOffset] = useState(0);
  const fixed = openHeight(props.entry, props.width);
  const visible = list?.clientHeight ?? 0;

  useLayoutEffect(() => {
    if (!list || !row) return;
    const place = () => {
      const height = card.current?.offsetHeight ?? 0;
      setOffset(openOffset(row.offsetTop - list.scrollTop, row.offsetHeight, height, list.clientHeight));
    };
    place();
    list.addEventListener("scroll", place, { passive: true });
    return () => list.removeEventListener("scroll", place);
  }, [list, row, props.entry.id, fixed]);

  const style: CSSProperties = {
    top: offset,
    height: fixed === null ? undefined : Math.min(fixed, visible || fixed),
    minHeight: fixed === null ? COMPACT_HEIGHT : undefined,
    maxHeight: visible || undefined,
  };
  return (
    <div
      ref={card}
      className={surfaceClass(props, fixed === null ? " is-lifted" : " is-lifted is-sized")}
      style={style}
      role="group"
      aria-label={props.entry.preview.slice(0, 80)}
      onContextMenu={(e) => props.actions.menu(props.entry, e)}
    >
      <FullContent {...props} open />
    </div>
  );
}
