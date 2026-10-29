// SPDX-License-Identifier: GPL-3.0-or-later
import { Image, ScanLine, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { usePageLayer } from "../island/pageLayer";
import { EmptyState, IconButton } from "../island/views/Controls";
import type { PageProps } from "../island/views/Expanded";
import { subscribeCaptures, useCaptures } from "./store";
import type { RecentCapture } from "./types";
import "./captures.css";

export function CapturesPage({ width, height }: PageProps) {
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
    <div className="cp-page" aria-label="Recent captures" aria-busy={loading || busy}>
      {error ? (
        <div className="cp-error" role="alert">
          <span>{error}</span>
          <button
            type="button"
            onClick={() => {
              useCaptures.setState({ error: null });
              void refresh();
            }}
          >
            Retry
          </button>
        </div>
      ) : null}
      {entries.length === 0 ? (
        <EmptyState
          icon={ScanLine}
          title={loading ? "Loading captures…" : "No recent captures"}
          detail={loading ? undefined : "Screenshots you take with Vorssaint will appear here."}
        />
      ) : (
        <div
          className="cp-rail"
          role="list"
          aria-label="Recent screenshots"
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
  const [failed, setFailed] = useState(false);
  const date = new Date(entry.createdAt).toLocaleString();
  return (
    <div className="cp-card" role="listitem">
      <div className="cp-preview">
        {entry.thumbnail && !failed ? (
          <img src={entry.thumbnail} alt="Screenshot preview" onError={() => setFailed(true)} />
        ) : (
          <Image size={22} aria-label="Preview unavailable" />
        )}
      </div>
      <div className="cp-info">
        <strong>Screenshot</strong>
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
            Restore
          </button>
          <IconButton
            icon={Trash2}
            label="Delete capture"
            disabled={busy}
            onClick={() => void useCaptures.getState().perform("remove", entry.id)}
          />
        </div>
      </div>
    </div>
  );
}

function ClearConfirmation() {
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
        <strong id="cp-clear-title">Clear all recent captures?</strong>
        <p id="cp-clear-detail">Saved screenshots stay in your Screenshots folder. Only history copies are removed.</p>
        <div className="cp-actions">
          <button ref={cancel} type="button" className="n-pill-button" onClick={close}>
            Cancel
          </button>
          <button
            type="button"
            className="n-pill-button is-prominent"
            onClick={() => void useCaptures.getState().perform("clear")}
          >
            Clear All
          </button>
        </div>
      </div>
    </div>
  );
}

export function CapturesTrailing() {
  const disabled = useCaptures((s) => s.entries.length === 0 || s.busy);
  return (
    <IconButton
      icon={Trash2}
      label="Clear all recent captures"
      disabled={disabled}
      onClick={() => useCaptures.setState({ confirmingClear: true })}
    />
  );
}
