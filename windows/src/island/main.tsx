import React from "react";
import ReactDOM from "react-dom/client";
import { Island } from "./Island";
import "./island.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Island />
  </React.StrictMode>,
);
