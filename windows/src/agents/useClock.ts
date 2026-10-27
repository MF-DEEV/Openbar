// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useEffect, useState } from "react";

/** Milliseconds since 1970, ticking every `interval` ms on the second (the Mac's TimelineView). */
export function useClock(interval: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      setNow(Date.now());
      timer = setTimeout(tick, interval - (Date.now() % 1000) + 5);
    };
    timer = setTimeout(tick, interval - (Date.now() % 1000) + 5);
    return () => clearTimeout(timer);
  }, [interval]);
  return now;
}
