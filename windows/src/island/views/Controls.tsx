// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The island's small shared controls (NotchComponents.swift; spec 01-shell §1.8).

import { useLocale } from "../../i18n";

import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

/** 28 × 28, radius 9, glyph 12 medium at 55% (white on a 12% wash when selected). */
export function IconButton(props: {
  icon: LucideIcon;
  label: string;
  selected?: boolean;
  disabled?: boolean;
  onClick?: (event: React.MouseEvent<HTMLButtonElement>) => void;
}) {
  useLocale();
  const Icon = props.icon;
  return (
    <button
      type="button"
      className={`n-icon-button${props.selected ? " is-selected" : ""}`}
      aria-label={props.label}
      title={props.label}
      aria-pressed={props.selected}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      <Icon size={14} strokeWidth={2} />
    </button>
  );
}

/** Height 28 capsule with a 14% wash when prominent. */
export function PillButton(props: { children: ReactNode; prominent?: boolean; onClick?: () => void; label?: string }) {
  useLocale();
  return (
    <button
      type="button"
      className={`n-pill-button${props.prominent ? " is-prominent" : ""}`}
      aria-label={props.label}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

/** The empty-state glyph: 56 × 56, radius 18, 5% wash, 26 pt light icon at 65%. */
export function EmptyState(props: { icon: LucideIcon; title: string; detail?: string }) {
  useLocale();
  const Icon = props.icon;
  return (
    <div className="n-empty">
      <div className="n-empty-glyph">
        <Icon size={26} strokeWidth={1.5} />
      </div>
      <div className="n-empty-title">{props.title}</div>
      {props.detail ? <div className="n-empty-detail">{props.detail}</div> : null}
    </div>
  );
}
