// SPDX-License-Identifier: GPL-3.0-or-later

import { localizeMessage, dateTimeFormat, useLocale, t } from "../i18n";

import { Image, ScanLine, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { usePageLayer } from "../island/pageLayer";
import { EmptyState, IconButton } from "../island/views/Controls";
import type { PageProps } from "../island/views/Expanded";
import { subscribeCaptures, useCaptures } from "./store";
import type { RecentCapture } from "./types";
import "./captures.css";

export function CapturesPage({ width, height }: PageProps) {
  useLocale();
  const { entries, loading, busy, error, confirmingClear, refresh } = useCaptures();
  useEffect(() => {
    useCaptures.setState({ error: null, confirmingClear: false });
    const stop = subscribeCaptures();
    return () => {
      stop();
      useCaptures.setState({ confirmingClear: false });
    };
  }, []);
  const perRow = Math.max(1, Math.floor((width + 8) / 244));
  const rows = Math.max(1, Math.min(Math.ceil(entries.length / perRow), Math.floor((height + 8) / 96)));
  return (
    <div className="cp-page" aria-label={t("recentCaptures.title")} aria-busy={loading || busy}>
      {error ? (
        <div className="cp-error" role="alert">
          <span>{localizeMessage(error)}</span>
          <button
            type="button"
            onClick={() => {
              useCaptures.setState({ error: null });
              void refresh();
            }}
          >
            {t("windows.captures.retry")}
          </button>
        </div>
      ) : null}
      {entries.length === 0 ? (
        <EmptyState
          icon={ScanLine}
          title={loading ? t("windows.captures.loadingCaptures") : t("windows.captures.noRecentCaptures")}
          detail={loading ? undefined : t("windows.captures.screenshotsYouTakeWithVorssaintWillAppearHere")}
        />
      ) : (
        <div
          className="cp-rail"
          role="list"
          aria-label={t("windows.captures.recentScreenshots")}
          style={{ gridTemplateRows: `repeat(${rows}, minmax(88px, 1fr))` }}
        >
          {entries.map((entry) => (
            <CaptureCard key={entry.id} entry={entry} busy={busy} />
          ))}
        </div>
      )}
      {confirmingClear ? <ClearConfirmation /> : null}
    </div>
  );
}

function CaptureCard({ entry, busy }: { entry: RecentCapture; busy: boolean }) {
  useLocale();
  const [failed, setFailed] = useState(false);
  const date = dateTimeFormat({ dateStyle: "short", timeStyle: "medium" }).format(new Date(entry.createdAt));
  return (
    <div className="cp-card" role="listitem">
      <div className="cp-preview">
        {entry.thumbnail && !failed ? (
          <img src={entry.thumbnail} alt={t("windows.captures.screenshotPreview")} onError={() => setFailed(true)} />
        ) : (
          <Image size={22} aria-label={t("windows.captures.previewUnavailable")} />
        )}
      </div>
      <div className="cp-info">
        <strong>{t("recentCaptures.screenshot")}</strong>
        <time dateTime={new Date(entry.createdAt).toISOString()} title={date}>
          {date}
        </time>
        <div className="cp-actions">
          <button
            type="button"
            className="n-pill-button is-prominent"
            disabled={busy}
            onClick={() => void useCaptures.getState().perform("open", entry.id)}
          >
            {t("recentCaptures.restore")}
          </button>
          <IconButton
            icon={Trash2}
            label={t("windows.captures.deleteCapture")}
            disabled={busy}
            onClick={() => void useCaptures.getState().perform("remove", entry.id)}
          />
        </div>
      </div>
    </div>
  );
}

function ClearConfirmation() {
  useLocale();
  const cancel = useRef<HTMLButtonElement>(null);
  const close = () => useCaptures.setState({ confirmingClear: false });
  usePageLayer(true, close);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    cancel.current?.focus();
    return () => previous?.focus();
  }, []);
  return (
    <div className="cp-confirm-backdrop">
      <div
        className="cp-confirm"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="cp-clear-title"
        aria-describedby="cp-clear-detail"
        onKeyDown={(event) => {
          if (event.key !== "Tab") return;
          const buttons = event.currentTarget.querySelectorAll("button");
          event.preventDefault();
          const next = document.activeElement === buttons[0] ? buttons[1] : buttons[0];
          next?.focus();
        }}
      >
        <strong id="cp-clear-title">{t("windows.captures.clearAllRecentCaptures")}</strong>
        <p id="cp-clear-detail">{t("windows.captures.savedScreenshotsStayInYourScreenshotsFolderOnly")}</p>
        <div className="cp-actions">
          <button ref={cancel} type="button" className="n-pill-button" onClick={close}>
            {t("clipboard.cancel")}
          </button>
          <button
            type="button"
            className="n-pill-button is-prominent"
            onClick={() => void useCaptures.getState().perform("clear")}
          >
            {t("notchNotifications.clearAll")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function CapturesTrailing() {
  useLocale();
  const disabled = useCaptures((s) => s.entries.length === 0 || s.busy);
  return (
    <IconButton
      icon={Trash2}
      label={t("windows.captures.clearAllRecentCaptures6214bb")}
      disabled={disabled}
      onClick={() => useCaptures.setState({ confirmingClear: true })}
    />
  );
}
