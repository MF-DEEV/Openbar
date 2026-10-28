// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLayoutEffect, useRef } from "react";

/** Read overflowing metadata in full without moving the surrounding controls. */
export function ScrollingText({ text, className }: { text: string; className: string }) {
  const viewport = useRef<HTMLDivElement>(null);
  const label = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const outer = viewport.current;
    const inner = label.current;
    if (!outer || !inner) return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    let animation: Animation | undefined;
    const update = () => {
      animation?.cancel();
      const distance = inner.scrollWidth - outer.clientWidth;
      if (distance <= 1 || preference.matches) return;
      const travel = (distance / 30) * 1000;
      const duration = travel * 2 + 3000;
      const pause = 1500 / duration;
      animation = inner.animate(
        [
          { transform: "translateX(0)", offset: 0 },
          { transform: "translateX(0)", offset: pause },
          { transform: `translateX(${-distance}px)`, offset: 0.5 },
          { transform: `translateX(${-distance}px)`, offset: 0.5 + pause },
          { transform: "translateX(0)", offset: 1 },
        ],
        { duration, iterations: Infinity, easing: "linear" },
      );
    };
    const observer = new ResizeObserver(update);
    observer.observe(outer);
    observer.observe(inner);
    preference.addEventListener("change", update);
    update();
    return () => {
      observer.disconnect();
      preference.removeEventListener("change", update);
      animation?.cancel();
    };
  }, [text]);
  return (
    <div ref={viewport} className={`${className} m-scrolling-text`} title={text}>
      <span ref={label}>{text}</span>
    </div>
  );
}
