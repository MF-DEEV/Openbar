// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Whole sessions running in the background, with no terminal
// (NotchClaudeBackgroundViews.swift): their mark, what can be done with them,
// their recent output, and the page that starts a new session.

import { Check, FileText, Folder, FolderPlus, Square, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useLocale } from "../i18n";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { canRemoveBackground, canStopBackground, problemText } from "./format";
import { S } from "./strings";
import { useSessions } from "./store";
import type { RecentFolder, Session } from "./types";

/** Says a session runs in the background, in words, on its row. */
export function BackgroundMark() {
  useLocale();
  return (
    <span className="s-background-mark" title={S.backgroundHelp}>
      {S.backgroundMark}
    </span>
  );
}

/** Logs, Stop, Remove. Stopping and removing ask first, in place. */
export function BackgroundBar({
  session,
  showingLogs,
  onLogs,
}: {
  session: Session;
  showingLogs: boolean;
  onLogs: (showing: boolean) => void;
}) {
  useLocale();
  const [asking, setAsking] = useState<"stop" | "remove" | null>(null);
  const [working, setWorking] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const run = async (action: "stop" | "remove") => {
    setWorking(true);
    const sessions = bridge.sessions;
    const failure = await (action === "stop"
      ? sessions.stopBackground(session.id)
      : sessions.removeBackground(session.id));
    setWorking(false);
    setAsking(null);
    setProblem(problemText(failure));
  };

  return (
    <div className="s-background-bar">
      {asking ? (
        <div className="s-background-row">
          <span className="s-background-ask">{asking === "stop" ? S.stopConfirm : S.removeConfirm}</span>
          <button type="button" className="s-background-button" onClick={() => setAsking(null)}>
            {S.keepSession}
          </button>
          <button
            type="button"
            className="s-background-button is-destructive"
            disabled={working}
            onClick={() => void run(asking)}
          >
            {asking === "stop" ? <Square size={10} /> : <Trash2 size={11} />}
            {asking === "stop" ? S.stopSession : S.removeSession}
          </button>
        </div>
      ) : (
        <div className="s-background-row">
          <BackgroundMark />
          <span className="s-background-gap" />
          <button
            type="button"
            className={`s-background-button${showingLogs ? " is-selected" : ""}`}
            aria-pressed={showingLogs}
            onClick={() => {
              onLogs(!showingLogs);
              // Logs read in the enlarged page.
              if (!showingLogs) useIsland.setState({ sessionsEnlarged: true });
            }}
          >
            <FileText size={11} />
            {S.logs}
          </button>
          {canStopBackground(session) ? (
            <button type="button" className="s-background-button" onClick={() => (setProblem(null), setAsking("stop"))}>
              <Square size={10} />
              {S.stopSession}
            </button>
          ) : null}
          {canRemoveBackground(session) ? (
            <button
              type="button"
              className="s-background-button"
              onClick={() => (setProblem(null), setAsking("remove"))}
            >
              <Trash2 size={11} />
              {S.removeSession}
            </button>
          ) : null}
        </div>
      )}
      {problem ? (
        <span className="s-background-problem" role="alert">
          {problem}
        </span>
      ) : null}
    </div>
  );
}

/** A background session's recent output, read again while it shows. */
export function BackgroundLogs({ session }: { session: Session }) {
  useLocale();
  const [logs, setLogs] = useState<string | null | undefined>(undefined);
  const ended = session.background?.ended === true;
  useEffect(() => {
    let gone = false;
    const load = async () => {
      const text = await bridge.sessions.backgroundLogs(session.id);
      if (!gone) setLogs(text ?? null);
    };
    void load();
    const timer = ended ? undefined : setInterval(() => void load(), 5000);
    return () => {
      gone = true;
      if (timer) clearInterval(timer);
    };
  }, [session.id, ended]);
  return (
    <div className="s-background-logs">
      <span className="s-section-title">{S.logsTitle}</span>
      <div className="s-background-screen" ref={(node) => node?.scrollTo({ top: node.scrollHeight })}>
        {logs ? <pre>{logs}</pre> : logs === undefined ? null : <p>{logs === null ? S.logsGone : S.logsEmpty}</p>}
      </div>
    </div>
  );
}

/** Starts a session: which folder, what to say first, background or terminal. */
export function NewSession() {
  useLocale();
  const available = useSessions((s) => s.snapshot.backgroundAvailable === true);
  const [folders, setFolders] = useState<RecentFolder[]>([]);
  const [chosen, setChosen] = useState<RecentFolder | null>(null);
  const [prompt, setPrompt] = useState("");
  const [inBackground, setInBackground] = useState(true);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    void bridge.sessions.recentFolders().then((found) => {
      setFolders(found ?? []);
      setChosen((current) => current ?? found?.[0] ?? null);
    });
  }, []);

  // Escape steps back to the list before it reaches the island.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopImmediatePropagation();
      useSessions.setState({ composing: false });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const browse = async () => {
    const path = await bridge.sessions.pickFolder();
    if (!path) return;
    const existing = folders.find((folder) => folder.path === path);
    const folder = existing ?? {
      path,
      name: path.split(/[\\/]/).filter(Boolean).pop() ?? path,
      configHome: folders[0]?.configHome,
    };
    if (!existing) setFolders([folder, ...folders]);
    setChosen(folder);
    setProblem(null);
  };

  const start = async () => {
    if (!chosen) return;
    setStarting(true);
    setProblem(null);
    const failure = await bridge.sessions.start(
      chosen.path,
      chosen.configHome,
      prompt.trim(),
      inBackground && available,
    );
    setStarting(false);
    if (failure) setProblem(problemText(failure));
    else if (failure === null) useSessions.setState({ composing: false });
    else setProblem(S.commandFailed);
  };

  return (
    <div className="s-new-session">
      <div className="s-new-folders" role="listbox" aria-label={S.newSession}>
        {folders.length === 0 ? <span className="s-new-none">{S.noRecentFolders}</span> : null}
        {folders.map((folder) => {
          const selected = chosen?.path === folder.path;
          return (
            <button
              key={folder.path}
              type="button"
              role="option"
              aria-selected={selected}
              aria-label={folder.path}
              className={`s-new-folder${selected ? " is-selected" : ""}`}
              onClick={() => {
                setChosen(folder);
                setProblem(null);
              }}
            >
              {selected ? <Check size={12} className="s-new-check" /> : <Folder size={12} />}
              <span className="s-new-name">{folder.name}</span>
              <span className="s-new-path">{folder.path}</span>
            </button>
          );
        })}
        <button type="button" className="s-new-folder" onClick={() => void browse()}>
          <FolderPlus size={12} />
          <span className="s-new-name">{S.browseFolder}</span>
        </button>
      </div>
      <input
        className="s-new-prompt"
        value={prompt}
        placeholder={S.firstPrompt}
        aria-label={S.firstPrompt}
        onChange={(event) => setPrompt(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && void start()}
      />
      {problem ? (
        <span className="s-background-problem" role="alert">
          {problem}
        </span>
      ) : null}
      <div className="s-background-row">
        {available ? (
          <>
            <button
              type="button"
              className={`s-background-button${inBackground ? " is-selected" : ""}`}
              aria-pressed={inBackground}
              onClick={() => setInBackground(true)}
            >
              {S.runInBackground}
            </button>
            <button
              type="button"
              className={`s-background-button${inBackground ? "" : " is-selected"}`}
              aria-pressed={!inBackground}
              onClick={() => setInBackground(false)}
            >
              {S.runInTerminal}
            </button>
          </>
        ) : (
          <span className="s-background-off">{S.backgroundOffHelp}</span>
        )}
        <span className="s-background-gap" />
        <button type="button" className="s-new-start" disabled={!chosen || starting} onClick={() => void start()}>
          {S.startSession}
        </button>
      </div>
    </div>
  );
}
