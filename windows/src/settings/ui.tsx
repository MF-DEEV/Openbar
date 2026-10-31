// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t, useLocale } from "../i18n";

// The settings window's building blocks, after Windows 11's own Settings and
// PowerToys: a section header, cards that pair a title and caption with one
// control on the right, and cards that expand into their sub-options (one
// level deep, never nested).

import { ChevronDown, type LucideIcon } from "lucide-react";
import { type ReactNode, useId, useState } from "react";
import type { Option } from "./Select";
export { Select } from "./Select";
export type { Option } from "./Select";

export function Page(props: { title: string; lead?: string; children: ReactNode }) {
  useLocale();
  return (
    <section className="st-page" aria-labelledby="st-page-title">
      <h1 id="st-page-title" className="st-page-title">
        {props.title}
      </h1>
      {props.lead ? <p className="st-page-lead">{props.lead}</p> : null}
      {props.children}
    </section>
  );
}

export function Group(props: { title?: string; children: ReactNode }) {
  useLocale();
  return (
    <div className="st-group">
      {props.title ? <h2 className="st-group-title">{props.title}</h2> : null}
      <div className="st-group-cards">{props.children}</div>
    </div>
  );
}

interface RowProps {
  icon?: LucideIcon;
  title: string;
  caption?: ReactNode;
  children?: ReactNode;
  disabled?: boolean;
  /** For the control: its accessible name comes from the title. */
  id?: string;
}

/** One setting: icon, title and caption on the left, its control on the right. */
export function Row({ icon: Icon, title, caption, children, disabled, id }: RowProps) {
  useLocale();
  const auto = useId();
  const labelId = id ?? auto;
  return (
    <div className={`st-card${disabled ? " is-disabled" : ""}`} role="group" aria-labelledby={labelId}>
      {Icon ? <Icon className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden /> : null}
      <div className="st-card-text">
        <div className="st-card-title" id={labelId}>
          {title}
        </div>
        {caption ? <div className="st-card-caption">{caption}</div> : null}
      </div>
      {children ? <div className="st-card-control">{children}</div> : null}
    </div>
  );
}

/** A card whose header toggles a list of sub-rows beneath it. */
export function Expander(props: {
  icon?: LucideIcon;
  title: string;
  caption?: ReactNode;
  control?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  useLocale();
  const [open, setOpen] = useState(props.defaultOpen ?? false);
  const bodyId = useId();
  const Icon = props.icon;
  return (
    <div className={`st-expander${open ? " is-open" : ""}`}>
      <div className="st-card st-expander-head">
        <button
          type="button"
          className="st-expander-toggle"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={() => setOpen((o) => !o)}
        >
          {Icon ? <Icon className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden /> : null}
          <span className="st-card-text">
            <span className="st-card-title">{props.title}</span>
            {props.caption ? <span className="st-card-caption">{props.caption}</span> : null}
          </span>
        </button>
        {props.control ? <div className="st-card-control">{props.control}</div> : null}
        <button
          type="button"
          className="st-expander-chevron"
          aria-label={props.title}
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={() => setOpen((o) => !o)}
        >
          <ChevronDown size={16} aria-hidden />
        </button>
      </div>
      <div className="st-expander-body" id={bodyId} hidden={!open}>
        {props.children}
      </div>
    </div>
  );
}

/** A row inside an expander: no card of its own. */
export function SubRow(props: { title: string; caption?: ReactNode; children?: ReactNode; disabled?: boolean }) {
  useLocale();
  const id = useId();
  return (
    <div className={`st-subrow${props.disabled ? " is-disabled" : ""}`} role="group" aria-labelledby={id}>
      <div className="st-card-text">
        <div className="st-card-title" id={id}>
          {props.title}
        </div>
        {props.caption ? <div className="st-card-caption">{props.caption}</div> : null}
      </div>
      {props.children ? <div className="st-card-control">{props.children}</div> : null}
    </div>
  );
}

/** Windows' toggle switch, with its On/Off word on the left. */
export function Toggle(props: {
  checked: boolean;
  onChange: (on: boolean) => void;
  label: string;
  disabled?: boolean;
  onText?: string;
  offText?: string;
}) {
  useLocale();
  return (
    <label className={`st-toggle${props.disabled ? " is-disabled" : ""}`}>
      <span className="st-toggle-state" aria-hidden>
        {props.checked ? (props.onText ?? t("commandBar.stateOn")) : (props.offText ?? t("notchAgents.off"))}
      </span>
      <input
        type="checkbox"
        role="switch"
        checked={props.checked}
        disabled={props.disabled}
        aria-label={props.label}
        onChange={(e) => props.onChange(e.target.checked)}
      />
      <span className="st-toggle-track" aria-hidden>
        <span className="st-toggle-thumb" />
      </span>
    </label>
  );
}

/** A few mutually exclusive choices shown side by side. */
export function Segmented<T extends string>(props: {
  value: T;
  options: Option<T>[];
  onChange: (v: T) => void;
  label: string;
  disabled?: boolean;
}) {
  useLocale();
  return (
    <div className="st-segmented" role="radiogroup" aria-label={props.label}>
      {props.options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === props.value}
          disabled={props.disabled}
          className={o.value === props.value ? "is-selected" : undefined}
          onClick={() => props.onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Slider(props: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  label: string;
  format?: (v: number) => string;
  disabled?: boolean;
}) {
  useLocale();
  const fraction = (props.value - props.min) / (props.max - props.min);
  return (
    <span className="st-slider">
      <span className="st-slider-value" aria-hidden>
        {props.format ? props.format(props.value) : props.value}
      </span>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        disabled={props.disabled}
        aria-label={props.label}
        aria-valuetext={props.format?.(props.value)}
        style={{ "--fill": `${Math.max(0, Math.min(1, fraction)) * 100}%` } as React.CSSProperties}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
    </span>
  );
}

export function Button(props: {
  children: ReactNode;
  onClick: () => void;
  accent?: boolean;
  disabled?: boolean;
  busy?: boolean;
}) {
  useLocale();
  return (
    <button
      type="button"
      className={`st-button${props.accent ? " is-accent" : ""}`}
      disabled={props.disabled || props.busy}
      aria-busy={props.busy || undefined}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

/** A short status word with a colored dot; the word carries the meaning. */
export function Status(props: { tone: "ok" | "warn" | "off"; children: ReactNode }) {
  useLocale();
  return (
    <span className={`st-status is-${props.tone}`}>
      <span className="st-status-dot" aria-hidden />
      {props.children}
    </span>
  );
}
