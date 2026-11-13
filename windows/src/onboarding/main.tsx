// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The first-run window (#168), the same flow as the Mac's onboarding: a
// request playing out in a drawn island, the agents found on this PC
// connecting by themselves, a sample request in the real island, and where
// Openbar lives afterwards. Few words; the steps show what they mean.

import "@fontsource-variable/inter";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowUp, Check, Globe, Sparkles, X } from "lucide-react";
import { AnimatePresence, motion, MotionConfig, useReducedMotion } from "motion/react";
import React, { useCallback, useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { useLocale } from "../i18n";
import { LANGUAGES } from "../i18n/languages";
import { initializeLocalization } from "../i18n/start";
import { inTauri } from "../shared/bridge";
import { loadPrefs, onPrefs, setPrefs } from "../shared/prefs";
import { ProviderMark } from "../sessions/glyphs";
import { S } from "../sessions/strings";
import { HOOK_AGENTS, PROVIDER_NAMES, type Provider } from "../sessions/types";
import trayIcon from "../../src-tauri/icons/64x64.png";
import {
  claudeTile,
  codexTile,
  hookAgentTile,
  shouldConnectClaude,
  STEPS,
  type AgentHooks,
  type ClaudeHooks,
  type CodexStatus,
  type TileState,
} from "./model";
import { O } from "./strings";
import "./onboarding.css";

const PROVIDERS: Provider[] = ["claude", "codex", "gemini", "copilot", "opencode", "cursor"];
const SPRING = { type: "spring", stiffness: 260, damping: 28 } as const;

// MARK: - The island, drawn

const ROUNDS: { provider: Provider; project: string; command: string }[] = [
  { provider: "claude", project: "checkout-api", command: "npm test" },
  { provider: "codex", project: "mobile-app", command: "dotnet build" },
  { provider: "gemini", project: "landing-page", command: "git push" },
  { provider: "opencode", project: "infra", command: "terraform plan" },
  { provider: "copilot", project: "docs-site", command: "bun run build" },
  { provider: "cursor", project: "design-system", command: "rd /s dist" },
];

type Phase = "resting" | "asking" | "pressing" | "allowed" | "closing";
const PHASES: [Phase, number][] = [
  ["resting", 900],
  ["asking", 1900],
  ["pressing", 350],
  ["allowed", 1600],
  ["closing", 1000],
];

/** A request playing out in a drawn island, on a loop, each round with another agent. */
function IslandDemo() {
  useLocale();
  const reduce = useReducedMotion();
  const [phase, setPhase] = useState<Phase>(reduce ? "asking" : "resting");
  const [round, setRound] = useState(0);
  useEffect(() => {
    if (reduce) return setPhase("asking");
    let cancelled = false;
    let timer = 0;
    let index = 0;
    const next = () => {
      if (cancelled) return;
      const [p, ms] = PHASES[index];
      setPhase(p);
      index = (index + 1) % PHASES.length;
      if (index === 0) timer = window.setTimeout(() => (setRound((r) => r + 1), next()), ms);
      else timer = window.setTimeout(next, ms);
    };
    next();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [reduce]);
  const current = ROUNDS[round % ROUNDS.length];
  const open = phase !== "resting" && phase !== "closing";
  const shown = phase === "allowed" || phase === "closing";
  return (
    <div
      className="ob-hero"
      aria-label={`${PROVIDER_NAMES[current.provider]}: ${S.waitingPermission}, ${current.command}`}
    >
      <div className="ob-glow" />
      <div className="ob-terminal" aria-hidden>
        <div className="ob-dots">
          <i />
          <i />
          <i />
        </div>
        <div className="ob-line ob-prompt">&gt; {current.provider === "claude" ? "claude" : current.provider}</div>
        <div className="ob-line">● Reading {current.project}\src …</div>
        <div className="ob-line" style={{ opacity: phase === "resting" ? 0 : 1 }}>
          ● Editing 3 files
        </div>
        <div className="ob-line" style={{ opacity: shown ? 1 : 0 }}>
          ● {current.command}
        </div>
      </div>
      <motion.div
        className="ob-island"
        initial={false}
        animate={{
          width: open ? 372 : 150,
          height: open ? 116 : 30,
          borderBottomLeftRadius: open ? 26 : 12,
          borderBottomRightRadius: open ? 26 : 12,
        }}
        transition={reduce ? { duration: 0 } : SPRING}
      >
        <AnimatePresence mode="popLayout" initial={false}>
          {open ? (
            <motion.div
              key="card"
              className="ob-card"
              initial={{ opacity: 0, scale: 0.92 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.94 }}
              transition={{ duration: 0.22 }}
            >
              <div className="ob-card-row">
                <ProviderMark provider={current.provider} size={17} />
                <div className="ob-card-name">
                  <b>{current.project}</b>
                  <span className={phase === "allowed" ? "" : "ob-waiting"}>
                    {phase === "allowed" ? PROVIDER_NAMES[current.provider] : S.waitingPermission}
                  </span>
                </div>
                <code>{current.command}</code>
              </div>
              {phase === "allowed" ? (
                <motion.div
                  className="ob-allowed"
                  initial={{ opacity: 0, scale: 0.96 }}
                  animate={{ opacity: 1, scale: 1 }}
                >
                  <Check size={14} strokeWidth={3} /> {S.allow}
                </motion.div>
              ) : (
                <div className="ob-pills">
                  <span className="ob-pill">
                    {S.deny} <small>Ctrl+N</small>
                  </span>
                  <motion.span
                    className="ob-pill ob-pill-main"
                    animate={
                      phase === "pressing"
                        ? { scale: 0.93, filter: "brightness(0.85)" }
                        : { scale: 1, filter: "brightness(1)" }
                    }
                    transition={{ type: "spring", stiffness: 600, damping: 22 }}
                  >
                    {S.allow} <small>Ctrl+Y</small>
                  </motion.span>
                </div>
              )}
            </motion.div>
          ) : (
            <motion.div
              key="strip"
              className="ob-strip"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            >
              <ProviderMark provider={current.provider} size={12} />
              {phase === "closing" ? (
                <motion.span
                  initial={{ scale: 0.3, opacity: 0 }}
                  animate={{ scale: 1, opacity: 1 }}
                  className="ob-strip-check"
                >
                  <Check size={11} strokeWidth={3.2} />
                </motion.span>
              ) : null}
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </div>
  );
}

// MARK: - Steps

function Welcome() {
  useLocale();
  const [language, setLanguage] = useState("auto");
  useEffect(() => {
    void loadPrefs().then((p) => setLanguage(p.island.language));
    let off: (() => void) | undefined;
    void onPrefs((p) => setLanguage(p.island.language)).then((u) => (off = u));
    return () => off?.();
  }, []);
  return (
    <div className="ob-step ob-welcome">
      <IslandDemo />
      <h1>{O.welcomeTitle}</h1>
      <p className="ob-lead">{O.welcomeBody}</p>
      <p className="ob-quiet">{O.welcomeToolkit}</p>
      <label className="ob-language">
        <Globe size={15} aria-hidden />
        <select
          value={language}
          aria-label="Language"
          onChange={(e) => {
            setLanguage(e.target.value);
            void setPrefs({ island: { language: e.target.value as never } });
          }}
        >
          <option value="auto">Auto</option>
          {LANGUAGES.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

/** Demo statuses for a plain browser, so the step can be looked at without the app. */
const BROWSER_HOOKS: AgentHooks[] = HOOK_AGENTS.map((agent) => ({
  agent,
  state: "missing",
  present: agent === "gemini" || agent === "opencode",
}));

function Agents() {
  useLocale();
  const reduce = useReducedMotion();
  const [claudePresent, setClaudePresent] = useState<boolean | null>(null);
  const [claude, setClaude] = useState<ClaudeHooks | null>(null);
  const [claudeFailed, setClaudeFailed] = useState(false);
  const [codex, setCodex] = useState<CodexStatus | null>(null);
  const [hooks, setHooks] = useState<AgentHooks[] | null>(null);
  const [failed, setFailed] = useState<Set<string>>(new Set());
  const asked = useRef(false);
  const ticks = useRef(0);

  const refresh = useCallback(async () => {
    ticks.current++;
    if (!inTauri) {
      // In a browser the found agents connect a moment later, as they would.
      const later = ticks.current > 1;
      setClaudePresent(true);
      setClaude({ installed: later, error: null });
      setCodex("notRunning");
      setHooks(BROWSER_HOOKS.map((h) => (h.present && later ? { ...h, state: "installed" } : h)));
      return;
    }
    const [present, status, codexStatus, agents] = await Promise.all([
      invoke<boolean>("onboarding_claude_present"),
      invoke<ClaudeHooks>("claude_hooks_status"),
      invoke<CodexStatus>("sessions_codex_status"),
      invoke<AgentHooks[]>("sessions_agent_hooks_status"),
    ]);
    setClaudePresent(present);
    setClaude(status);
    setCodex(codexStatus);
    setHooks(agents);
  }, []);

  useEffect(() => {
    void refresh();
    // The hooks and Codex's server answer in their own time.
    const timer = window.setInterval(() => ticks.current < 16 && void refresh(), 1500);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const installClaude = useCallback(async () => {
    setClaudeFailed(false);
    try {
      if (inTauri) await invoke("claude_hooks_install");
    } catch {
      setClaudeFailed(true);
    }
    await refresh();
  }, [refresh]);

  useEffect(() => {
    if (shouldConnectClaude(claudePresent, claude, asked.current)) {
      asked.current = true;
      void installClaude();
    }
  }, [claudePresent, claude, installClaude]);

  const retry = async (provider: Provider) => {
    if (provider === "claude") return installClaude();
    try {
      if (inTauri) await invoke("sessions_agent_hooks_install", { agent: provider, install: true });
      setFailed((f) => new Set([...f].filter((p) => p !== provider)));
    } catch {
      setFailed((f) => new Set([...f, provider]));
    }
    await refresh();
  };

  const state = (provider: Provider): TileState => {
    if (provider === "claude") return claudeTile(claudePresent, claude, claudeFailed);
    if (provider === "codex") return codexTile(codex);
    return hookAgentTile(
      hooks?.find((h) => h.agent === provider),
      failed.has(provider),
    );
  };

  return (
    <div className="ob-step ob-centered">
      <h2>{O.agentsTitle}</h2>
      <p className="ob-lead">{O.agentsBody}</p>
      <div className="ob-grid">
        {PROVIDERS.map((provider, i) => {
          const s = state(provider);
          return (
            <motion.div
              key={provider}
              className={`ob-tile ob-${s}`}
              initial={reduce ? false : { opacity: 0, y: 14 }}
              animate={{ opacity: s === "missing" ? 0.45 : 1, y: 0 }}
              transition={{ ...SPRING, delay: reduce ? 0 : i * 0.07 }}
            >
              <div className="ob-tile-mark">
                <ProviderMark provider={provider} size={22} />
                <AnimatePresence>
                  {s === "connected" || s === "ready" ? (
                    <motion.span
                      className="ob-badge"
                      initial={{ scale: 0.2, opacity: 0 }}
                      animate={{ scale: 1, opacity: 1 }}
                      transition={{ type: "spring", stiffness: 500, damping: 16 }}
                    >
                      <Check size={11} strokeWidth={3.4} />
                    </motion.span>
                  ) : null}
                </AnimatePresence>
              </div>
              <b>{PROVIDER_NAMES[provider]}</b>
              <TileStatus state={s} onRetry={() => void retry(provider)} />
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}

function TileStatus({ state, onRetry }: { state: TileState; onRetry: () => void }) {
  useLocale();
  switch (state) {
    case "checking":
      return <span className="ob-status ob-spinner" aria-hidden />;
    case "connecting":
      return (
        <span className="ob-status">
          <span className="ob-spinner" aria-hidden /> {O.agentConnecting}
        </span>
      );
    case "connected":
      return <span className="ob-status ob-ok">{O.agentConnected}</span>;
    case "ready":
      return <span className="ob-status ob-ok">{O.agentReady}</span>;
    case "missing":
      return <span className="ob-status">{O.agentNotInstalled}</span>;
    case "failed":
      return (
        <button type="button" className="ob-retry" onClick={onRetry} title={O.agentFailed}>
          {O.retry}
        </button>
      );
  }
}

function Keys() {
  useLocale();
  return (
    <div className="ob-keys">
      <div>
        <span>
          <kbd>Ctrl+Y</kbd> {O.keyAllow}
        </span>
        <span>
          <kbd>Ctrl+N</kbd> {O.keyDeny}
        </span>
        <span>
          <kbd>Ctrl+T</kbd> {O.keyAlways}
        </span>
      </div>
      <small>{O.anyApp}</small>
    </div>
  );
}

function TryIt() {
  useLocale();
  const reduce = useReducedMotion();
  const [shown, setShown] = useState(false);
  const [answer, setAnswer] = useState<boolean | null>(null);
  useEffect(() => {
    if (!inTauri) return;
    let off: (() => void) | undefined;
    void listen<boolean>("sessions://sample-answered", (e) => setAnswer(e.payload)).then((u) => (off = u));
    return () => {
      off?.();
      void invoke("sessions_dismiss_sample");
    };
  }, []);
  const show = useCallback(async () => {
    setAnswer(null);
    if (inTauri) await invoke("sessions_show_sample");
    else window.setTimeout(() => setAnswer(true), 1800);
    setShown(true);
  }, []);
  // The island shows the sample by itself once the step has settled: the
  // point is to see a session working, not to find a button first.
  useEffect(() => {
    const timer = window.setTimeout(() => void show(), 700);
    return () => window.clearTimeout(timer);
  }, [show]);
  const tone = answer === null ? "wait" : answer ? "ok" : "no";
  return (
    <div className="ob-step ob-centered">
      <motion.div
        className={`ob-orb ob-orb-${tone}`}
        animate={reduce || answer !== null ? { y: 0 } : { y: [0, -6, 0] }}
        transition={answer !== null ? SPRING : { duration: 1.8, repeat: Infinity, ease: "easeInOut" }}
      >
        <AnimatePresence mode="wait" initial={false}>
          <motion.span
            key={tone}
            initial={{ scale: 0.4, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.4, opacity: 0 }}
          >
            {answer === null ? (
              <ArrowUp size={36} strokeWidth={2.6} />
            ) : answer ? (
              <Check size={38} strokeWidth={3} />
            ) : (
              <X size={36} strokeWidth={3} />
            )}
          </motion.span>
        </AnimatePresence>
      </motion.div>
      <h2>{answer === null ? O.tryTitle : answer ? O.allowed : O.tryDenied}</h2>
      <p className="ob-lead">{answer === null ? O.tryBody : O.tryResult}</p>
      <button type="button" className="ob-primary ob-show" onClick={() => void show()}>
        <Sparkles size={15} aria-hidden /> {shown ? O.tryAgain : O.tryButton}
      </button>
      <Keys />
    </div>
  );
}

function Done() {
  useLocale();
  return (
    <div className="ob-step ob-done">
      <div className="ob-screen" aria-hidden>
        <div className="ob-screen-island">
          <ProviderMark provider="claude" size={11} />
          <span className="ob-ring" />
          <span className="ob-callout">
            <ArrowUp size={12} strokeWidth={2.8} />
            {O.doneIsland}
          </span>
        </div>
        <div className="ob-taskbar">
          <span className="ob-start" />
          <span className="ob-tray">
            <span className="ob-tray-icon">
              <img src={trayIcon} alt="" />
              <span className="ob-ring" />
              <span className="ob-callout ob-callout-up">
                {O.doneTray}
                <ArrowUp size={12} strokeWidth={2.8} style={{ transform: "rotate(180deg)" }} />
              </span>
            </span>
            <span className="ob-clock">9:41</span>
          </span>
        </div>
      </div>
      <h1>{O.doneTitle}</h1>
      <p className="ob-lead">{O.doneBody}</p>
      <Keys />
    </div>
  );
}

// MARK: - The window

function App() {
  useLocale();
  const reduce = useReducedMotion();
  const [index, setIndex] = useState(0);
  const [direction, setDirection] = useState(1);
  const step = STEPS[index];
  const go = (next: number) => {
    setDirection(next > index ? 1 : -1);
    setIndex(next);
  };
  const finish = () => {
    if (inTauri) void invoke("onboarding_finish");
    else go(0);
  };
  const last = index === STEPS.length - 1;
  return (
    <MotionConfig reducedMotion="user">
      <div className="ob-app">
        <main className="ob-main">
          <AnimatePresence mode="popLayout" initial={false} custom={direction}>
            <motion.div
              key={step}
              className="ob-page"
              initial={reduce ? { opacity: 0 } : { opacity: 0, x: 40 * direction }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0 }}
              transition={reduce ? { duration: 0.15 } : SPRING}
            >
              {step === "welcome" ? (
                <Welcome />
              ) : step === "agents" ? (
                <Agents />
              ) : step === "tryIt" ? (
                <TryIt />
              ) : (
                <Done />
              )}
            </motion.div>
          </AnimatePresence>
        </main>
        <footer className="ob-footer">
          {index === 0 ? (
            <button type="button" className="ob-secondary" onClick={finish}>
              {O.skip}
            </button>
          ) : (
            <button type="button" className="ob-secondary" onClick={() => go(index - 1)}>
              {O.back}
            </button>
          )}
          <div className="ob-progress" aria-hidden>
            {STEPS.map((s, i) => (
              <motion.span key={s} animate={{ width: i === index ? 18 : 7 }} className={i === index ? "on" : ""} />
            ))}
          </div>
          <button type="button" className="ob-primary" autoFocus onClick={() => (last ? finish() : go(index + 1))}>
            {last ? O.start : O.continue}
          </button>
        </footer>
      </div>
    </MotionConfig>
  );
}

void initializeLocalization().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
