// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Settings › Sessions › Phone approvals (Telegram), #214: what leaves the PC
// and where to, a bot's token from BotFather, the one-time code that links
// the phone, and when messages go out. The same as the Mac's
// PhoneApprovalsSettings.swift; the bridge is phone/ in the Rust side.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { t, tf, useLocale } from "../i18n";
import { inTauri } from "../shared/bridge";
import { Select } from "./Select";
import { useSettings } from "./state";
import { Button, SubRow, Toggle } from "./ui";

export interface PhoneStatus {
  supported: boolean;
  bot: string | null;
  linked: string | null;
  linking: { code: string; link: string; expires: number; qr?: string } | null;
  problem: string | null;
  checking: boolean;
}

const AWAY_CHOICES = [1, 2, 5, 10, 15, 30];

/** Outside the app (designing in a browser), a stand-in that walks through the steps. */
const DEMO: PhoneStatus = { supported: true, bot: null, linked: null, linking: null, problem: null, checking: false };

export function problemText(problem: string): string {
  switch (problem) {
    case "invalidToken":
      return t("windows.settings.sessions.phoneProblemInvalidToken");
    case "credentialFailed":
      return t("windows.settings.sessions.phoneProblemCredentialFailed");
    case "offline":
      return t("windows.settings.sessions.phoneProblemOffline");
    case "conflict":
      return t("windows.settings.sessions.phoneProblemConflict");
    case "revoked":
      return t("windows.settings.sessions.phoneProblemRevoked");
    case "codeExpired":
      return t("windows.settings.sessions.phoneProblemCodeExpired");
    default:
      return t("windows.settings.sessions.phoneProblemTooManyTries");
  }
}

/** Minutes after midnight as the time field writes them, and back. */
export function timeText(minutes: number): string {
  const m = Math.min(Math.max(Math.round(minutes), 0), 24 * 60 - 1);
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

export function timeMinutes(text: string): number | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(text);
  if (!match) return null;
  const minutes = Number(match[1]) * 60 + Number(match[2]);
  return minutes < 24 * 60 ? minutes : null;
}

async function open(url: string) {
  if (!inTauri) {
    window.open(url);
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  void openUrl(url);
}

export function PhoneApprovals() {
  useLocale();
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  const [status, setStatus] = useState<PhoneStatus | null>(inTauri ? null : DEMO);
  const [draft, setDraft] = useState("");
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    if (!inTauri) return;
    let off: (() => void) | undefined;
    let gone = false;
    void invoke<PhoneStatus>("phone_status").then((s) => !gone && setStatus(s));
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<PhoneStatus>("phone://status", (e) => setStatus(e.payload)).then((f) => {
        if (gone) f();
        else off = f;
      }),
    );
    return () => {
      gone = true;
      off?.();
    };
  }, []);

  // Not on this system (Linux): the section is left out.
  if (!status?.supported) return null;

  const call = async (command: string, args?: Record<string, unknown>) => {
    if (!inTauri) return;
    setStatus(await invoke<PhoneStatus>(command, args));
  };
  const submitToken = async () => {
    const token = draft.trim();
    if (!token) return;
    if (!inTauri) {
      setStatus({
        ...DEMO,
        bot: "openbar_bot",
        linking: { code: "K7PX2MQA", link: "https://t.me/openbar_bot?start=K7PX2MQA", expires: 0 },
      });
      setDraft("");
      return;
    }
    setStatus({ ...status, checking: true, problem: null });
    const next = await invoke<PhoneStatus>("phone_use_token", { token });
    setStatus(next);
    if (next.bot && !next.problem) setDraft("");
  };

  const problem = status.problem ? <div className="st-error">{problemText(status.problem)}</div> : null;

  return (
    <section className="st-session-sounds st-phone" aria-label={t("windows.settings.sessions.phoneTitle")}>
      <h3 className="st-group-title">{t("windows.settings.sessions.phoneTitle")}</h3>
      <p className="st-card-caption">{t("windows.settings.sessions.phoneCaption")}</p>
      <p className="st-card-caption st-phone-privacy">{t("windows.settings.sessions.phonePrivacy")}</p>

      {status.linked ? (
        <>
          <SubRow
            title={t("windows.settings.sessions.phoneEnable")}
            caption={tf("windows.settings.sessions.phoneLinked", status.linked, status.bot ?? "bot")}
          >
            <Toggle
              label={t("windows.settings.sessions.phoneEnable")}
              checked={prefs.phoneEnabled}
              onChange={(on) => set("phoneEnabled", on)}
            />
          </SubRow>
          {problem}
          <SubRow
            title={t("windows.settings.sessions.phoneAwayAfter")}
            caption={t("windows.settings.sessions.phoneAwayHint")}
          >
            <Select<string>
              label={t("windows.settings.sessions.phoneAwayAfter")}
              value={String(prefs.phoneAwayMinutes)}
              disabled={prefs.phoneSendAlways}
              options={AWAY_CHOICES.map((m) => ({
                value: String(m),
                label: tf("windows.settings.sessions.phoneMinutes", m),
              }))}
              onChange={(value) => set("phoneAwayMinutes", Number(value))}
            />
          </SubRow>
          <SubRow title={t("windows.settings.sessions.phoneSendAlways")}>
            <Toggle
              label={t("windows.settings.sessions.phoneSendAlways")}
              checked={prefs.phoneSendAlways}
              onChange={(on) => set("phoneSendAlways", on)}
            />
          </SubRow>
          <SubRow
            title={t("windows.settings.sessions.phoneQuietHours")}
            caption={t("windows.settings.sessions.phoneQuietHint")}
          >
            <Toggle
              label={t("windows.settings.sessions.phoneQuietHours")}
              checked={prefs.phoneQuietHours}
              onChange={(on) => set("phoneQuietHours", on)}
            />
          </SubRow>
          {prefs.phoneQuietHours ? (
            <div className="st-phone-quiet">
              <input
                className="ag-st-input st-phone-time"
                type="time"
                aria-label={`${t("windows.settings.sessions.phoneQuietHours")} 1`}
                value={timeText(prefs.phoneQuietFrom)}
                onChange={(e) => {
                  const m = timeMinutes(e.target.value);
                  if (m !== null) set("phoneQuietFrom", m);
                }}
              />
              <span aria-hidden="true">–</span>
              <input
                className="ag-st-input st-phone-time"
                type="time"
                aria-label={`${t("windows.settings.sessions.phoneQuietHours")} 2`}
                value={timeText(prefs.phoneQuietTo)}
                onChange={(e) => {
                  const m = timeMinutes(e.target.value);
                  if (m !== null) set("phoneQuietTo", m);
                }}
              />
            </div>
          ) : null}
          <SubRow
            title={t("windows.settings.sessions.phoneHideCode")}
            caption={t("windows.settings.sessions.phoneHideCodeHint")}
          >
            <Toggle
              label={t("windows.settings.sessions.phoneHideCode")}
              checked={prefs.phoneHideCode}
              onChange={(on) => set("phoneHideCode", on)}
            />
          </SubRow>
          <SubRow title={t("windows.settings.sessions.phoneTellEnds")}>
            <Toggle
              label={t("windows.settings.sessions.phoneTellEnds")}
              checked={prefs.phoneTellEnds}
              onChange={(on) => set("phoneTellEnds", on)}
            />
          </SubRow>
          {confirming ? (
            <div className="st-confirm" role="alertdialog" aria-label={t("windows.settings.sessions.phoneUnlinkTitle")}>
              <div className="st-card-title">{t("windows.settings.sessions.phoneUnlinkTitle")}</div>
              <p className="st-card-caption">{t("windows.settings.sessions.phoneUnlinkMessage")}</p>
              <div className="st-confirm-buttons">
                <Button onClick={() => setConfirming(false)}>{t("windows.settings.sessions.remoteCancel")}</Button>
                <Button
                  accent
                  onClick={() => {
                    setConfirming(false);
                    void call("phone_unlink");
                  }}
                >
                  {t("windows.settings.sessions.phoneUnlink")}
                </Button>
              </div>
            </div>
          ) : (
            <div className="st-phone-actions">
              <Button onClick={() => void (inTauri ? invoke("phone_test") : null)}>
                {t("windows.settings.sessions.phoneSendTest")}
              </Button>
              <Button onClick={() => setConfirming(true)}>{t("windows.settings.sessions.phoneUnlink")}</Button>
            </div>
          )}
        </>
      ) : status.linking && status.bot ? (
        <div className="st-phone-link">
          {status.linking.qr ? (
            <div
              className="st-phone-qr"
              role="img"
              aria-label={t("windows.settings.sessions.phoneOpenInTelegram")}
              dangerouslySetInnerHTML={{ __html: status.linking.qr }}
            />
          ) : null}
          <div className="st-phone-code-side">
            <p className="st-card-caption">{tf("windows.settings.sessions.phoneCodeIntro", status.bot)}</p>
            <div className="st-phone-code" aria-live="polite">
              {status.linking.code}
            </div>
            <p className="st-card-caption">{t("windows.settings.sessions.phoneCodeExpires")}</p>
            {problem}
            <div className="st-phone-actions">
              <Button accent onClick={() => void open(status.linking!.link)}>
                {t("windows.settings.sessions.phoneOpenInTelegram")}
              </Button>
              <Button onClick={() => void navigator.clipboard?.writeText(status.linking!.code)}>
                {t("windows.settings.sessions.phoneCopyCode")}
              </Button>
              <Button onClick={() => void call("phone_cancel_link")}>
                {t("windows.settings.sessions.remoteCancel")}
              </Button>
            </div>
          </div>
        </div>
      ) : status.bot ? (
        <SubRow title={tf("windows.settings.sessions.phoneBotReady", status.bot)} caption={problem ?? undefined}>
          <Button accent onClick={() => void call("phone_start_link")}>
            {t("windows.settings.sessions.phoneLinkPhone")}
          </Button>
          <Button onClick={() => void call("phone_unlink")}>{t("windows.settings.sessions.phoneForgetBot")}</Button>
        </SubRow>
      ) : (
        <>
          <ol className="st-phone-steps">
            <li>
              {t("windows.settings.sessions.phoneStep1")}{" "}
              <button type="button" className="st-link" onClick={() => void open("https://t.me/BotFather")}>
                {t("windows.settings.sessions.phoneOpenBotFather")}
              </button>
            </li>
            <li>{t("windows.settings.sessions.phoneStep2")}</li>
            <li>{t("windows.settings.sessions.phoneStep3")}</li>
          </ol>
          <SubRow title={t("windows.settings.sessions.phoneToken")} caption={problem ?? undefined}>
            <input
              className="ag-st-input st-phone-token"
              type="password"
              value={draft}
              placeholder={t("windows.settings.sessions.phoneToken")}
              aria-label={t("windows.settings.sessions.phoneToken")}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void submitToken()}
            />
            <Button accent busy={status.checking} disabled={!draft.trim()} onClick={() => void submitToken()}>
              {t("windows.settings.sessions.phoneUseToken")}
            </Button>
          </SubRow>
        </>
      )}
    </section>
  );
}
