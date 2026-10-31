// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Calendar section's options (NotchSettings.swift:442-454 and
// NotchCalendarSelection.swift): access, the countdowns, week numbers and
// which calendars show, grouped by account. Unchecked calendars stay out of
// the page and the countdown.

import { useLocale } from "../i18n";

import { useEffect, useState } from "react";
import { groupedChoices } from "../calendar/model";
import type { CalendarSnapshot } from "../calendar/types";
import { bridge } from "../shared/bridge";
import { useSettings } from "./state";
import { T } from "./strings";
import { Button, Status, SubRow, Toggle } from "./ui";

/** What calendar.rs last read, kept current while the window is open. */
function useCalendarSnapshot() {
  const [snapshot, setSnapshot] = useState<CalendarSnapshot | null>(null);
  useEffect(() => {
    let live = true;
    const unlisten = bridge.calendar.onSnapshot((s) => live && setSnapshot(s));
    void bridge.calendar.snapshot().then((s) => live && s && setSnapshot(s));
    return () => {
      live = false;
      void unlisten.then((stop) => stop());
    };
  }, []);
  return [snapshot, setSnapshot] as const;
}

export function CalendarOptions() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const [snapshot, setSnapshot] = useCalendarSnapshot();
  const [busy, setBusy] = useState(false);
  const access = snapshot?.access ?? "unavailable";
  const request = async () => {
    setBusy(true);
    try {
      await bridge.calendar.requestAccess();
      const next = await bridge.calendar.snapshot();
      if (next) setSnapshot(next);
    } finally {
      setBusy(false);
    }
  };
  const excluded = new Set(island.calendarExcluded);
  const show = (id: string, on: boolean) =>
    set("calendarExcluded", on ? island.calendarExcluded.filter((x) => x !== id) : [...excluded, id].sort());

  return (
    <>
      <p className="st-note">{T.calendar.permission}</p>
      <SubRow title={T.calendar.access} caption={access === "unavailable" ? T.calendar.unavailableCaption : undefined}>
        {access === "allowed" ? (
          <Status tone="ok">{T.calendar.granted}</Status>
        ) : access === "denied" ? (
          <>
            <Status tone="warn">{T.calendar.denied}</Status>
            <Button onClick={() => void bridge.calendar.openPrivacySettings()}>{T.calendar.settings}</Button>
          </>
        ) : access === "notAsked" ? (
          <>
            <Status tone="off">{T.calendar.notAsked}</Status>
            <Button accent busy={busy} onClick={() => void request()}>
              {T.calendar.allow}
            </Button>
          </>
        ) : (
          <Status tone="off">{T.calendar.unavailable}</Status>
        )}
      </SubRow>
      <SubRow title={T.calendar.countdown} caption={T.calendar.countdownHint}>
        <Toggle
          label={T.calendar.countdown}
          checked={island.calendarCountdown}
          onChange={(on) => set("calendarCountdown", on)}
        />
      </SubRow>
      <SubRow title={T.calendar.timeLeft} caption={T.calendar.timeLeftHint}>
        <Toggle
          label={T.calendar.timeLeft}
          checked={island.calendarTimeLeft}
          onChange={(on) => set("calendarTimeLeft", on)}
        />
      </SubRow>
      <SubRow title={T.calendar.weekNumbers}>
        <Toggle
          label={T.calendar.weekNumbers}
          checked={island.calendarWeekNumbers}
          onChange={(on) => set("calendarWeekNumbers", on)}
        />
      </SubRow>
      {access === "allowed" && snapshot ? (
        <div className="st-calendars" role="group" aria-label={T.calendar.calendars}>
          <div className="st-calendars-title">{T.calendar.calendars}</div>
          {snapshot.calendars.length === 0 ? <p>{T.calendar.noCalendars}</p> : null}
          {groupedChoices(snapshot.calendars).map((group) => (
            <div key={group[0]!.sourceId} className="st-calendars-group">
              {group[0]!.source ? <div className="st-calendars-source">{group[0]!.source}</div> : null}
              {group.map((choice) => (
                <label key={choice.id} className="st-calendars-choice">
                  <input
                    type="checkbox"
                    checked={!excluded.has(choice.id)}
                    onChange={(e) => show(choice.id, e.target.checked)}
                  />
                  <span className="st-calendars-dot" style={{ background: choice.color }} aria-hidden />
                  <span>{choice.title}</span>
                </label>
              ))}
            </div>
          ))}
        </div>
      ) : null}
    </>
  );
}
