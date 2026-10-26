// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { SlidersHorizontal } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { bridge } from "../shared/bridge";

export function VolumeControl({ source, name, accent }: { source: string; name: string; accent: string }) {
  const [open, setOpen] = useState(false);
  const reduceMotion = useReducedMotion();
  const [level, setLevel] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const pending = useRef<number | null>(null);
  const writing = useRef(false);
  const editing = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      pending.current = null;
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    let stopped = false;
    const read = async () => {
      if (writing.current || editing.current) return;
      try {
        const value = await bridge.music.volume(source);
        if (!stopped && !writing.current && !editing.current) {
          setLevel(value === undefined ? null : Math.round(value * 100));
          setFailed(value === undefined);
        }
      } catch {
        if (!stopped) setFailed(true);
      }
    };
    void read();
    const timer = setInterval(() => void read(), 1000);
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => {
      stopped = true;
      clearInterval(timer);
      document.removeEventListener("pointerdown", outside);
    };
  }, [open, source]);

  const change = async (value: number) => {
    setLevel(value);
    pending.current = value;
    if (writing.current) return;
    writing.current = true;
    try {
      while (pending.current !== null && mounted.current) {
        const next = pending.current;
        pending.current = null;
        const actual = await bridge.music.volume(source, next / 100);
        if (mounted.current) setFailed(actual === undefined);
      }
    } catch {
      if (mounted.current) setFailed(true);
    } finally {
      writing.current = false;
    }
  };

  return (
    <div
      className="m-volume"
      ref={root}
      onKeyDown={(event) => {
        if (event.key === "Escape" && open) {
          event.stopPropagation();
          setOpen(false);
          root.current?.querySelector("button")?.focus();
        }
      }}
    >
      <button
        type="button"
        className={`m-button is-small${open ? " is-selected" : ""}`}
        aria-label="Player volume"
        title="Player volume"
        aria-expanded={open}
        disabled={!source}
        onClick={() => setOpen((value) => !value)}
      >
        <SlidersHorizontal size={19} strokeWidth={2} />
      </button>
      <AnimatePresence>
        {open ? (
          <motion.div
            className="m-volume-popover"
            role="group"
            aria-label={`${name} volume`}
            initial={{ opacity: 0, y: reduceMotion ? 0 : 6, scale: reduceMotion ? 1 : 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: reduceMotion ? 0 : 4, scale: reduceMotion ? 1 : 0.98 }}
            transition={{ duration: reduceMotion ? 0 : 0.18, ease: [0.2, 0.8, 0.2, 1] }}
          >
            <div className="m-volume-label">
              <span>Volume</span>
              <span>{failed ? "Unavailable" : level === null ? "..." : level + "%"}</span>
            </div>
            <input
              type="range"
              className="m-slider"
              min={0}
              max={100}
              step={1}
              value={level ?? 0}
              disabled={level === null || failed}
              aria-label={`${name} volume`}
              aria-valuetext={`${level ?? 0}%`}
              style={{ "--fill": (level ?? 0) + "%", "--accent": accent } as CSSProperties}
              onPointerDown={() => {
                editing.current = true;
              }}
              onPointerUp={() => {
                editing.current = false;
              }}
              onPointerCancel={() => {
                editing.current = false;
              }}
              onBlur={() => {
                editing.current = false;
              }}
              onChange={(event) => void change(Number(event.target.value))}
            />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
