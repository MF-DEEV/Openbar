// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { addMonths, startOfMonth } from "./model";
import { C } from "./strings";
import { useLocale } from "../i18n";

/** Native scrolling stays on the compositor. Five pages are recycled only
 * after scrolling settles, keeping the DOM bounded across years of browsing. */
export function MonthScroller({
  month,
  onBrowse,
  children,
}: {
  month: number;
  onBrowse(month: number): void;
  children(month: number): ReactNode;
}) {
  useLocale();
  const [anchor, setAnchor] = useState(() => startOfMonth(month));
  const viewport = useRef<HTMLDivElement>(null);
  const current = useRef({ anchor, onBrowse });
  current.current = { anchor, onBrowse };
  const resetting = useRef(false);

  useLayoutEffect(() => {
    const el = viewport.current!;
    const center = () => {
      resetting.current = true;
      el.scrollTo({ top: el.clientHeight * 2, behavior: "instant" });
      requestAnimationFrame(() => {
        resetting.current = false;
      });
    };
    center();
    const observer = new ResizeObserver(center);
    observer.observe(el);
    return () => observer.disconnect();
  }, [anchor]);

  useEffect(() => {
    const el = viewport.current!;
    let timer: ReturnType<typeof setTimeout>;
    const settle = () => {
      if (resetting.current || !el.clientHeight) return;
      const index = Math.max(0, Math.min(4, Math.round(el.scrollTop / el.clientHeight)));
      // Fallback must not commit halfway through the browser's snap animation.
      if (Math.abs(el.scrollTop - index * el.clientHeight) > 1) return;
      const next = addMonths(current.current.anchor, index - 2);
      if (next === current.current.anchor) return;
      setAnchor(next);
      current.current.onBrowse(next);
    };
    const scroll = () => {
      clearTimeout(timer);
      timer = setTimeout(settle, 180);
    };
    el.addEventListener("scroll", scroll, { passive: true });
    el.addEventListener("scrollend", settle);
    return () => {
      clearTimeout(timer);
      el.removeEventListener("scroll", scroll);
      el.removeEventListener("scrollend", settle);
    };
  }, []);

  // Header arrows use the same smooth movement as the wheel. A jump to Today
  // from a distant year recenters directly instead of animating through years.
  useEffect(() => {
    const target = startOfMonth(month);
    if (target === anchor) return;
    const a = new Date(anchor),
      b = new Date(target);
    const offset = (b.getFullYear() - a.getFullYear()) * 12 + b.getMonth() - a.getMonth();
    if (Math.abs(offset) > 2) {
      setAnchor(target);
      return;
    }
    viewport.current?.scrollTo({
      top: viewport.current.clientHeight * (2 + offset),
      behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth",
    });
  }, [month, anchor]);

  return (
    <div className="cal-month-scroll" ref={viewport} tabIndex={0} role="region" aria-label={C.month}>
      {[-2, -1, 0, 1, 2].map((offset) => {
        const date = addMonths(anchor, offset);
        return (
          <section key={date} className="cal-month-page" data-month={date}>
            {children(date)}
          </section>
        );
      })}
    </div>
  );
}
