import "@fontsource-variable/inter";
import React from "react";
import ReactDOM from "react-dom/client";
import { DevDesktop } from "./DevDesktop";
import { Island } from "./Island";
import "./pages";
import { inTauri } from "../shared/bridge";
import { ClawdPreview } from "../sessions/clawd/ClawdPreview";
import "./island.css";

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
