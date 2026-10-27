// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Clipboard page (NotchClipboardView.swift; spec 05 §3): the history as a
// list of cards, a search field that shows on the first key typed, the
// arrows, Enter and Ctrl+1–9, and in the top row the magnifier, the pinned
// filter and Clear unpinned. Activating an entry pastes it into the app the
// island came from; an elevated app gets it copied instead.

import { ClipboardList, LayoutGrid, ListFilter, Pin, Search, Trash2 } from "lucide-react";
import { type MouseEvent, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from "react";
import { t } from "../i18n";
import { usePageLayer } from "../island/pageLayer";
import { useIsland } from "../island/store";
import type { PageProps } from "../island/views/Expanded";
import { EmptyState, IconButton } from "../island/views/Controls";
import { bridge } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import { useMenu } from "../shared/ui/Menu";
import {
  CARD_HEIGHT,
  CARD_SPACING,
  type CardActions,
  CardButton,
  ComfortableCard,
  COMPACT_HEIGHT,
  CompactCard,
  OpenCard,
} from "./Cards";
import { Dwell } from "./dwell";
import { clipboardKey, highlightFor, openingScroll, pastePlace, pasteTarget, steppedItem } from "./keys";
import { filterEntries, searchTokens } from "./search";
import { useClipboard } from "./store";
import { C } from "./strings";
import type { ClipboardEntry } from "./types";
import "./clipboard.css";

const SEARCH_HEIGHT = 36;
const ROW_SPACING = 10;
const COPIED_MS = 1600;

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Whether `entry` has a neighbour in its own group (pinned or recent) to swap with. */
function canMove(all: ClipboardEntry[], entry: ClipboardEntry, up: boolean): boolean {
  const group = all.filter((e) => e.pinned === entry.pinned);
  const at = group.findIndex((e) => e.id === entry.id);
  return at >= 0 && (up ? at > 0 : at < group.length - 1);
}

export function ClipboardPage({ width }: PageProps) {
  const all = useClipboard((s) => s.entries);
  const pinnedOnly = useClipboard((s) => s.pinnedOnly);
  const searchOpen = useClipboard((s) => s.searchOpen);
  const searchRequest = useClipboard((s) => s.searchRequest);
  const confirmClear = useClipboard((s) => s.confirmClear);
  const enabled = useIsland((s) => s.settings.clipboardHistoryEnabled);
  // Any value but "compact" reads as comfortable, as on the Mac.
  const comfortable = useIsland((s) => s.settings.clipboardCardSize) !== "compact";
  const [query, setQuery] = useState("");
  const [highlighted, setHighlighted] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [focused, setFocused] = useState(() => document.hasFocus());
  const [listElement, setListElement] = useState<HTMLDivElement | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const rows = useRef(new Map<string, HTMLDivElement>());
  const keysMoved = useRef(false);
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const dwellRef = useRef<Dwell | null>(null);
  dwellRef.current ??= new Dwell(redraw);
  const dwell = dwellRef.current;
  const menu = useMenu();

  const entries = useMemo(
    () => filterEntries(all, query).filter((e) => !pinnedOnly || e.pinned),
    [all, query, pinnedOnly],
  );
  const ids = entries.map((e) => e.id).join("|");
  const tokens = useMemo(() => searchTokens(query), [query]);
  const rowHeight = comfortable ? CARD_HEIGHT : COMPACT_HEIGHT;
  const canReorder = query.trim() === "";

  // MARK: Highlight

  // A new search or filter starts from its top result; a row that leaves the
  // list hands the highlight on (NotchClipboardView.swift:136, 152, 205-209).
  const lastFilter = useRef({ query, pinnedOnly });
  useEffect(() => {
    const fresh = lastFilter.current.query !== query || lastFilter.current.pinnedOnly !== pinnedOnly;
    lastFilter.current = { query, pinnedOnly };
    if (fresh) keysMoved.current = false;
    setHighlighted((current) => highlightFor(fresh ? null : current, entries, query));
  }, [ids, query, pinnedOnly]);

  // The list follows the highlight the least that shows it whole. Opening
  // keeps the pinned entries above the highlight in view, moving by whole
  // rows only while less than half a compact row of it would show.
  const previousHighlight = useRef<string | null>(null);
  useLayoutEffect(() => {
    const previous = previousHighlight.current;
    previousHighlight.current = highlighted;
    const list = listElement;
    if (!highlighted || !list) return;
    if (previous === null) {
      const index = entries.findIndex((e) => e.id === highlighted);
      const skip = openingScroll(index, rowHeight, CARD_SPACING, COMPACT_HEIGHT / 2, list.clientHeight);
      if (skip > 0) list.scrollTop = skip * (rowHeight + CARD_SPACING);
      return;
    }
    rows.current.get(highlighted)?.scrollIntoView({ block: "nearest", behavior: reducedMotion() ? "auto" : "smooth" });
  }, [highlighted, listElement]);

  // Resting on an entry with the arrow keys opens it, as resting the pointer does.
  useEffect(() => {
    dwell.highlight(comfortable ? null : highlighted, keysMoved.current);
  }, [highlighted, comfortable]);
  useEffect(() => () => dwell.dispose(), []);

  // MARK: Search

  // The page opens with the field hidden but focused, so the first key goes in.
  useEffect(() => {
    useClipboard.getState().setSearchOpen(false);
    input.current?.focus();
    return () => useClipboard.setState({ searchOpen: false, confirmClear: null });
  }, []);
  useEffect(() => {
    if (searchRequest > 0) input.current?.focus();
  }, [searchRequest]);

  const closeSearch = () => {
    setQuery("");
    useClipboard.getState().setSearchOpen(false);
    input.current?.focus();
  };
  // Escape empties and hides the search before it closes the island.
  usePageLayer(searchOpen, closeSearch);

  // MARK: Copied

  // The tick answers one copy; the copied entry moves to the top, and the list follows it.
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(null), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copied]);
  useEffect(() => {
    if (copied)
      rows.current.get(copied)?.scrollIntoView({ block: "nearest", behavior: reducedMotion() ? "auto" : "smooth" });
  }, [copied, ids]);

  // The "Ctrl+n" badges show only while the island has the keyboard.
  useEffect(() => {
    const on = () => setFocused(true);
    const off = () => setFocused(false);
    window.addEventListener("focus", on);
    window.addEventListener("blur", off);
    return () => {
      window.removeEventListener("focus", on);
      window.removeEventListener("blur", off);
    };
  }, []);

  // MARK: Actions

  async function paste(entry: ClipboardEntry) {
    const result = await bridge.clipboard.paste(entry.id);
    if (result === "copied") setCopied(entry.id);
    else if (result === "failed") void bridge.clipboard.beep();
    else useIsland.getState().collapse();
  }

  async function copy(entry: ClipboardEntry, event?: MouseEvent) {
    // A copied entry moves to the top, so a double click's second click
    // would copy whichever entry took its place.
    if (event && event.detail > 1) return;
    const ok = await bridge.clipboard.copy(entry.id);
    if (ok === false) void bridge.clipboard.beep();
    else setCopied(entry.id);
  }

  function remove(entry: ClipboardEntry) {
    if (copied === entry.id) setCopied(null);
    void bridge.clipboard.remove(entry.id);
  }

  function move(entry: ClipboardEntry, up: boolean) {
    if (canReorder && canMove(all, entry, up)) void bridge.clipboard.move(entry.id, up);
  }

  const actions: CardActions = {
    activate: (entry) => void paste(entry),
    copy: (entry, event) => void copy(entry, event),
    pin: (entry) => void bridge.clipboard.togglePin(entry.id),
    remove,
    menu(entry, event) {
      event.preventDefault();
      menu.open({ x: event.clientX, y: event.clientY }, [
        { label: C.paste, onSelect: () => void paste(entry) },
        { label: C.copy, onSelect: () => void copy(entry) },
        { kind: "separator" },
        { label: entry.pinned ? C.unpin : C.pin, onSelect: () => void bridge.clipboard.togglePin(entry.id) },
        { label: C.moveUp, disabled: !canReorder || !canMove(all, entry, true), onSelect: () => move(entry, true) },
        {
          label: C.moveDown,
          disabled: !canReorder || !canMove(all, entry, false),
          onSelect: () => move(entry, false),
        },
        { kind: "separator" },
        { label: C.delete, destructive: true, onSelect: () => remove(entry) },
      ]);
    },
  };

  // MARK: Keys

  // Read on the way back up, after the field and any open menu or sheet:
  // a menu's own arrows stop there, and Escape belongs to the island.
  const onKey = useRef<(e: KeyboardEvent) => void>(() => {});
  onKey.current = (e) => {
    if (e.defaultPrevented) return;
    if (confirmClear || menu.isOpen) {
      // Typing over a menu or the sheet goes nowhere, not into the hidden field.
      if (clipboardKey(e)?.kind === "reveal" && document.activeElement === input.current) e.preventDefault();
      return;
    }
    const place = pastePlace(e);
    if (place !== null) {
      e.preventDefault();
      const entry = entries[place];
      if (entry) void paste(entry);
      else void bridge.clipboard.beep();
      return;
    }
    const key = clipboardKey(e);
    if (!key) return;
    if (key.kind === "move") {
      if (entries.length === 0) return;
      e.preventDefault();
      keysMoved.current = true;
      setHighlighted((current) =>
        steppedItem(
          current,
          entries.map((x) => x.id),
          key.backwards,
        ),
      );
    } else if (key.kind === "paste") {
      const id = pasteTarget(
        highlighted,
        entries.map((x) => x.id),
      );
      const entry = entries.find((x) => x.id === id);
      if (!entry) return;
      e.preventDefault();
      void paste(entry);
    } else {
      // Typing shows the field; focus taken elsewhere comes back for this key.
      useClipboard.getState().setSearchOpen(true);
      if (document.activeElement !== input.current) input.current?.focus();
    }
  };
  useEffect(() => {
    const listener = (e: KeyboardEvent) => onKey.current(e);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  // MARK: Layout

  let body;
  if (!enabled && all.length === 0) {
    // Says why it is empty and turns the history on from here.
    body = (
      <div className="cb-empty">
        <EmptyState icon={ClipboardList} title={C.disabled} />
        <button
          type="button"
          className="cb-enable"
          onClick={() => void setPrefs({ island: { clipboardHistoryEnabled: true } })}
        >
          {C.enable}
        </button>
      </div>
    );
  } else if (entries.length === 0) {
    body = (
      <div className="cb-empty">
        <EmptyState
          icon={pinnedOnly ? Pin : ClipboardList}
          title={query === "" && !pinnedOnly ? C.empty : C.noResults}
        />
      </div>
    );
  } else {
    const opened = comfortable ? null : dwell.opened;
    body = (
      <div
        ref={setListElement}
        className="cb-list"
        role="list"
        aria-label={C.title}
        // The search field keeps the keyboard while the cards are clicked.
        onMouseDown={(e) => e.preventDefault()}
      >
        {entries.map((entry, place) => {
          const props = {
            entry,
            place,
            highlighted: highlighted === entry.id,
            hovered: dwell.hovered === entry.id,
            copied: copied === entry.id,
            keyFocus: focused,
            tokens,
            actions,
          };
          const open = opened === entry.id;
          return (
            <div
              key={entry.id}
              role="listitem"
              ref={(el) => {
                if (el) rows.current.set(entry.id, el);
                else rows.current.delete(entry.id);
              }}
              className={`cb-row${open ? " is-open" : ""}`}
              style={{ height: rowHeight }}
              onPointerEnter={() => !comfortable && dwell.enter(entry.id)}
              onPointerLeave={() => !comfortable && dwell.leave(entry.id)}
            >
              {comfortable ? <ComfortableCard {...props} /> : <CompactCard {...props} covered={open} />}
              {open ? (
                <OpenCard {...props} list={listElement} row={rows.current.get(entry.id) ?? null} width={width} />
              ) : null}
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <div className="cb-page" style={{ gap: searchOpen ? ROW_SPACING : 0 }}>
      <label
        className={`cb-search${searchOpen ? " is-open" : ""}`}
        style={{ height: searchOpen ? SEARCH_HEIGHT : 0 }}
        aria-hidden={!searchOpen || undefined}
      >
        <Search size={13} strokeWidth={2} aria-hidden="true" />
        <input
          ref={input}
          value={query}
          placeholder={C.search}
          aria-label={C.search}
          tabIndex={searchOpen ? 0 : -1}
          spellCheck={false}
          onChange={(e) => {
            setQuery(e.target.value);
            // Text that reaches the hidden field, pasted into it too, shows it.
            if (e.target.value !== "") useClipboard.getState().setSearchOpen(true);
          }}
          onBlur={() => {
            // A field left empty has nothing to keep open.
            if (query.trim() === "") useClipboard.getState().setSearchOpen(false);
          }}
        />
      </label>
      {body}
      {confirmClear ? (
        <ClearSheet count={Object.keys(confirmClear).length} onClosed={() => input.current?.focus()} />
      ) : null}
      {menu.menu}
    </div>
  );
}

/** "Clear unpinned (n)?" inside the island: the island is topmost, a dialog would open behind it. */
function ClearSheet(props: { count: number; onClosed(): void }) {
  const answer = (clear: boolean) => {
    useClipboard.getState().answerClearRecent(clear);
    props.onClosed();
  };
  usePageLayer(true, () => answer(false));
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  return (
    <div className="cb-sheet-backdrop" onMouseDown={(e) => e.target === e.currentTarget && answer(false)}>
      <div className="cb-sheet" role="alertdialog" aria-labelledby="cb-sheet-title" aria-describedby="cb-sheet-message">
        <div className="cb-sheet-title" id="cb-sheet-title">
          {C.clearRecentConfirm(props.count)}
        </div>
        <div className="cb-sheet-message" id="cb-sheet-message">
          {C.clearRecentConfirmMessage}
        </div>
        <div className="cb-sheet-buttons">
          <button ref={cancel} type="button" className="n-pill-button" onClick={() => answer(false)}>
            {C.cancel}
          </button>
          <button type="button" className="n-pill-button is-prominent cb-destructive" onClick={() => answer(true)}>
            {C.clearRecent}
          </button>
        </div>
      </div>
    </div>
  );
}

/** The top row's leading side: the sections button where the island has no Explore circle, and the magnifier. */
export function ClipboardLeading() {
  const searchOpen = useClipboard((s) => s.searchOpen);
  const hasExploreBubble = useIsland((s) => s.settings.quickAccess.some((b) => b.action.kind === "explore"));
  return (
    <>
      {hasExploreBubble ? null : (
        <IconButton
          icon={LayoutGrid}
          label={t("island.sections")}
          onClick={() => useIsland.getState().toggleSections()}
        />
      )}
      {searchOpen ? null : (
        <IconButton icon={Search} label={C.search} onClick={() => useClipboard.getState().requestSearch()} />
      )}
    </>
  );
}

/** The pinned filter and Clear unpinned. The history window isn't on Windows, so its button isn't either. */
export function ClipboardTrailing() {
  const pinnedOnly = useClipboard((s) => s.pinnedOnly);
  const hasRecent = useClipboard((s) => s.entries.some((e) => !e.pinned));
  return (
    <>
      <CardButton label={C.pinned} selected={pinnedOnly} onClick={() => useClipboard.getState().togglePinnedOnly()}>
        <ListFilter size={14} strokeWidth={pinnedOnly ? 2.6 : 2} />
      </CardButton>
      <CardButton label={C.clearRecent} disabled={!hasRecent} onClick={() => useClipboard.getState().askClearRecent()}>
        <Trash2 size={14} strokeWidth={2} />
      </CardButton>
    </>
  );
}
