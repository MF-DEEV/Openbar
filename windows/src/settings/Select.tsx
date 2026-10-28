// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { Check, ChevronDown, type LucideIcon } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

export interface Option<T extends string> {
  value: T;
  label: string;
  icon?: LucideIcon;
}

/** Shared animated picker for settings rows and the preview's round shortcuts. */
export function Select<T extends string>({
  value,
  options,
  onChange,
  label,
  disabled,
  children,
  className = "",
  style,
}: {
  value: T;
  options: Option<T>[];
  onChange: (value: T) => void;
  label: string;
  disabled?: boolean;
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
}) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [placement, setPlacement] = useState<{
    left: number;
    top: number;
    width: number;
    maxHeight: number;
    above: boolean;
  } | null>(null);
  const reduce = useReducedMotion();
  const search = useRef({ text: "", at: 0 });
  const selected = Math.max(
    0,
    options.findIndex((option) => option.value === value),
  );

  const show = (index = selected) => {
    if (disabled || !options.length) return;
    search.current = { text: "", at: 0 };
    setActive(index);
    setOpen(true);
  };
  const choose = (index: number) => {
    const option = options[index];
    if (!option) return;
    setOpen(false);
    trigger.current?.focus({ preventScroll: true });
    if (option.value !== value) onChange(option.value);
  };

  useLayoutEffect(() => {
    if (!open) return;
    const position = () => {
      const rect = trigger.current?.getBoundingClientRect();
      if (!rect) return;
      const margin = 10;
      const below = window.innerHeight - rect.bottom - margin - 6;
      const above = rect.top - margin - 6;
      const preferred = Math.min(320, options.length * 36 + 12);
      const upwards = below < preferred && above > below;
      const maxHeight = Math.max(0, Math.min(preferred, upwards ? above : below));
      const width = Math.min(Math.max(rect.width, 236), window.innerWidth - margin * 2);
      setPlacement({
        left: Math.max(margin, Math.min(rect.left, window.innerWidth - width - margin)),
        top: upwards ? rect.top - maxHeight - 6 : rect.bottom + 6,
        width,
        maxHeight,
        above: upwards,
      });
    };
    position();
    window.addEventListener("resize", position);
    return () => window.removeEventListener("resize", position);
  }, [open, options.length]);

  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!trigger.current?.contains(target) && !menu.current?.contains(target)) setOpen(false);
    };
    const scroll = (event: Event) => {
      if (!menu.current?.contains(event.target as Node)) setOpen(false);
    };
    const blur = () => setOpen(false);
    document.addEventListener("pointerdown", outside);
    document.addEventListener("scroll", scroll, true);
    window.addEventListener("blur", blur);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("scroll", scroll, true);
      window.removeEventListener("blur", blur);
    };
  }, [open]);

  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);

  useLayoutEffect(() => {
    if (open && placement) document.getElementById(`${id}-${active}`)?.scrollIntoView({ block: "nearest" });
  }, [open, active, placement, id]);

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const last = options.length - 1;
    if (last < 0) return;
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
        event.preventDefault();
        if (!open) show();
        else setActive((index) => (index + (event.key === "ArrowDown" ? 1 : last)) % options.length);
        return;
      case "Home":
      case "End":
        event.preventDefault();
        if (!open) show(event.key === "Home" ? 0 : last);
        else setActive(event.key === "Home" ? 0 : last);
        return;
      case "Enter":
      case " ":
        event.preventDefault();
        if (open) choose(active);
        else show();
        return;
      case "Escape":
        if (open) {
          event.preventDefault();
          event.stopPropagation();
          setOpen(false);
        }
        return;
      case "Tab":
        setOpen(false);
        return;
    }
    if (event.key.length !== 1 || event.ctrlKey || event.altKey || event.metaKey) return;
    event.preventDefault();
    const previous = search.current;
    const text = (Date.now() - previous.at < 700 ? previous.text : "") + event.key.toLocaleLowerCase();
    search.current = { text, at: Date.now() };
    const prefix = [...text].every((letter) => letter === text[0]) ? text[0]! : text;
    const start = open ? active : selected;
    for (let step = 1; step <= options.length; step++) {
      const index = (start + step) % options.length;
      if (options[index]!.label.toLocaleLowerCase().startsWith(prefix)) {
        if (!open) {
          setActive(index);
          setOpen(true);
        } else setActive(index);
        break;
      }
    }
  };

  return (
    <>
      <span className={`st-select${children ? " is-icon" : ""} ${className}`} style={style}>
        <button
          ref={trigger}
          type="button"
          role="combobox"
          className="st-select-trigger"
          aria-label={label}
          aria-haspopup="listbox"
          aria-expanded={open}
          aria-controls={open ? id : undefined}
          aria-activedescendant={open ? `${id}-${active}` : undefined}
          disabled={disabled || !options.length}
          title={children ? label : undefined}
          onKeyDown={onKeyDown}
          onBlur={() => setOpen(false)}
          onClick={() => (open ? setOpen(false) : show())}
        >
          {children ?? (
            <>
              <span className="st-select-value">
                {options.find((option) => option.value === value)?.label ?? value}
              </span>
              <ChevronDown size={14} className="st-select-chevron" aria-hidden />
            </>
          )}
        </button>
      </span>
      {typeof document !== "undefined" &&
        createPortal(
          <AnimatePresence>
            {open && placement ? (
              <motion.div
                key={id}
                ref={menu}
                id={id}
                role="listbox"
                aria-label={label}
                className="st-select-menu"
                style={{
                  position: "fixed",
                  left: placement.left,
                  top: placement.top,
                  width: placement.width,
                  maxHeight: placement.maxHeight,
                  transformOrigin: placement.above ? "bottom center" : "top center",
                }}
                initial={{ opacity: 0, y: reduce ? 0 : placement.above ? 6 : -6, scale: reduce ? 1 : 0.97 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: reduce ? 0 : placement.above ? 3 : -3, scale: reduce ? 1 : 0.985 }}
                transition={{ duration: reduce ? 0 : 0.18, ease: [0.16, 1, 0.3, 1] }}
              >
                {options.map((option, index) => (
                  <div
                    key={option.value}
                    id={`${id}-${index}`}
                    role="option"
                    aria-selected={option.value === value}
                    className={`st-select-option${index === active ? " is-active" : ""}`}
                    onPointerMove={() => setActive(index)}
                    onPointerDown={(event) => event.preventDefault()}
                    onClick={() => choose(index)}
                  >
                    {option.icon ? <option.icon size={17} strokeWidth={1.75} aria-hidden /> : null}
                    <span>{option.label}</span>
                    {option.value === value ? <Check size={15} className="st-select-check" aria-hidden /> : null}
                  </div>
                ))}
              </motion.div>
            ) : null}
          </AnimatePresence>,
          document.body,
        )}
    </>
  );
}
