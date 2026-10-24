// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Sessions options: Claude Code and Codex in the island. Claude Code needs Vorssaint's
// hooks in its settings.json; Codex needs nothing, its app-server already
// tells the island everything (spec 02-sessions, 07-codex-sessions).

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

/** The Sessions section's options, inside its card on the Content tab. */
export function SessionsOptions() {
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  const hooks = useHooks();

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
      <SubRow
        title="Claude Code"
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
      <SubRow title="Codex" caption={T.sessions.codexCaption}>
        <Status tone="ok">{T.sessions.codexReady}</Status>
      </SubRow>
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
    </>
  );
}
