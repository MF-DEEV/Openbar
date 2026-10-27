// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Menus drawn by the page itself (the island window has no native menus that
// stay above it): a button's dropdown, or a right-click menu. The menu stays
// inside the island's frame so the window's hit region covers it, and Escape
// closes it before anything else.

import { Check, ChevronRight, type LucideIcon } from "lucide-react";
import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { usePageLayer } from "../../island/pageLayer";

export type MenuEntry =
  | {
      kind?: "item";
      label: string;
      icon?: LucideIcon;
      /** A picture in place of the icon (an app icon). */
      image?: string;
      checked?: boolean;
      disabled?: boolean;
      destructive?: boolean;
      detail?: string;
      onSelect?(): void;
      submenu?: MenuEntry[];
    }
  | { kind: "separator" }
  | { kind: "header"; label: string };

export interface MenuAnchor {
  x: number;
  y: number;
  /** Open above/left of the point when there is no room (a button's bottom edge is the usual anchor). */
  alignRight?: boolean;
}

interface MenuState {
  anchor: MenuAnchor;
  entries: MenuEntry[];
}

/** A right-click or button menu. Render `menu` once; call `open` from an event. */
export function useMenu() {
  const [state, setState] = useState<MenuState | null>(null);
  const close = () => setState(null);
  return {
    isOpen: state !== null,
    open(anchor: MenuAnchor, entries: MenuEntry[]) {
      setState({ anchor, entries });
    },
    /** Opens below a button, right-aligned to it. */
    openBelow(element: Element, entries: MenuEntry[], alignRight = true) {
      const r = element.getBoundingClientRect();
      setState({ anchor: { x: alignRight ? r.right : r.left, y: r.bottom + 4, alignRight }, entries });
    },
    close,
    menu: state ? <Menu anchor={state.anchor} entries={state.entries} onClose={close} /> : null,
  };
}

function frameBounds(): DOMRect {
  const frame = document.querySelector(".island-frame");
  return frame?.getBoundingClientRect() ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight);
}

export function Menu(props: { anchor: MenuAnchor; entries: MenuEntry[]; onClose(): void; nested?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const [active, setActive] = useState(-1);
  const [sub, setSub] = useState<{ index: number; anchor: MenuAnchor } | null>(null);
  usePageLayer(!props.nested, props.onClose);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const bounds = frameBounds();
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    let left = props.anchor.alignRight ? props.anchor.x - w : props.anchor.x;
    let top = props.anchor.y;
    if (top + h > bounds.bottom - 8) top = Math.max(bounds.top + 8, bounds.bottom - 8 - h);
    if (left + w > bounds.right - 8) left = bounds.right - 8 - w;
    left = Math.max(bounds.left + 8, left);
    setPos({ left, top });
  }, [props.anchor.x, props.anchor.y, props.anchor.alignRight]);

  useEffect(() => {
    if (props.nested) return;
    const outside = (e: PointerEvent) => {
      if (!(e.target as Element).closest?.(".u-menu")) props.onClose();
    };
    const key = (e: KeyboardEvent) => {
      const items = props.entries;
      const selectable = (i: number) => {
        const it = items[i];
        return it && (it.kind ?? "item") === "item" && !(it as { disabled?: boolean }).disabled;
      };
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        const dir = e.key === "ArrowDown" ? 1 : -1;
        let i = active;
        for (let n = 0; n < items.length; n++) {
          i = (i + dir + items.length) % items.length;
          if (selectable(i)) break;
        }
        setActive(i);
      } else if (e.key === "Enter" && active >= 0) {
        e.preventDefault();
        e.stopPropagation();
        const it = items[active];
        if (it && (it.kind ?? "item") === "item" && !("submenu" in it && it.submenu)) {
          props.onClose();
          (it as { onSelect?(): void }).onSelect?.();
        }
      }
    };
    // A tick later, so the click that opened the menu doesn't close it.
    const t = setTimeout(() => document.addEventListener("pointerdown", outside, true));
    window.addEventListener("keydown", key, true);
    return () => {
      clearTimeout(t);
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("keydown", key, true);
    };
  }, [active, props.entries]);

  const body = (
    <div
      ref={ref}
      role="menu"
      className="u-menu"
      style={pos ? { left: pos.left, top: pos.top } : { left: -9999, top: -9999 }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {props.entries.map((entry, i) => {
        if (entry.kind === "separator") return <div key={i} className="u-menu-separator" role="separator" />;
        if (entry.kind === "header")
          return (
            <div key={i} className="u-menu-header">
              {entry.label}
            </div>
          );
        const Icon = entry.icon;
        return (
          <button
            key={i}
            type="button"
            role={entry.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
            aria-checked={entry.checked}
            disabled={entry.disabled}
            className={`u-menu-item${entry.destructive ? " is-destructive" : ""}${active === i ? " is-active" : ""}`}
            onPointerEnter={(e) => {
              setActive(i);
              if (entry.submenu) {
                const r = e.currentTarget.getBoundingClientRect();
                setSub({ index: i, anchor: { x: r.right + 2, y: r.top - 4 } });
              } else setSub(null);
            }}
            onClick={() => {
              if (entry.submenu) return;
              props.onClose();
              entry.onSelect?.();
            }}
          >
            <span className="u-menu-check">{entry.checked ? <Check size={12} strokeWidth={2.5} /> : null}</span>
            {entry.image ? (
              <img className="u-menu-image" src={entry.image} alt="" />
            ) : Icon ? (
              <Icon className="u-menu-icon" size={13} strokeWidth={2} />
            ) : null}
            <span className="u-menu-label">{entry.label}</span>
            {entry.detail ? <span className="u-menu-detail">{entry.detail}</span> : null}
            {entry.submenu ? <ChevronRight className="u-menu-chevron" size={12} /> : null}
          </button>
        );
      })}
      {sub && props.entries[sub.index] && "submenu" in props.entries[sub.index]! ? (
        <Menu
          nested
          anchor={sub.anchor}
          entries={(props.entries[sub.index] as { submenu: MenuEntry[] }).submenu}
          onClose={props.onClose}
        />
      ) : null}
    </div>
  );
  return props.nested ? body : createPortal(body, document.body);
}

/** A button that opens a dropdown menu below itself. */
export function MenuButton(props: {
  entries: () => MenuEntry[];
  label: string;
  className?: string;
  children: ReactNode;
  alignRight?: boolean;
  disabled?: boolean;
}) {
  const menu = useMenu();
  return (
    <>
      <button
        type="button"
        className={props.className}
        aria-label={props.label}
        title={props.label}
        aria-haspopup="menu"
        aria-expanded={menu.isOpen}
        disabled={props.disabled}
        onClick={(e) =>
          menu.isOpen ? menu.close() : menu.openBelow(e.currentTarget, props.entries(), props.alignRight ?? true)
        }
      >
        {props.children}
      </button>
      {menu.menu}
    </>
  );
}
