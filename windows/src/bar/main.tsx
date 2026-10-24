import "@fontsource-variable/inter";
import React from "react";
import ReactDOM from "react-dom/client";
import { Bar } from "./Bar";
import "./bar.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Bar />
  </React.StrictMode>,
);
