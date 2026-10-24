import "@fontsource-variable/inter";
import React from "react";
import ReactDOM from "react-dom/client";
import { DevDesktop } from "./DevDesktop";
import { Island } from "./Island";
import { inTauri } from "../shared/bridge";
import "./island.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {!inTauri && new URLSearchParams(location.search).has("desktop") ? <DevDesktop /> : null}
    <Island />
  </React.StrictMode>,
);
