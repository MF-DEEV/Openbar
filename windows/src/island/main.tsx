import "@fontsource-variable/inter";
import React from "react";
import ReactDOM from "react-dom/client";
import { DevDesktop } from "./DevDesktop";
import { Island } from "./Island";
import "./pages";
import { inTauri } from "../shared/bridge";
import { ClawdPreview } from "../sessions/clawd/ClawdPreview";
import "./island.css";
import "../shared/controls.css";
import "../shared/ui/ui.css";

const dev = inTauri ? null : new URLSearchParams(location.search);

// Designing in a plain browser: ?sessions fills the Sessions section with samples.
if (!inTauri && new URLSearchParams(location.search).has("sessions")) {
  void Promise.all([import("../sessions/demo"), import("../sessions/store")]).then(([demo, store]) => {
    store.useSessions.getState().receive(demo.demoSnapshot());
  });
}

if (!inTauri && new URLSearchParams(location.search).has("music")) {
  void Promise.all([import("../music/demo"), import("../music/store")]).then(([demo, store]) => {
    store.useMusic.getState().receive(demo.demoNowPlaying());
  });
}

// ?timer=running|paused|done|pomodoro|stopwatch starts a sample session.
if (!inTauri && new URLSearchParams(location.search).has("timer")) {
  void import("../timer/demo").then((demo) => demo.startDemo(new URLSearchParams(location.search).get("timer") ?? ""));
}

// ?system streams sample readings into the System section, one every 2 s.
if (!inTauri && new URLSearchParams(location.search).has("system")) {
  void Promise.all([import("../system/demo"), import("../system/store")]).then(([demo, store]) => {
    let seq = 0;
    for (; seq < 60; seq++) store.useSystem.getState().receive(demo.demoSnapshot(seq + 1));
    setInterval(() => store.useSystem.getState().receive(demo.demoSnapshot(++seq)), 2000);
  });
}

if (!inTauri && new URLSearchParams(location.search).has("calendar")) {
  const access = new URLSearchParams(location.search).get("calendar") || "allowed";
  void Promise.all([import("../calendar/demo"), import("../calendar/store"), import("../shared/prefs")]).then(
    ([demo, store, prefs]) => {
      store.receive(demo.demoCalendar(access as "allowed"));
      void prefs.setPrefs({ island: { calendarCountdown: true } });
    },
  );
}

// ?controls fills Controls and the Mixer with sample audio.
if (!inTauri && new URLSearchParams(location.search).has("controls")) {
  void import("../controls/demo").then((demo) => demo.fillDemo());
}

if (!inTauri && new URLSearchParams(location.search).has("tools")) {
  void import("../tools/demo").then((demo) => demo.startToolsDemo(new URLSearchParams(location.search).get("tools")));
}

if (!inTauri && new URLSearchParams(location.search).has("clipboard")) {
  void Promise.all([import("../clipboard/demo"), import("../clipboard/store")]).then(([demo, store]) => {
    store.useClipboard.getState().receive(demo.demoHistory());
  });
}

if (!inTauri && new URLSearchParams(location.search).has("files")) {
  void Promise.all([import("../files/demo"), import("../files/store")]).then(([demo, store]) => {
    store.useFiles.getState().receive(demo.demoShelf());
  });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {dev?.has("clawd") ? (
      <ClawdPreview />
    ) : (
      <>
        {dev?.has("desktop") ? <DevDesktop /> : null}
        <Island />
      </>
    )}
  </React.StrictMode>,
);
