// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

// Reminders of a session nobody reacted to (#231): what to remind of, then
// when, once any of it is on. The same as the Mac's Sessions settings.

import { useLocale } from "../i18n";
import type { SessionPrefs } from "../shared/prefs";
import { REMIND_AFTER, REMIND_EVERY } from "../sessions/reminders";
import { Select } from "./Select";
import { T } from "./strings";
import { SubRow, Toggle } from "./ui";

export function SessionReminders({
  prefs,
  set,
}: {
  prefs: SessionPrefs;
  set: <K extends keyof SessionPrefs>(key: K, value: SessionPrefs[K]) => void;
}) {
  useLocale();
  const S = T.sessions;
  const on = prefs.remindPermission || prefs.remindQuestion || prefs.remindFinished;
  const kinds = [
    ["remindPermission", S.remindPermission],
    ["remindQuestion", S.remindQuestion],
    ["remindFinished", S.remindFinished],
  ] as const;
  return (
    <section className="st-session-sounds" aria-label={S.reminders}>
      <h3 className="st-group-title">{S.reminders}</h3>
      <p className="st-card-caption">{S.remindersCaption}</p>
      {kinds.map(([key, title]) => (
        <SubRow key={key} title={title}>
          <Toggle label={title} checked={prefs[key]} onChange={(value) => set(key, value)} />
        </SubRow>
      ))}
      {on ? (
        <>
          <SubRow title={S.remindAfter}>
            <Select<string>
              label={S.remindAfter}
              value={String(prefs.remindAfter)}
              options={REMIND_AFTER.map((n) => ({ value: String(n), label: S.remindAfterValue(n) }))}
              onChange={(v) => set("remindAfter", Number(v))}
            />
          </SubRow>
          <SubRow title={S.remindEvery}>
            <Select<string>
              label={S.remindEvery}
              value={String(prefs.remindEvery)}
              options={REMIND_EVERY.map((n) => ({
                value: String(n),
                label: n === 0 ? S.remindOnce : S.remindEveryValue(n),
              }))}
              onChange={(v) => set("remindEvery", Number(v))}
            />
          </SubRow>
          <SubRow title={S.remindDuringDnd} caption={S.remindDuringDndCaption}>
            <Toggle
              label={S.remindDuringDnd}
              checked={prefs.remindDuringDnd}
              onChange={(value) => set("remindDuringDnd", value)}
            />
          </SubRow>
        </>
      ) : null}
    </section>
  );
}
