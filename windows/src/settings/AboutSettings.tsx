// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Settings › About: the version, reporting a bug and exporting diagnostics
// (#230), as the Mac's About and Feedback do. A bug report opens GitHub's
// issue form with the version and system filled in; with Attach on, a
// diagnostics file is saved first, to drag into it.

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Bug, ExternalLink, FileArchive, Lightbulb, Paperclip } from "lucide-react";
import { useEffect, useState } from "react";
import { t, useLocale } from "../i18n";
import { baseName, issueUrl, REPOSITORY } from "../diagnostics/diagnostics";
import { inTauri } from "../shared/bridge";
import { onLinux } from "../shared/platform";
import { useDiagnosticsExport } from "./DiagnosticsDialog";
import { useSettings } from "./state";
import { Button, Group, Page, Row, Toggle } from "./ui";

const COPYRIGHT = "© 2026 MF DEV, Adrian and the Openbar contributors. Based on Vorssaint, © 2026 Vorssaint.";

function open(url: string) {
  if (inTauri) void openUrl(url);
  else window.open(url, "_blank", "noopener");
}

export function AboutSettings() {
  useLocale();
  const attach = useSettings((s) => s.prefs.island.feedbackAttachDiagnostics);
  const set = useSettings((s) => s.island);
  const [version, setVersion] = useState("0.4.0");
  const [system, setSystem] = useState("");
  const exporter = useDiagnosticsExport();

  useEffect(() => {
    if (!inTauri) return;
    void getVersion().then(setVersion, () => undefined);
    void invoke<string>("diagnostics_system_line").then(setSystem, () => undefined);
  }, []);

  const details = (attachment?: string) => ({ version, system, linux: onLinux(), attachment });
  const reportBug = () => {
    if (attach) {
      exporter.open({
        forBugReport: true,
        then: (path) => open(issueUrl("bug", details(baseName(path)))),
      });
    } else {
      open(issueUrl("bug", details()));
    }
  };

  return (
    <Page title={t("s.tabAbout")}>
      <div className="st-about-head">
        <span className="st-about-mark" aria-hidden />
        <div>
          <div className="st-about-name">{t("s.keepAwakeActiveIconOpenbar")}</div>
          <div className="st-about-version">
            <span>
              {t("s.versionPrefix")} {version}
            </span>
            {version.includes("-") ? <span className="st-about-badge">{t("s.betaBadgeLabel")}</span> : null}
          </div>
        </div>
      </div>
      <Group title={t("feedback.sectionTitle")}>
        <Row icon={Bug} title={t("feedback.commandBug")} caption={t("feedback.sectionCaption")}>
          <Button onClick={reportBug}>{t("feedback.commandBug")}</Button>
        </Row>
        <Row icon={Paperclip} title={t("diagnostics.attachTitle")} caption={t("diagnostics.attachCaption")}>
          <Toggle
            label={t("diagnostics.attachTitle")}
            checked={attach}
            onChange={(on) => set("feedbackAttachDiagnostics", on)}
          />
        </Row>
        <Row icon={Lightbulb} title={t("feedback.commandFeature")}>
          <Button onClick={() => open(issueUrl("feature", details()))}>{t("feedback.commandFeature")}</Button>
        </Row>
      </Group>
      <Group title={t("diagnostics.settingsTitle")}>
        <Row icon={FileArchive} title={t("diagnostics.windowTitle")} caption={t("diagnostics.settingsCaption")}>
          <Button onClick={() => exporter.open()}>{t("diagnostics.exportButton")}</Button>
        </Row>
      </Group>
      <Group>
        <Row icon={ExternalLink} title={t("s.viewOnGitHub")}>
          <Button onClick={() => open(REPOSITORY)}>{t("s.viewOnGitHub")}</Button>
        </Row>
      </Group>
      <p className="st-about-copyright">{COPYRIGHT}</p>
      {exporter.dialog}
    </Page>
  );
}
