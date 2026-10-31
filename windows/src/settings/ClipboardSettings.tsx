// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Clipboard options, inside its card on the Content tab: the history itself
// and what it keeps (ClipboardSettings.swift, ClipboardIgnoredAppsList.swift),
// then the island's entry size and notice (NotchSettings.swift:479-495).

import { formattingLocale, useLocale } from "../i18n";

import { useState } from "react";
import { CLIPBOARD_LIMITS, CLIPBOARD_RETENTION_DAYS } from "../island/settings";
import { useSettings } from "./state";
import { T } from "./strings";
import { Segmented, Select, SubRow, Toggle } from "./ui";

const S = T.clipboard;

export function ClipboardOptions() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const [draft, setDraft] = useState<string | null>(null);
  const off = !island.clipboardHistoryEnabled;
  return (
    <>
      <SubRow title={S.enable} caption={S.enableCaption}>
        <Toggle
          label={S.enable}
          checked={island.clipboardHistoryEnabled}
          onChange={(on) => set("clipboardHistoryEnabled", on)}
        />
      </SubRow>
      <SubRow title={S.includeImagesFiles} caption={S.includeImagesFilesCaption} disabled={off}>
        <Toggle
          label={S.includeImagesFiles}
          checked={island.clipboardHistoryIncludeImagesFiles}
          disabled={off}
          onChange={(on) => set("clipboardHistoryIncludeImagesFiles", on)}
        />
      </SubRow>
      <SubRow title={S.skipSensitive} caption={S.skipSensitiveCaption} disabled={off}>
        <Toggle
          label={S.skipSensitive}
          checked={island.clipboardFilterSensitiveText}
          disabled={off}
          onChange={(on) => set("clipboardFilterSensitiveText", on)}
        />
      </SubRow>
      <SubRow title={S.limit} disabled={off}>
        <Select
          label={S.limit}
          value={String(island.clipboardHistoryLimit)}
          disabled={off}
          options={CLIPBOARD_LIMITS.map((n) => ({
            value: String(n),
            label: n === 0 ? S.limitUnlimited : n.toLocaleString(formattingLocale()),
          }))}
          onChange={(v) => set("clipboardHistoryLimit", Number(v))}
        />
      </SubRow>
      <SubRow title={S.retention} caption={S.retentionCaption} disabled={off}>
        <Select
          label={S.retention}
          value={String(island.clipboardHistoryRetentionDays)}
          disabled={off}
          options={CLIPBOARD_RETENTION_DAYS.map((n) => ({ value: String(n), label: n === 0 ? S.forever : S.days(n) }))}
          onChange={(v) => set("clipboardHistoryRetentionDays", Number(v))}
        />
      </SubRow>
      <div className="st-download-folders">
        <label htmlFor="clipboard-ignored-apps">{S.ignoredApps}</label>
        <p>{S.ignoredAppsCaption}</p>
        <textarea
          id="clipboard-ignored-apps"
          rows={3}
          spellCheck={false}
          placeholder={"KeePass.exe\n1Password.exe"}
          value={draft ?? island.clipboardIgnoredApps.join("\n")}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => {
            if (draft === null) return;
            const names = draft
              .split(/\r?\n/)
              .map((line) => line.trim().split(/[\\/]/).pop() ?? "")
              .filter(Boolean);
            set("clipboardIgnoredApps", [...new Set(names)]);
            setDraft(null);
          }}
        />
      </div>
      <SubRow title={S.entrySize} caption={S.entrySizeCaption}>
        <Segmented
          label={S.entrySize}
          value={island.clipboardCardSize}
          options={[
            { value: "compact", label: S.compact },
            { value: "comfortable", label: S.comfortable },
          ]}
          onChange={(size) => set("clipboardCardSize", size)}
        />
      </SubRow>
      <SubRow title={S.notice} caption={S.noticeCaption}>
        <Toggle label={S.notice} checked={island.clipboardNotice} onChange={(on) => set("clipboardNotice", on)} />
      </SubRow>
    </>
  );
}
