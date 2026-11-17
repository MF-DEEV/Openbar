import { initializeLocalization } from "../i18n/start";
import "@fontsource-variable/inter";
import React from "react";
import ReactDOM from "react-dom/client";
import { Bar } from "./Bar";
import "./bar.css";

void initializeLocalization().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Bar />
    </React.StrictMode>,
  );
});
