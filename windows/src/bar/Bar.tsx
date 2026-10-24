// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The menu bar imitation along the top of the display: the active app's name
// on the left, the date and time on the right, like macOS's bar. The window
// behind it is acrylic and reserved as an app bar, so maximized windows
// start below it; the island hangs from its middle.

import { useEffect, useState } from "react";
import { bridge } from "../shared/bridge";

export const BAR_HEIGHT = 24;

function useClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      setNow(new Date());
      timer = setTimeout(tick, 1000 - (Date.now() % 1000) + 5);
    };
    tick();
    return () => clearTimeout(timer);
  }, []);
  return now;
}

export function formatBarClock(date: Date, locale?: string): string {
  const day = new Intl.DateTimeFormat(locale, { weekday: "short", day: "numeric", month: "short" }).format(date);
  const time = new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(date);
  return `${day}  ${time}`.replace(/,/g, "");
}

export function Bar(props: { appName?: string }) {
  const now = useClock();
  const [app, setApp] = useState(props.appName ?? "");
  useEffect(() => {
    if (props.appName !== undefined) return;
    const unlisten = bridge.onForegroundApp(setApp);
    return () => void unlisten.then((f) => f());
  }, [props.appName]);

  return (
    <div className="bar" style={{ height: BAR_HEIGHT }}>
      <div className="bar-leading">
        <span className="bar-app">{app}</span>
      </div>
      <div className="bar-trailing">
        <span className="bar-clock">{formatBarClock(now)}</span>
      </div>
    </div>
  );
}
