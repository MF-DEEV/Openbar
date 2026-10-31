// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t, tf } from "../i18n";

import { SlidersHorizontal } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { bridge } from "../shared/bridge";
import { usePageLayer } from "../island/pageLayer";
import { SourcePicker } from "./SourcePicker";

export function VolumeControl({ source, name, accent }: { source: string; name: string; accent: string }) {
  useLocale();
  const [open, setOpen] = useState(false);
  const reduceMotion = useReducedMotion();
  const [level, setLevel] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const currentSource = useRef(source);
  currentSource.current = source;
  const [position, setPosition] = useState<{ left: number; top: number; width: number; maxHeight: number } | null>(
    null,
  );
  usePageLayer(open, () => setOpen(false));
  const pending = useRef<number | null>(null);
  const writing = useRef(false);
  const editing = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    pending.current = null;
    editing.current = false;
    setLevel(null);
    setFailed(false);
  }, [source]);
  useLayoutEffect(() => {
    if (!open || !panel.current || !root.current) return;
    const element = panel.current;
    const frame = document.querySelector(".island-frame");
    const update = () => {
      const bounds = frame?.getBoundingClientRect() ?? new DOMRect(0, 0, window.innerWidth, window.innerHeight);
      const anchor = root.current?.getBoundingClientRect();
      if (!anchor) return;
      const width = Math.min(240, bounds.width - 24);
      const maxHeight = Math.max(0, bounds.height - 24);
      const height = Math.min(element.scrollHeight, maxHeight);
      const next = {
        width,
        maxHeight,
        left: Math.max(bounds.left + 12, Math.min(anchor.right - width, bounds.right - width - 12)),
        top: Math.max(bounds.top + 12, Math.min(anchor.top - height - 6, bounds.bottom - height - 12)),
      };
      setPosition((previous) =>
        previous &&
        Object.keys(next).every((key) => previous[key as keyof typeof next] === next[key as keyof typeof next])
          ? previous
          : next,
      );
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    if (frame) observer.observe(frame);
    return () => observer.disconnect();
  }, [open]);
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
      if (!source) return;
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
    const timer = source ? setInterval(() => void read(), 1000) : undefined;
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node) && !panel.current?.contains(event.target as Node))
        setOpen(false);
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
        if (mounted.current && currentSource.current === source) setFailed(actual === undefined);
      }
    } catch {
      if (mounted.current && currentSource.current === source) setFailed(true);
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
        aria-label={t("windows.music.musicOptions")}
        title={t("windows.music.musicOptions")}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <SlidersHorizontal size={19} strokeWidth={2} />
      </button>
      {createPortal(
        <AnimatePresence>
          {open ? (
            <motion.div
              ref={panel}
              className="m-volume-popover"
              role="group"
              aria-label={t("windows.music.musicOptions")}
              style={position ? { ...position, overflowY: "auto" } : { visibility: "hidden" }}
              initial={{ opacity: 0, y: reduceMotion ? 0 : 6, scale: reduceMotion ? 1 : 0.97 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: reduceMotion ? 0 : 4, scale: reduceMotion ? 1 : 0.98 }}
              transition={{ duration: reduceMotion ? 0 : 0.18, ease: [0.2, 0.8, 0.2, 1] }}
            >
              <div className="m-volume-label">
                <span>{t("notch.volume")}</span>
                <span>{!source || failed ? t("notchShape.unavailable") : level === null ? "..." : level + "%"}</span>
              </div>
              <input
                type="range"
                className="m-slider"
                min={0}
                max={100}
                step={1}
                value={level ?? 0}
                disabled={level === null || failed}
                aria-label={tf("windows.music.valueVolume", name)}
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
              <SourcePicker />
            </motion.div>
          ) : null}
        </AnimatePresence>,
        document.body,
      )}
    </div>
  );
}
