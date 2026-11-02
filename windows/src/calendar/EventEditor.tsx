// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { useRef, useState } from "react";
import { Check, LoaderCircle, Trash2 } from "lucide-react";
import { t, useLocale } from "../i18n";
import { usePageLayer } from "../island/pageLayer";
import { bridge } from "../shared/bridge";
import { addDays } from "./model";
import { displayedEnd, initialDraft, inputDate, inputTime, parseEnd, toggleAllDay } from "./editor";
import { receive, useCalendar } from "./store";
import { C } from "./strings";
import type { CalendarEvent } from "./types";

const colours = ["#ff5c63", "#ffb45b", "#78cf98", "#59a6ff", "#b896ef"];
const close = () => useCalendar.setState({ editing: undefined });

export function EventEditor({ event }: { event: CalendarEvent | null }) {
  useLocale();
  const [draft, setDraft] = useState(() => initialDraft(event, useCalendar.getState().selectedDay ?? Date.now()));
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [error, setError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  usePageLayer(true, () => {
    if (!pending.current) close();
  });

  async function save(remove = false) {
    if (pending.current) return;
    if (!remove && (!Number.isFinite(draft.start) || !Number.isFinite(draft.end) || draft.end <= draft.start)) {
      setError(t("windows.calendar.invalidRange"));
      return;
    }
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const snapshot =
        remove && draft.id ? await bridge.calendar.deleteEvent(draft.id) : await bridge.calendar.saveEvent(draft);
      if (!snapshot) throw new Error(remove ? t("windows.calendar.deleteFailed") : t("windows.calendar.saveFailed"));
      receive(snapshot);
      // A new date may be outside the month visible before editing.
      useCalendar.setState({
        focus: draft.start,
        selectedDay: new Date(draft.start).setHours(0, 0, 0, 0),
        showingMonth: false,
      });
      close();
    } catch (reason) {
      setError(
        typeof reason === "string"
          ? reason
          : reason instanceof Error
            ? reason.message
            : t("windows.calendar.saveFailed"),
      );
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }

  return (
    <form
      className="cal-editor"
      aria-label={event ? t("windows.calendar.editEvent") : t("windows.calendar.newEvent")}
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
      onChange={() => {
        setError("");
        setConfirmDelete(false);
      }}
    >
      <fieldset className="cal-editor-fields" disabled={busy}>
        <label className="cal-editor-title">
          <span className="cal-field-label">{t("windows.calendar.eventTitle")}</span>
          <input
            autoFocus
            required
            maxLength={200}
            placeholder={t("windows.calendar.eventTitle")}
            value={draft.title}
            onChange={(e) => setDraft({ ...draft, title: e.target.value })}
          />
        </label>
        <div className="cal-editor-dates">
          <label>
            <span className="cal-field-label">{t("windows.calendar.starts")}</span>
            <input
              required
              type={draft.allDay ? "date" : "datetime-local"}
              value={inputDate(draft.start, draft.allDay)}
              min={draft.allDay ? "1970-01-01" : "1970-01-01T00:00"}
              max={draft.allDay ? "2199-12-31" : "2199-12-31T23:59"}
              onChange={(e) => {
                const start = inputTime(e.target.value, draft.allDay);
                if (!Number.isFinite(start)) return;
                setDraft({
                  ...draft,
                  start,
                  end: draft.allDay
                    ? addDays(start, Math.max(1, Math.round((draft.end - draft.start) / 86_400_000)))
                    : start + (draft.end - draft.start),
                });
              }}
            />
          </label>
          <label>
            <span className="cal-field-label">{t("windows.calendar.ends")}</span>
            <input
              required
              type={draft.allDay ? "date" : "datetime-local"}
              value={displayedEnd(draft)}
              min={draft.allDay ? "1970-01-01" : "1970-01-01T00:00"}
              max={draft.allDay ? "2199-12-31" : "2199-12-31T23:59"}
              onChange={(e) => {
                const end = parseEnd(e.target.value, draft.allDay);
                if (Number.isFinite(end)) setDraft({ ...draft, end });
              }}
            />
          </label>
        </div>
        <div className="cal-editor-options">
          <label className="cal-all-day">
            <input
              type="checkbox"
              checked={draft.allDay}
              onChange={(e) => setDraft(toggleAllDay(draft, e.target.checked))}
            />
            {C.allDay}
          </label>
          <div className="cal-colours" role="group" aria-label={t("windows.calendar.colour")}>
            {colours.map((colour, i) => (
              <button
                key={colour}
                type="button"
                aria-label={`${t("windows.calendar.colour")} ${i + 1}`}
                aria-pressed={draft.color === colour}
                style={{ backgroundColor: colour }}
                onClick={() => setDraft({ ...draft, color: colour })}
              >
                {draft.color === colour ? <Check size={11} strokeWidth={3} /> : null}
              </button>
            ))}
          </div>
        </div>
        <label>
          <span className="cal-field-label">{t("windows.calendar.location")}</span>
          <input
            maxLength={500}
            placeholder={t("windows.calendar.location")}
            value={draft.location}
            onChange={(e) => setDraft({ ...draft, location: e.target.value })}
          />
        </label>
        <label>
          <span className="cal-field-label">{t("windows.calendar.description")}</span>
          <textarea
            maxLength={10000}
            rows={2}
            placeholder={t("windows.calendar.description")}
            value={draft.description}
            onChange={(e) => setDraft({ ...draft, description: e.target.value })}
          />
        </label>
        <p className="cal-local-hint">{t("windows.calendar.localHint")}</p>
      </fieldset>
      {error ? (
        <p className="cal-editor-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="cal-editor-footer">
        {event ? (
          <button
            type="button"
            className={`cal-delete${confirmDelete ? " is-confirming" : ""}`}
            disabled={busy}
            onClick={() => {
              if (confirmDelete) void save(true);
              else setConfirmDelete(true);
            }}
          >
            <Trash2 size={13} />
            {confirmDelete ? t("windows.calendar.confirmDelete") : t("windows.calendar.delete")}
          </button>
        ) : (
          <span />
        )}
        <div>
          <button type="button" className="n-pill-button" disabled={busy} onClick={close}>
            {t("windows.calendar.cancel")}
          </button>
          <button type="submit" className="n-pill-button is-prominent" disabled={busy || !draft.title.trim()}>
            {busy ? <LoaderCircle size={13} className="cal-spinner" /> : null}
            {t("windows.calendar.save")}
          </button>
        </div>
      </div>
    </form>
  );
}
