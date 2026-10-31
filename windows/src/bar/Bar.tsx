// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { dateTimeFormat, useLocale } from "../i18n";

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
  const day = dateTimeFormat({ weekday: "short", day: "numeric", month: "short" }, locale).format(date);
  const time = dateTimeFormat({ hour: "numeric", minute: "2-digit" }, locale).format(date);
  return `${day}  ${time}`.replace(/,/g, "");
}

export function Bar(props: { appName?: string }) {
  useLocale();
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
