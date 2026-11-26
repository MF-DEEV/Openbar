// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sessions options: Claude Code, Codex and the agents that report through
// hooks in the island. Claude Code needs Openbar's hooks in its
// settings.json; Codex needs nothing, its app-server already tells the island
// everything (spec 02-sessions, 07-codex-sessions); Gemini CLI, Copilot CLI,
// OpenCode, Cursor, Pi, Mistral Vibe and Devin CLI each get the island's
// hooks or plugin on Install (docs/more-session-agents.md).

import { useLocale, t, tf } from "../i18n";
import { CLAUDE_DESKTOP, type ClaudeAccount, HOOK_AGENTS, type HookAgent, PROVIDER_NAMES } from "../sessions/types";
import { type HookRow, linkCaption, summary } from "./hooks";
import { openSettingsPage } from "./navigation";
import { ProviderMark } from "../sessions/glyphs";
import type { ClaudeAccountSetting, Grouping, ProgressStyle, SessionPrefs } from "../shared/prefs";
import { Eye, EyeOff, MinusCircle } from "lucide-react";
import { voiceHeard } from "../shared/platform";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { bridge, inTauri, type VoiceModel } from "../shared/bridge";
import { ShortcutField } from "../shared/ui/ShortcutField";
import { formatAccelerator } from "../shared/shortcut";
import { getLanguage } from "../i18n";
import { Select } from "./Select";
import { useSettings } from "./state";
import { Button, Segmented, Status, SubRow, Toggle } from "./ui";

/** Minutes without input the away card counts as a break (#215). */
const AWAY_MINUTES = [5, 10, 15, 30, 60];
import { T } from "./strings";
import { SessionSounds } from "./SessionSounds";
import { SessionReminders } from "./SessionReminders";
import { SessionAutoMode } from "./SessionAutoMode";
import { DiagnosticsSubRow } from "./DiagnosticsDialog";

/** Accounts for designing the page in a plain browser. */
const DEMO_ACCOUNTS: ClaudeAccount[] = [
  {
    id: "c:\\users\\vors\\.claude",
    path: "C:\\Users\\vors\\.claude",
    name: "Default",
    named: false,
    manual: false,
    hidden: false,
    hooks: "installed",
  },
  {
    id: "c:\\users\\vors\\.claude-side",
    path: "C:\\Users\\vors\\.claude-side",
    name: "Side",
    named: false,
    manual: false,
    hidden: false,
    hooks: "missing",
  },
  {
    id: "c:\\users\\vors\\.claude-work",
    path: "C:\\Users\\vors\\.claude-work",
    name: "Work",
    named: false,
    manual: false,
    hidden: false,
    hooks: "installed",
  },
];

/** `~\…` for a folder in the user's home. */
function shortPath(path: string): string {
  const match = /^[A-Za-z]:\\Users\\[^\\]+(\\.*)$/.exec(path);
  return match ? "~" + match[1] : path;
}

const samePath = (a: string, b: string) =>
  a.toLowerCase().replace(/[\\/]+$/, "") === b.toLowerCase().replace(/[\\/]+$/, "");

/** The Claude Code accounts on this PC (#216): each config folder with its
 * name and a way to hide it; a folder Openbar did not find can be added by
 * hand. Whether each holds the hooks is on the Hooks page (#236). */
function ClaudeAccounts() {
  useLocale();
  const saved = useSettings((s) => s.prefs.sessions.claudeAccounts);
  const set = useSettings((s) => s.sessions);
  const [accounts, setAccounts] = useState<ClaudeAccount[] | null>(null);
  const [notAdded, setNotAdded] = useState(false);
  const refresh = useCallback(async () => {
    if (!inTauri) {
      setAccounts(
        DEMO_ACCOUNTS.map((a) => {
          const setting = saved.find((s) => samePath(s.path, a.path));
          return { ...a, name: setting?.name ?? a.name, named: !!setting?.name, hidden: !!setting?.hidden };
        }),
      );
      return;
    }
    setAccounts((await invoke<ClaudeAccount[]>("claude_accounts_status")) ?? []);
  }, [saved]);
  useEffect(() => {
    void refresh();
    // A folder made while the page shows, as by a first session with a new CLAUDE_CONFIG_DIR, joins the list.
    const timer = window.setInterval(() => void refresh(), 5000);
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  const change = (account: ClaudeAccount, patch: Partial<ClaudeAccountSetting>) => {
    const list = [...saved];
    const index = list.findIndex((s) => samePath(s.path, account.path));
    const next = {
      ...(index >= 0 ? list[index] : { path: account.path, ...(account.manual ? { manual: true } : {}) }),
      ...patch,
    };
    if (next.name !== undefined && !next.name.trim()) delete next.name;
    if (!next.hidden) delete next.hidden;
    if (index >= 0) list[index] = next;
    else list.push(next);
    set(
      "claudeAccounts",
      list.filter((s) => s.name || s.hidden || s.manual),
    );
  };
  const add = async () => {
    const path = await bridge.sessions.pickFolder();
    if (!path) return;
    const ok = inTauri ? await invoke<boolean>("claude_accounts_can_add", { path }) : true;
    if (!ok) {
      const hidden = saved.find((s) => samePath(s.path, path) && s.hidden);
      if (hidden)
        set(
          "claudeAccounts",
          saved.map((s) => (s === hidden ? { ...s, hidden: undefined } : s)),
        );
      else setNotAdded(true);
      return;
    }
    setNotAdded(false);
    set("claudeAccounts", [...saved, { path, manual: true }]);
  };

  return (
    <>
      <SubRow
        title={t("windows.claudeAccounts.title")}
        caption={
          <>
            {t("windows.claudeAccounts.hint")}
            {notAdded ? <div className="st-error">{t("windows.claudeAccounts.notAdded")}</div> : null}
          </>
        }
      >
        <Button onClick={() => void add()}>{t("windows.claudeAccounts.add")}</Button>
      </SubRow>
      {(accounts ?? []).map((account) => (
        <div key={account.id} className={`st-account-row${account.hidden ? " is-hidden" : ""}`}>
          <div className="st-account-main">
            <div className="st-account-line">
              <input
                key={account.name}
                className="ag-st-input st-account-name"
                defaultValue={account.name}
                placeholder={t("windows.claudeAccounts.namePlaceholder")}
                aria-label={`${t("windows.claudeAccounts.account")} ${account.name}`}
                maxLength={32}
                onBlur={(event) => {
                  const value = event.currentTarget.value.trim();
                  if (value !== account.name) change(account, { name: value || undefined });
                }}
                onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
              />
              <span className="st-account-path" title={account.path}>
                {shortPath(account.path)}
              </span>
            </div>
            {/* Where its hooks stand is on the Hooks page (#236). */}
            {account.hidden ? <div className="st-account-state">{t("windows.claudeAccounts.hiddenNote")}</div> : null}
          </div>
          {account.manual ? (
            <button
              type="button"
              className="st-account-icon"
              title={t("windows.claudeAccounts.remove")}
              aria-label={`${t("windows.claudeAccounts.remove")} ${account.name}`}
              onClick={() =>
                set(
                  "claudeAccounts",
                  saved.filter((s) => !samePath(s.path, account.path)),
                )
              }
            >
              <MinusCircle size={14} />
            </button>
          ) : (
            <button
              type="button"
              className="st-account-icon"
              title={account.hidden ? t("windows.claudeAccounts.show") : t("windows.claudeAccounts.hide")}
              aria-label={`${account.hidden ? t("windows.claudeAccounts.show") : t("windows.claudeAccounts.hide")} ${account.name}`}
              aria-pressed={account.hidden}
              onClick={() => change(account, { hidden: !account.hidden })}
            >
              {account.hidden ? <Eye size={14} /> : <EyeOff size={14} />}
            </button>
          )}
        </div>
      ))}
    </>
  );
}

interface EditorStatus {
  id: string;
  name: string;
  state: "missing" | "outdated" | "current";
}

/** The IDE bridge in each editor found on this PC. */
function EditorExtensions() {
  useLocale();
  const [editors, setEditors] = useState<EditorStatus[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [failure, setFailure] = useState<{ id: string; reason: string } | null>(null);
  const refresh = useCallback(async () => {
    setEditors(inTauri ? await invoke<EditorStatus[]>("ide_bridge_status") : []);
  }, []);
  useEffect(() => {
    void refresh();
    // The person may add an editor or remove the extension while the window is open.
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);
  const install = async (id: string) => {
    setBusy(id);
    setFailure(null);
    try {
      await invoke("ide_bridge_install", { id });
    } catch (e) {
      setFailure({ id, reason: String(e) });
    }
    try {
      await refresh();
    } finally {
      setBusy(null);
    }
  };
  return (
    <>
      <SubRow title={T.sessions.bridge} caption={T.sessions.bridgeCaption}>
        {editors?.length === 0 ? <Status tone="off">{T.sessions.bridgeNoEditors}</Status> : null}
      </SubRow>
      {editors?.map((editor) => (
        <SubRow
          key={editor.id}
          title={editor.name}
          caption={
            failure?.id === editor.id ? (
              <div className="st-error">{T.sessions.bridgeFailures[failure.reason] ?? failure.reason}</div>
            ) : undefined
          }
        >
          {editor.state === "current" ? (
            <Status tone="ok">{T.sessions.bridgeOn}</Status>
          ) : (
            <>
              {editor.state === "outdated" ? (
                <Status tone="warn">{T.sessions.bridgeStale}</Status>
              ) : (
                <Status tone="off">{T.sessions.bridgeOff}</Status>
              )}
              <Button accent onClick={() => void install(editor.id)} busy={busy === editor.id} disabled={busy !== null}>
                {editor.state === "outdated" ? T.sessions.bridgeUpdate : T.sessions.bridgeInstall}
              </Button>
            </>
          )}
        </SubRow>
      ))}
    </>
  );
}

const AGENT_PREF = {
  gemini: "agentGemini",
  copilot: "agentCopilot",
  opencode: "agentOpencode",
  cursor: "agentCursor",
  pi: "agentPi",
  vibe: "agentVibe",
  devin: "agentDevin",
  antigravity: "agentAntigravity",
  grok: "agentGrok",
  zcode: "agentZcode",
  hermes: "agentHermes",
  qwen: "agentQwen",
  droid: "agentDroid",
  cline: "agentCline",
  kimi: "agentKimi",
  kiro: "agentKiro",
} as const satisfies Record<HookAgent, keyof SessionPrefs>;

/** Gemini CLI, Copilot CLI, OpenCode, Cursor, Pi, Mistral Vibe, Devin CLI,
 * Google Antigravity, Grok CLI and ZCode: each found on this PC is connected by itself, and its
 * switch shows it in the list or takes its hooks out. Where its hooks stand
 * is on the Hooks page (#236). */
function MoreAgents() {
  useLocale();
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  return (
    <>
      <SubRow
        title={t("windows.settings.sessions.moreAgents")}
        caption={t("windows.settings.sessions.moreAgentsCaption")}
      />
      {HOOK_AGENTS.map((agent) => (
        <SubRow
          key={agent}
          title={PROVIDER_NAMES[agent]}
          caption={agent === "hermes" ? <HermesCaption shown={prefs[AGENT_PREF[agent]]} /> : undefined}
        >
          <span className="st-agent-mark" aria-hidden>
            <ProviderMark provider={agent} size={14} />
          </span>
          <Toggle
            label={tf("windows.settings.sessions.showAgent", PROVIDER_NAMES[agent])}
            checked={prefs[AGENT_PREF[agent]]}
            onChange={(on) => set(AGENT_PREF[agent], on)}
          />
        </SubRow>
      ))}
    </>
  );
}

/** Under Hermes's switch (#244): Hermes runs a new hook only once the person
 * allowed it in its terminal, and its app and gateway skip one nobody
 * allowed. Read from Hermes's allowlist, never written. */
function HermesCaption({ shown }: { shown: boolean }) {
  useLocale();
  const [waiting, setWaiting] = useState<number | undefined>(undefined);
  useEffect(() => {
    if (!inTauri || !shown) return;
    let live = true;
    void invoke<{ agent: string; waitingApproval?: number }[]>("sessions_agent_hooks_status")
      .then((statuses) => {
        if (live) setWaiting(statuses.find((s) => s.agent === "hermes")?.waitingApproval);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [shown]);
  if (!shown || waiting === undefined) return <>{t("windows.settings.sessions.hermesOff")}</>;
  if (waiting > 0) return <>{tf("windows.settings.sessions.hermesWaiting", String(waiting))}</>;
  return <>{t("windows.settings.sessions.hermesApproved")}</>;
}

/** The Claude app's two switches (#246): its Code tab's sessions, heard
 * through Claude Code's hooks, and Cowork's, read from its files only once
 * turned on. Shown only where the app is installed. */
function ClaudeDesktopAgent() {
  useLocale();
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  const [installed, setInstalled] = useState(!inTauri);
  useEffect(() => {
    if (!inTauri) return;
    void invoke<boolean>("sessions_claude_desktop_installed")
      .then(setInstalled)
      .catch(() => setInstalled(false));
  }, []);
  if (!installed) return null;
  return (
    <>
      <SubRow title={CLAUDE_DESKTOP}>
        <span className="st-agent-mark" aria-hidden>
          <ProviderMark provider="claude" size={14} />
        </span>
      </SubRow>
      <SubRow
        title={t("windows.settings.sessions.claudeDesktopCodeTab")}
        caption={t("windows.settings.sessions.claudeDesktopCodeTabCaption")}
      >
        <Toggle
          label={t("windows.settings.sessions.claudeDesktopCodeTab")}
          checked={prefs.claudeDesktop}
          onChange={(on) => set("claudeDesktop", on)}
        />
      </SubRow>
      <SubRow
        title={t("windows.settings.sessions.claudeDesktopCowork")}
        caption={t("windows.settings.sessions.claudeDesktopCoworkCaption")}
      >
        <Toggle
          label={t("windows.settings.sessions.claudeDesktopCowork")}
          checked={prefs.claudeCowork && prefs.claudeDesktop}
          disabled={!prefs.claudeDesktop}
          onChange={(on) => set("claudeCowork", on)}
        />
      </SubRow>
    </>
  );
}

/** Where every agent's hooks stand, in a line, and the page that checks and
 * fixes them (#236). */
function HooksLink() {
  useLocale();
  const [caption, setCaption] = useState<string | undefined>(undefined);
  useEffect(() => {
    const refresh = async () => {
      if (!inTauri) return setCaption(linkCaption(3, 1));
      const rows = await invoke<HookRow[]>("hooks_page_status").catch(() => null);
      if (rows) {
        const { working, attention } = summary(rows);
        setCaption(linkCaption(working, attention));
      }
    };
    void refresh();
    window.addEventListener("focus", refresh);
    let off: (() => void) | undefined;
    let gone = false;
    if (inTauri) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen("hooks://changed", () => void refresh()).then((f) => (gone ? f() : (off = f))),
      );
    }
    return () => {
      gone = true;
      window.removeEventListener("focus", refresh);
      off?.();
    };
  }, []);
  return (
    <SubRow title={t("windows.hooks.title")} caption={caption}>
      <Button onClick={() => openSettingsPage("hooks")}>{t("windows.hooks.open")}</Button>
    </SubRow>
  );
}

/** How a host stands (sessions/remote.rs `State`). */
type RemoteState =
  | { state: "connecting" }
  | { state: "connected"; hasClient: boolean; hooksInstalled: boolean }
  | { state: "disconnected"; reason: string };

interface RemoteHostStatus {
  alias: string;
  state: RemoteState | null;
  busy: boolean;
  failure: string | null;
}

/** An alias ssh takes as a destination and never as an option (sessions/remote.rs). */
export function isValidAlias(alias: string): boolean {
  return alias.length > 0 && alias.length <= 255 && !alias.startsWith("-") && /^[A-Za-z0-9._\-@:[\]%]+$/.test(alias);
}

export function remoteStateText(state: RemoteState | null): string {
  if (!state || state.state === "connecting") return t("windows.settings.sessions.remoteConnecting");
  if (state.state === "disconnected") return tf("windows.settings.sessions.remoteDisconnected", state.reason);
  if (state.hooksInstalled) return t("windows.settings.sessions.remoteReady");
  return state.hasClient
    ? t("windows.settings.sessions.remoteNotSetUp")
    : t("windows.settings.sessions.remoteNoClient");
}

/** Sessions on machines reached over SSH: the switch, the hosts by their
 * ssh config alias, and setting a host up or taking Openbar off it, each
 * asked first, in place. */
function RemoteHosts() {
  useLocale();
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  const [statuses, setStatuses] = useState<RemoteHostStatus[]>([]);
  const [draft, setDraft] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [confirming, setConfirming] = useState<{ alias: string; install: boolean } | null>(null);
  const hosts = prefs.remoteHosts;
  const enabled = prefs.remoteHostsEnabled;
  const refresh = useCallback(async () => {
    if (!inTauri) {
      setStatuses(
        hosts.map((alias) => ({
          alias,
          state: { state: "connected", hasClient: true, hooksInstalled: false },
          busy: false,
          failure: null,
        })),
      );
      return;
    }
    setStatuses(await invoke<RemoteHostStatus[]>("sessions_remote_hosts_status", { hosts }));
  }, [hosts]);
  useEffect(() => {
    if (!enabled) return;
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2000);
    let off: (() => void) | undefined;
    if (inTauri) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen("sessions-remote-hosts", () => void refresh()).then((f) => (off = f)),
      );
    }
    return () => {
      window.clearInterval(timer);
      off?.();
    };
  }, [enabled, refresh]);
  const add = () => {
    const alias = draft.trim();
    if (!isValidAlias(alias)) {
      setInvalid(alias.length > 0);
      return;
    }
    setInvalid(false);
    setDraft("");
    if (!hosts.includes(alias)) set("remoteHosts", [...hosts, alias]);
  };
  const change = async (alias: string, install: boolean) => {
    setConfirming(null);
    try {
      if (inTauri) await invoke("sessions_remote_hosts_change", { alias, install });
    } catch {
      // The failure is shown with the host's status.
    }
    void refresh();
  };
  return (
    <>
      <SubRow title={t("windows.settings.sessions.remoteTitle")} caption={t("windows.settings.sessions.remoteCaption")}>
        <Toggle
          label={t("windows.settings.sessions.remoteTitle")}
          checked={enabled}
          onChange={(on) => set("remoteHostsEnabled", on)}
        />
      </SubRow>
      {enabled
        ? hosts.map((alias) => {
            const status = statuses.find((s) => s.alias === alias);
            const state = status?.state ?? null;
            const connected = state?.state === "connected" ? state : null;
            const asking = confirming?.alias === alias ? confirming : null;
            return (
              <SubRow
                key={alias}
                title={alias}
                caption={
                  <>
                    {remoteStateText(state)}
                    {status?.failure ? <div className="st-error">{status.failure}</div> : null}
                    {asking ? (
                      <div className="st-remote-confirm">
                        {tf(
                          asking.install
                            ? "windows.settings.sessions.remoteConfirmSetUp"
                            : "windows.settings.sessions.remoteConfirmRemove",
                          alias,
                        )}
                      </div>
                    ) : null}
                  </>
                }
              >
                {asking ? (
                  <>
                    <Button accent onClick={() => void change(alias, asking.install)}>
                      {asking.install
                        ? t("windows.settings.sessions.remoteSetUp")
                        : t("windows.settings.sessions.remoteRemoveHooks")}
                    </Button>
                    <Button onClick={() => setConfirming(null)}>{t("windows.settings.sessions.remoteCancel")}</Button>
                  </>
                ) : connected && (connected.hooksInstalled || connected.hasClient) ? (
                  <Button
                    busy={status?.busy}
                    onClick={() => setConfirming({ alias, install: !connected.hooksInstalled })}
                  >
                    {connected.hooksInstalled
                      ? t("windows.settings.sessions.remoteRemoveHooks")
                      : t("windows.settings.sessions.remoteSetUp")}
                  </Button>
                ) : null}
                {asking ? null : (
                  <Button
                    onClick={() =>
                      set(
                        "remoteHosts",
                        hosts.filter((h) => h !== alias),
                      )
                    }
                  >
                    {t("windows.settings.sessions.remoteDelete")}
                  </Button>
                )}
              </SubRow>
            );
          })
        : null}
      {enabled ? (
        <SubRow
          title={t("windows.settings.sessions.remoteAdd")}
          caption={invalid ? <div className="st-error">{t("windows.settings.sessions.remoteInvalid")}</div> : undefined}
        >
          <input
            className="ag-st-input st-remote-input"
            type="text"
            value={draft}
            placeholder={t("windows.settings.sessions.remotePlaceholder")}
            aria-label={t("windows.settings.sessions.remotePlaceholder")}
            spellCheck={false}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
          <Button onClick={add} disabled={!draft.trim()}>
            {t("windows.settings.sessions.remoteAddButton")}
          </Button>
        </SubRow>
      ) : null}
    </>
  );
}

/** The speech model on this PC; a stand-in outside the app. */
function useVoiceModel() {
  const [model, setModel] = useState<VoiceModel | null>(null);
  useEffect(() => {
    if (!inTauri) {
      setModel({ phase: "missing", downloaded: 0, size: 59_707_625, error: null });
      return;
    }
    void bridge.voice.model().then((m) => m && setModel(m));
    const off = bridge.voice.onModel(setModel);
    return () => void off.then((f) => f());
  }, []);
  const megabytes = (bytes: number) => `${Math.round(bytes / 1_000_000)} MB`;
  return {
    model,
    megabytes,
    download: () => void bridge.voice.downloadModel().then((m) => m && setModel(m)),
    remove: () => void bridge.voice.deleteModel().then((m) => m && setModel(m)),
  };
}

/** Languages whisper's model hears well, named in the app's language. */
const VOICE_LANGUAGES = [
  "en",
  "de",
  "es",
  "fr",
  "it",
  "ja",
  "ko",
  "pt",
  "ru",
  "sk",
  "tr",
  "uk",
  "zh",
  "pl",
  "nl",
  "cs",
  "sv",
];

function VoiceReplies({
  prefs,
  set,
}: {
  prefs: SessionPrefs;
  set: <K extends keyof SessionPrefs>(key: K, value: SessionPrefs[K]) => void;
}) {
  useLocale();
  const voice = useVoiceModel();
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const names = new Intl.DisplayNames([getLanguage()], { type: "language" });
  const languages = [
    { value: "", label: t("windows.settings.sessions.voiceLanguageApp") },
    { value: "auto", label: t("windows.settings.sessions.voiceLanguageAuto") },
    ...VOICE_LANGUAGES.map((code) => ({ value: code, label: names.of(code) ?? code })).sort((a, b) =>
      a.label.localeCompare(b.label),
    ),
  ];
  const model = voice.model;
  return (
    <>
      <SubRow title={t("windows.settings.sessions.voice")} caption={t("windows.settings.sessions.voiceCaption")}>
        <Toggle
          label={t("windows.settings.sessions.voice")}
          checked={prefs.voiceReplies}
          onChange={(on) => set("voiceReplies", on)}
        />
      </SubRow>
      {prefs.voiceReplies ? (
        <>
          <SubRow
            title={t("windows.settings.sessions.voiceModel")}
            caption={
              <>
                {t("windows.settings.sessions.voiceModelCaption")}
                {model?.phase === "failed" && model.error ? (
                  <div className="st-error">{tf("windows.settings.sessions.voiceModelFailed", model.error)}</div>
                ) : null}
              </>
            }
          >
            {model?.phase === "ready" ? (
              <>
                <Status tone="ok">{t("windows.settings.sessions.voiceModelReady")}</Status>
                <Button onClick={voice.remove}>{t("windows.settings.sessions.voiceModelDelete")}</Button>
              </>
            ) : model?.phase === "downloading" ? (
              <Status tone="warn">
                {tf(
                  "windows.settings.sessions.voiceModelDownloading",
                  `${Math.floor((model.downloaded / Math.max(1, model.size)) * 100)}%`,
                )}
              </Status>
            ) : model ? (
              <>
                <Status tone="off">{t("windows.settings.sessions.voiceModelMissing")}</Status>
                <Button accent onClick={voice.download}>
                  {tf("windows.settings.sessions.voiceModelDownload", voice.megabytes(model.size))}
                </Button>
              </>
            ) : null}
          </SubRow>
          <SubRow title={t("windows.settings.sessions.voiceLanguage")}>
            <Select
              label={t("windows.settings.sessions.voiceLanguage")}
              value={prefs.voiceLanguage}
              options={languages}
              onChange={(value) => set("voiceLanguage", value)}
            />
          </SubRow>
          <SubRow
            title={t("windows.settings.sessions.voiceShortcut")}
            caption={
              <>
                {t("windows.settings.sessions.voiceShortcutCaption")}
                {shortcutError ? <div className="st-error">{shortcutError}</div> : null}
              </>
            }
          >
            {prefs.voiceShortcutEnabled ? (
              <div className="st-island-shortcut">
                <span className="st-keys" aria-label={t("windows.settings.configuredShortcut")}>
                  {formatAccelerator(prefs.voiceShortcut)
                    .split("+")
                    .map((key, i) => (
                      <kbd key={i}>{key}</kbd>
                    ))}
                </span>
                <ShortcutField
                  className="st-button st-shortcut"
                  label={t("windows.settings.sessions.voiceShortcut")}
                  actionLabel={T.behavior.shortcutChange}
                  recordingLabel={T.behavior.shortcutPressKeys}
                  value={prefs.voiceShortcut}
                  onRecording={(on) => {
                    if (on) setShortcutError(null);
                  }}
                  onInvalid={() => setShortcutError(T.behavior.shortcutInvalid)}
                  onError={(e) => setShortcutError(String(e))}
                  onChange={(accelerator) => {
                    if (accelerator === useSettings.getState().prefs.island.shortcut) {
                      setShortcutError(t("windows.settings.sessions.voiceShortcutTaken"));
                      return;
                    }
                    set("voiceShortcut", accelerator);
                  }}
                />
              </div>
            ) : null}
            <Toggle
              label={t("windows.settings.sessions.voiceShortcut")}
              checked={prefs.voiceShortcutEnabled}
              onChange={(on) => set("voiceShortcutEnabled", on)}
            />
          </SubRow>
          <SubRow
            title={t("windows.settings.sessions.voiceSendOnStop")}
            caption={t("windows.settings.sessions.voiceSendOnStopCaption")}
          >
            <Toggle
              label={t("windows.settings.sessions.voiceSendOnStop")}
              checked={prefs.voiceSendOnStop}
              onChange={(on) => set("voiceSendOnStop", on)}
            />
          </SubRow>
        </>
      ) : null}
    </>
  );
}

/** The Sessions section's options, inside its card on the Content tab. */
type CodexStatus = "connected" | "notRunning" | "notFound";

/** Whether the island hears Codex now; checked again when the window comes back. */
function useCodexStatus(): CodexStatus | null {
  const [status, setStatus] = useState<CodexStatus | null>(null);
  useEffect(() => {
    const check = () =>
      inTauri ? void invoke<CodexStatus>("sessions_codex_status").then(setStatus) : setStatus("notRunning");
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, []);
  return status;
}

export function SessionsOptions() {
  useLocale();
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  const codex = useCodexStatus();

  return (
    <>
      <p className="st-note">{T.sessions.lead}</p>
      <HooksLink />
      <ClaudeAccounts />
      <EditorExtensions />
      <SubRow title={t("windows.settings.codex")} caption={T.sessions.codexCaption}>
        {codex === "connected" ? (
          <Status tone="ok">{T.sessions.codexConnected}</Status>
        ) : codex === "notRunning" ? (
          <Status tone="off">{T.sessions.codexNotRunning}</Status>
        ) : codex === "notFound" ? (
          <Status tone="off">{T.sessions.codexNotFound}</Status>
        ) : null}
      </SubRow>
      <MoreAgents />
      <ClaudeDesktopAgent />
      <SubRow title={T.sessions.live} caption={T.sessions.liveCaption}>
        <Toggle label={T.sessions.live} checked={prefs.live} onChange={(on) => set("live", on)} />
      </SubRow>
      <SubRow title={T.sessions.autoExpand} caption={T.sessions.autoExpandCaption}>
        <Toggle label={T.sessions.autoExpand} checked={prefs.autoExpand} onChange={(on) => set("autoExpand", on)} />
      </SubRow>
      <SessionSounds prefs={prefs} set={set} />
      <SessionReminders prefs={prefs} set={set} />
      <SessionAutoMode prefs={prefs} set={set} />
      <SubRow title={T.sessions.previews} caption={T.sessions.previewsCaption}>
        <Toggle label={T.sessions.previews} checked={prefs.previews} onChange={(on) => set("previews", on)} />
      </SubRow>
      <SubRow title={T.sessions.buddies} caption={T.sessions.buddiesCaption}>
        <Toggle label={T.sessions.buddies} checked={prefs.buddies} onChange={(on) => set("buddies", on)} />
      </SubRow>
      <SubRow title={T.sessions.limits} caption={T.sessions.limitsCaption}>
        <Toggle label={T.sessions.limits} checked={prefs.limits} onChange={(on) => set("limits", on)} />
      </SubRow>
      <SubRow title={T.sessions.grouping} caption={T.sessions.groupingCaption}>
        <Segmented<Grouping>
          label={T.sessions.grouping}
          value={prefs.grouping}
          options={[
            { value: "none", label: T.sessions.groupNone },
            { value: "project", label: T.sessions.groupByProject },
            { value: "agent", label: T.sessions.groupByAgent },
          ]}
          onChange={(v) => set("grouping", v)}
        />
      </SubRow>
      <SubRow title={T.sessions.progressStyle} caption={T.sessions.progressStyleCaption}>
        <Segmented<ProgressStyle>
          label={T.sessions.progressStyle}
          value={prefs.progressStyle}
          options={[
            { value: "ring", label: T.sessions.progressRing },
            { value: "dots", label: T.sessions.progressDots },
            { value: "bar", label: T.sessions.progressBar },
          ]}
          onChange={(v) => set("progressStyle", v)}
        />
      </SubRow>
      <SubRow title={T.sessions.awayCard} caption={T.sessions.awayCardCaption}>
        <Toggle label={T.sessions.awayCard} checked={prefs.awayCard} onChange={(on) => set("awayCard", on)} />
      </SubRow>
      {prefs.awayCard ? (
        <SubRow title={T.sessions.awayMinutes}>
          <Select<string>
            label={T.sessions.awayMinutes}
            value={String(prefs.awayMinutes)}
            options={AWAY_MINUTES.map((n) => ({ value: String(n), label: T.sessions.awayMinutesValue(n) }))}
            onChange={(v) => set("awayMinutes", Number(v))}
          />
        </SubRow>
      ) : null}
      {voiceHeard() ? <VoiceReplies prefs={prefs} set={set} /> : null}
      <RemoteHosts />
      <DiagnosticsSubRow />
    </>
  );
}
