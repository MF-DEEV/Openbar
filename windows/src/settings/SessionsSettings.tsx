// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sessions options: Claude Code, Codex and the agents that report through
// hooks in the island. Claude Code needs Vorssaint's hooks in its
// settings.json; Codex needs nothing, its app-server already tells the island
// everything (spec 02-sessions, 07-codex-sessions); Gemini CLI, Copilot CLI,
// OpenCode and Cursor each get the island's hooks or plugin on Install
// (docs/more-session-agents.md).

import { type StringKey, useLocale, t, tf } from "../i18n";
import { HOOK_AGENTS, type HookAgent, PROVIDER_NAMES } from "../sessions/types";
import { ProviderMark } from "../sessions/glyphs";
import type { SessionPrefs } from "../shared/prefs";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { inTauri } from "../shared/bridge";
import { useSettings } from "./state";
import { Button, Status, SubRow, Toggle } from "./ui";
import { T } from "./strings";

interface HooksStatus {
  installed: boolean;
  stale: boolean;
  error: string | null;
}

function useHooks() {
  const [status, setStatus] = useState<HooksStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    if (!inTauri) {
      setStatus((s) => s ?? { installed: false, stale: false, error: null });
      return;
    }
    setStatus(await invoke<HooksStatus>("claude_hooks_status"));
  }, []);
  useEffect(() => {
    void refresh();
    // The person may edit settings.json by hand while the window is open.
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);
  const run = async (command: "claude_hooks_install" | "claude_hooks_remove") => {
    setBusy(true);
    setError(null);
    try {
      if (inTauri) await invoke(command);
      else setStatus({ installed: command === "claude_hooks_install", stale: false, error: null });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  return { status, busy, error, install: () => run("claude_hooks_install"), remove: () => run("claude_hooks_remove") };
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

type AgentHookState = "installed" | "outdated" | "missing" | "unreadable";
interface AgentHooksStatus {
  agent: HookAgent;
  state: AgentHookState;
  present: boolean;
  failure?: string;
}

const AGENT_PREF = {
  gemini: "agentGemini",
  copilot: "agentCopilot",
  opencode: "agentOpencode",
  cursor: "agentCursor",
} as const satisfies Record<HookAgent, keyof SessionPrefs>;

const AGENT_HINT: Record<HookAgent, StringKey> = {
  gemini: "windows.settings.sessions.agentHintGemini",
  copilot: "windows.settings.sessions.agentHintCopilot",
  opencode: "windows.settings.sessions.agentHintOpencode",
  cursor: "windows.settings.sessions.agentHintCursor",
};

/** What an agent's hooks are doing now that they follow its switch by themselves. */
function AgentState({ status, shown }: { status: AgentHooksStatus; shown: boolean }) {
  useLocale();
  if (status.state === "unreadable")
    return <Status tone="warn">{t("windows.settings.sessions.agentUnreadable")}</Status>;
  if (!shown) return <Status tone="off">{t("windows.settings.sessions.agentOff")}</Status>;
  if (status.state === "installed") return <Status tone="ok">{t("windows.settings.sessions.agentConnected")}</Status>;
  if (!status.present) return <Status tone="off">{T.sessions.codexNotFound}</Status>;
  return (
    <Status tone="warn">
      {t(
        status.state === "outdated"
          ? "windows.settings.sessions.agentUpdating"
          : "windows.settings.sessions.agentConnecting",
      )}
    </Status>
  );
}

/** Gemini CLI, Copilot CLI, OpenCode and Cursor: each found on this PC is
 * connected by itself, and its switch shows it in the list or takes its hooks out. */
function MoreAgents() {
  useLocale();
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  const [statuses, setStatuses] = useState<AgentHooksStatus[] | null>(null);
  const [busy, setBusy] = useState<HookAgent | null>(null);
  const [failure, setFailure] = useState<{ agent: HookAgent; reason: string } | null>(null);
  const refresh = useCallback(async () => {
    setStatuses(
      inTauri
        ? await invoke<AgentHooksStatus[]>("sessions_agent_hooks_status")
        : (statuses ?? HOOK_AGENTS.map((agent) => ({ agent, state: "missing" as const, present: agent !== "cursor" }))),
    );
  }, [statuses]);
  useEffect(() => {
    void refresh();
    // The person may edit an agent's settings by hand while the window is open.
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const run = async (agent: HookAgent, install: boolean) => {
    setBusy(agent);
    setFailure(null);
    try {
      if (inTauri) {
        await invoke("sessions_agent_hooks_install", { agent, install });
        await refresh();
      } else {
        setStatuses((list) =>
          (list ?? []).map((s) => (s.agent === agent ? { ...s, state: install ? "installed" : "missing" } : s)),
        );
      }
    } catch (e) {
      setFailure({ agent, reason: String(e) });
    } finally {
      setBusy(null);
    }
  };
  return (
    <>
      <SubRow
        title={t("windows.settings.sessions.moreAgents")}
        caption={t("windows.settings.sessions.moreAgentsCaption")}
      />
      {HOOK_AGENTS.map((agent) => {
        const status = statuses?.find((s) => s.agent === agent);
        const shown = prefs[AGENT_PREF[agent]];
        return (
          <SubRow
            key={agent}
            title={PROVIDER_NAMES[agent]}
            caption={
              <>
                {t(AGENT_HINT[agent])}
                {failure?.agent === agent || status?.failure ? (
                  <div className="st-error">{failure?.agent === agent ? failure.reason : status?.failure}</div>
                ) : null}
              </>
            }
          >
            <span className="st-agent-mark" aria-hidden>
              <ProviderMark provider={agent} size={14} />
            </span>
            {status ? <AgentState status={status} shown={shown} /> : null}
            {(status?.failure || failure?.agent === agent) && status?.state !== "unreadable" ? (
              <Button onClick={() => void run(agent, shown)} busy={busy === agent} disabled={busy !== null}>
                {t("windows.settings.sessions.tryAgain")}
              </Button>
            ) : null}
            <Toggle
              label={tf("windows.settings.sessions.showAgent", PROVIDER_NAMES[agent])}
              checked={shown}
              onChange={(on) => {
                set(AGENT_PREF[agent], on);
                // The service puts the hooks in or takes them out; read the result back.
                window.setTimeout(() => void refresh(), 800);
              }}
            />
          </SubRow>
        );
      })}
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
  const hooks = useHooks();
  const codex = useCodexStatus();

  const hookState = !hooks.status ? null : hooks.status.error ? (
    <Status tone="warn">{T.sessions.hooksProblem}</Status>
  ) : hooks.status.installed ? (
    <Status tone="ok">{T.sessions.hooksOn}</Status>
  ) : hooks.status.stale ? (
    <Status tone="warn">{T.sessions.hooksStale}</Status>
  ) : (
    <Status tone="off">{T.sessions.hooksOff}</Status>
  );

  return (
    <>
      <p className="st-note">{T.sessions.lead}</p>
      <SubRow
        title={t("windows.settings.claudeCode")}
        caption={
          <>
            {hooks.status?.installed ? T.sessions.claudeOnCaption : T.sessions.claudeOffCaption}
            {hooks.error ? <div className="st-error">{hooks.error}</div> : null}
            {hooks.status?.error ? <div className="st-error">{hooks.status.error}</div> : null}
          </>
        }
      >
        {hookState}
        {hooks.status?.installed ? (
          <Button onClick={hooks.remove} busy={hooks.busy}>
            {T.sessions.remove}
          </Button>
        ) : (
          <Button accent onClick={hooks.install} busy={hooks.busy} disabled={!hooks.status}>
            {hooks.status?.stale ? T.sessions.update : T.sessions.install}
          </Button>
        )}
      </SubRow>
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
      <SubRow title={T.sessions.live} caption={T.sessions.liveCaption}>
        <Toggle label={T.sessions.live} checked={prefs.live} onChange={(on) => set("live", on)} />
      </SubRow>
      <SubRow title={T.sessions.autoExpand} caption={T.sessions.autoExpandCaption}>
        <Toggle label={T.sessions.autoExpand} checked={prefs.autoExpand} onChange={(on) => set("autoExpand", on)} />
      </SubRow>
      <SubRow title={T.sessions.previews} caption={T.sessions.previewsCaption}>
        <Toggle label={T.sessions.previews} checked={prefs.previews} onChange={(on) => set("previews", on)} />
      </SubRow>
      <SubRow title={T.sessions.buddies} caption={T.sessions.buddiesCaption}>
        <Toggle label={T.sessions.buddies} checked={prefs.buddies} onChange={(on) => set("buddies", on)} />
      </SubRow>
      <SubRow title={T.sessions.limits} caption={T.sessions.limitsCaption}>
        <Toggle label={T.sessions.limits} checked={prefs.limits} onChange={(on) => set("limits", on)} />
      </SubRow>
    </>
  );
}
