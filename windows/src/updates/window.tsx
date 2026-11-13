// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The update prompt (#213), the Mac's UpdatePreviewView: the new version,
// what changed, and Install and relaunch, Remind me later or Skip this
// version. The install is updater.rs; a download whose signature doesn't
// match is refused there and explained here.

import "@fontsource-variable/inter";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ShieldAlert } from "lucide-react";
import { motion, MotionConfig, useReducedMotion } from "motion/react";
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { t, tf, useLocale } from "../i18n";
import { initializeLocalization } from "../i18n/start";
import { inTauri } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import appIcon from "../../src-tauri/icons/128x128.png";
import { openRelease } from "./open";
import { SNOOZE } from "./releases";
import { useUpdates } from "./store";
import "../settings/settings.css";
import "./window.css";

function close() {
  if (inTauri) void getCurrentWindow().close();
}

function Notes({ text }: { text: string }) {
  if (!text) return <p className="up-empty">{t("updatePrompt.noNotes")}</p>;
  return (
    <div className="up-notes">
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noreferrer"
              onClick={(event) => {
                if (inTauri && href) {
                  event.preventDefault();
                  void openUrl(href);
                }
              }}
            >
              {children}
            </a>
          ),
          img: ({ alt }) => <span>{alt}</span>,
        }}
      >
        {text}
      </Markdown>
    </div>
  );
}

function Refused({ reason }: { reason: string }) {
  const latest = useUpdates((s) => s.latest);
  return (
    <motion.div
      className="up-refused"
      role="alert"
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
    >
      <ShieldAlert size={18} strokeWidth={1.75} aria-hidden />
      <div>
        <strong>{t("updatePrompt.signatureRefusedTitle")}</strong>
        <p>{reason === "signature" ? t("updatePrompt.signatureRefusedBody") : reason}</p>
        {latest ? (
          <button type="button" className="up-link" onClick={() => openRelease(latest)}>
            {t("updatePrompt.openReleasePage")}
          </button>
        ) : null}
      </div>
    </motion.div>
  );
}

function App() {
  useLocale();
  const { status, latest, progress, installError } = useUpdates();
  const [current, setCurrent] = useState("");
  const reduced = useReducedMotion();

  useEffect(() => {
    if (inTauri) void getVersion().then(setCurrent, () => undefined);
    if (useUpdates.getState().status === "idle") void useUpdates.getState().check();
  }, []);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && status !== "downloading" && status !== "installing") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [status]);

  const busy = status === "downloading" || status === "installing";
  const later = async () => {
    await setPrefs({ island: { updateSnoozedUntil: Date.now() + SNOOZE } });
    close();
  };
  const skip = async () => {
    if (latest) await setPrefs({ island: { updateSkipped: latest.version } });
    close();
  };

  if (!latest) {
    return (
      <div className="up-app">
        <div className="up-plain">
          <img className="up-icon" src={appIcon} alt="" />
          <p>{status === "checking" || status === "idle" ? t("s.updateChecking") : t("s.updateUpToDate")}</p>
          <button type="button" className="st-button" onClick={close}>
            {t("updatePrompt.close")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <MotionConfig reducedMotion="user">
      <div className="up-app">
        <header className="up-head">
          <img className="up-icon" src={appIcon} alt="" />
          <div>
            <h1>{tf("updatePrompt.titleFormat", latest.version)}</h1>
            {current ? <p>{tf("updatePrompt.currentFormat", current)}</p> : null}
          </div>
        </header>
        <main className="up-body">
          {installError ? <Refused reason={installError} /> : null}
          <Notes text={latest.notes} />
        </main>
        <footer className="up-foot">
          <p className="up-note">{t("updatePrompt.relaunchNote")}</p>
          {busy ? (
            <div className="up-progress" role="status" aria-live="polite">
              <span>
                {status === "installing" ? t("s.updateInstalling") : t("s.updateDownloading")}
                {status === "downloading" && progress !== null ? ` ${Math.round(progress * 100)}%` : ""}
              </span>
              <div className="up-bar" aria-hidden>
                <motion.div
                  className={progress === null ? "up-fill is-unknown" : "up-fill"}
                  animate={{ width: `${Math.round((progress ?? 0.3) * 100)}%` }}
                  transition={reduced ? { duration: 0 } : { duration: 0.25 }}
                />
              </div>
            </div>
          ) : (
            <div className="up-actions">
              <button type="button" className="up-link" onClick={() => void skip()}>
                {t("updatePrompt.skip")}
              </button>
              <span className="up-spacer" />
              <button type="button" className="st-button" onClick={() => void later()}>
                {t("updatePrompt.later")}
              </button>
              <button
                type="button"
                className="st-button is-accent"
                autoFocus
                onClick={() => void useUpdates.getState().install()}
              >
                {t("updatePrompt.install")}
              </button>
            </div>
          )}
        </footer>
      </div>
    </MotionConfig>
  );
}

void initializeLocalization().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
