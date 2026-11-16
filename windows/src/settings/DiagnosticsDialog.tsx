// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The short screen before a diagnostics file is saved (#230): what goes in,
// in plain words, with a switch for each part that may stay out. The rows
// come from PARTS, the list the file is written from.

import { invoke } from "@tauri-apps/api/core";
import {
  Check,
  FileArchive,
  FileText,
  ListChecks,
  type LucideIcon,
  Monitor,
  Network,
  ShieldCheck,
  Terminal,
  ToggleRight,
} from "lucide-react";
import React, { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { getLanguage, t, tf, useLocale } from "../i18n";
import { EVENT_COUNT, LOG_DAYS, PARTS, type PartId, request } from "../diagnostics/diagnostics";
import { inTauri } from "../shared/bridge";
import { useSettings } from "./state";
import { Button, SubRow, Toggle } from "./ui";
import "./diagnostics.css";

const ICONS: Record<PartId, LucideIcon> = {
  system: Monitor,
  features: ToggleRight,
  agents: Terminal,
  events: ListChecks,
  hosts: Network,
  logs: FileText,
};

function title(id: PartId): string {
  switch (id) {
    case "system":
      return t("diagnostics.systemTitle");
    case "features":
      return t("diagnostics.featuresTitle");
    case "agents":
      return t("diagnostics.agentsTitle");
    case "events":
      return t("diagnostics.eventsTitle");
    case "hosts":
      return t("diagnostics.hostsTitle");
    case "logs":
      return t("diagnostics.logsTitle");
  }
}

function caption(id: PartId): string {
  switch (id) {
    case "system":
      return t("diagnostics.systemCaption");
    case "features":
      return t("diagnostics.featuresCaption");
    case "agents":
      return t("diagnostics.agentsCaption");
    case "events":
      return tf("diagnostics.eventsCaptionFormat", EVENT_COUNT);
    case "hosts":
      return t("diagnostics.hostsCaption");
    case "logs":
      return tf("diagnostics.logsCaptionFormat", LOG_DAYS);
  }
}

export function DiagnosticsDialog(props: {
  /** Saved for a bug report, whose form opens next. */
  forBugReport?: boolean;
  onClose: () => void;
  onSaved: (path: string) => void;
}) {
  useLocale();
  const prefs = useSettings((s) => s.prefs);
  const [chosen, setChosen] = useState<Set<PartId>>(() => new Set(PARTS.map((p) => p.id)));
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const titleId = useId();
  const saveRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    saveRef.current?.querySelector<HTMLButtonElement>(".st-button.is-accent")?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !saving) props.onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props, saving]);

  const toggle = (id: PartId, on: boolean) =>
    setChosen((current) => {
      const next = new Set(current);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  const save = async () => {
    setSaving(true);
    setFailure(null);
    try {
      const body = request(chosen, prefs, getLanguage());
      const path = inTauri
        ? await invoke<string | null>("diagnostics_export", { request: body })
        : await new Promise<string>((done) => setTimeout(() => done("openbar-diagnostics.zip"), 700));
      if (path) props.onSaved(path);
    } catch (error) {
      setFailure(tf("diagnostics.failedFormat", String(error)));
    } finally {
      setSaving(false);
    }
  };

  const row = (id: PartId, optional: boolean) => {
    const Icon = ICONS[id];
    return (
      <div className="dg-row" key={id} role="group" aria-label={title(id)}>
        <Icon className="dg-row-icon" size={18} strokeWidth={1.75} aria-hidden />
        <div className="dg-row-text">
          <div className="dg-row-title">{title(id)}</div>
          <div className="dg-row-caption">{caption(id)}</div>
        </div>
        {optional ? (
          <Toggle label={title(id)} checked={chosen.has(id)} disabled={saving} onChange={(on) => toggle(id, on)} />
        ) : (
          <Check className="dg-row-check" size={16} strokeWidth={2.25} aria-hidden />
        )}
      </div>
    );
  };

  // On the body: the page slides in under a transform, which would hold a
  // fixed backdrop inside it instead of over the whole window.
  return createPortal(
    <div
      className="dg-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !saving) props.onClose();
      }}
    >
      <div className="dg-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <header className="dg-head">
          <span className="dg-head-icon" aria-hidden>
            <FileArchive size={22} strokeWidth={1.75} />
          </span>
          <div>
            <h2 className="dg-title" id={titleId}>
              {t("diagnostics.windowTitle")}
            </h2>
            <p className="dg-intro">{t("diagnostics.intro")}</p>
          </div>
        </header>
        <section className="dg-section">
          <h3 className="dg-section-title">{t("diagnostics.alwaysTitle")}</h3>
          <div className="dg-rows">{PARTS.filter((p) => !p.optional).map((p) => row(p.id, false))}</div>
        </section>
        <section className="dg-section">
          <h3 className="dg-section-title">{t("diagnostics.optionalTitle")}</h3>
          <div className="dg-rows">{PARTS.filter((p) => p.optional).map((p) => row(p.id, true))}</div>
        </section>
        <div className="dg-note">
          <ShieldCheck size={16} strokeWidth={1.75} aria-hidden />
          <div>
            <p>{t("diagnostics.privacyNote")}</p>
            <p>{t("diagnostics.nothingSent")}</p>
            {props.forBugReport ? <p>{t("diagnostics.reportHint")}</p> : null}
          </div>
        </div>
        {failure ? (
          <p className="dg-failure" role="alert">
            {failure}
          </p>
        ) : null}
        <footer className="dg-foot" ref={saveRef}>
          {saving ? (
            <span className="dg-saving" role="status">
              <span className="dg-spinner" aria-hidden />
              {t("diagnostics.saving")}
            </span>
          ) : null}
          <Button onClick={props.onClose} disabled={saving}>
            {t("diagnostics.cancel")}
          </Button>
          <Button accent busy={saving} onClick={() => void save()}>
            {props.forBugReport ? t("diagnostics.saveAndReport") : t("diagnostics.save")}
          </Button>
        </footer>
      </div>
    </div>,
    document.body,
  );
}

/** The dialog and how to open it, for About and Settings › Sessions. */
export function useDiagnosticsExport(): {
  dialog: React.JSX.Element | null;
  open: (options?: { forBugReport?: boolean; then?: (path: string) => void }) => void;
} {
  const [shown, setShown] = useState<{ forBugReport: boolean; then?: (path: string) => void } | null>(null);
  const dialog = shown ? (
    <DiagnosticsDialog
      forBugReport={shown.forBugReport}
      onClose={() => setShown(null)}
      onSaved={(path) => {
        setShown(null);
        shown.then?.(path);
      }}
    />
  ) : null;
  return {
    dialog,
    open: (options) => setShown({ forBugReport: options?.forBugReport ?? false, then: options?.then }),
  };
}

/** Export diagnostics in Settings › Content › Sessions, beside the hooks it reports on. */
export function DiagnosticsSubRow() {
  useLocale();
  const exporter = useDiagnosticsExport();
  return (
    <>
      <SubRow title={t("diagnostics.settingsTitle")} caption={t("diagnostics.settingsCaption")}>
        <Button onClick={() => exporter.open()}>{t("diagnostics.exportButton")}</Button>
      </SubRow>
      {exporter.dialog}
    </>
  );
}
