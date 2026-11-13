// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Settings › General › Updates: the automatic check, the beta channel, and
// what the last check found. Install opens the update prompt (updates/window.tsx).

import { getVersion } from "@tauri-apps/api/app";
import { BadgeCheck, CircleAlert, Download, FlaskConical, RefreshCw, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { getLanguage, t, useLocale } from "../i18n";
import { inTauri } from "../shared/bridge";
import { openPrompt } from "../updates/open";
import { useUpdates } from "../updates/store";
import { useSettings } from "./state";
import { Button, Row, Toggle } from "./ui";

export function UpdateRows() {
  useLocale();
  const automatic = useSettings((s) => s.prefs.island.checkUpdates);
  const betas = useSettings((s) => s.prefs.island.betaUpdates);
  const set = useSettings((s) => s.island);
  const { status, latest, checkedAt, failure, progress, installError } = useUpdates();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    if (inTauri) void getVersion().then(setVersion, () => undefined);
  }, []);
  // With the check on, opening Settings asks once, so the row says something current.
  useEffect(() => {
    if (automatic && inTauri && useUpdates.getState().status === "idle") void useUpdates.getState().check();
  }, [automatic]);

  const title =
    status === "checking"
      ? t("s.updateChecking")
      : status === "downloading"
        ? `${t("s.updateDownloading")}${progress !== null ? ` ${Math.round(progress * 100)}%` : ""}`
        : status === "installing"
          ? t("s.updateInstalling")
          : status === "available" && latest
            ? `${t("s.updateAvailablePrefix")} ${latest.version}`
            : status === "failed"
              ? `${t("s.updateFailedPrefix")} ${failure ?? ""}`.trim()
              : status === "upToDate"
                ? t("s.updateUpToDate")
                : (version ?? "");
  const when = checkedAt
    ? new Intl.DateTimeFormat(getLanguage(), { dateStyle: "short", timeStyle: "short" }).format(new Date(checkedAt))
    : null;
  const caption = [version, when ? `${t("s.updateLastChecked")} ${when}` : null].filter(Boolean).join(" · ");
  const icon =
    status === "available" || status === "downloading" || status === "installing"
      ? Download
      : status === "failed"
        ? CircleAlert
        : BadgeCheck;

  return (
    <>
      <Row icon={RefreshCw} title={t("s.autoCheckToggle")}>
        <Toggle label={t("s.autoCheckToggle")} checked={automatic} onChange={(on) => set("checkUpdates", on)} />
      </Row>
      <Row icon={FlaskConical} title={t("s.includeBetaUpdatesToggle")} caption={t("s.includeBetaUpdatesCaption")}>
        <Toggle
          label={t("s.includeBetaUpdatesToggle")}
          checked={betas}
          onChange={(on) => {
            set("betaUpdates", on);
            // The channel decides what the feed offers; ask again with it.
            if (inTauri) setTimeout(() => void useUpdates.getState().check(), 300);
          }}
        />
      </Row>
      {installError ? (
        <Row
          icon={ShieldAlert}
          title={t("updatePrompt.signatureRefusedTitle")}
          caption={installError === "signature" ? t("updatePrompt.signatureRefusedBody") : installError}
        />
      ) : null}
      <Row icon={icon} title={title} caption={caption || undefined}>
        {status === "available" && latest ? (
          <Button accent onClick={() => openPrompt(latest)}>
            {t("s.updateBannerAction")}
          </Button>
        ) : null}
        <Button
          busy={status === "checking" || status === "downloading" || status === "installing"}
          onClick={() => void useUpdates.getState().check()}
        >
          {t("s.menuCheckUpdates")}
        </Button>
      </Row>
    </>
  );
}
