// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// AI Agents options, inside its card on the Content tab
// (NotchAgentsSettings.swift, CodexAccountsSettings.swift; spec 03 §3.5, §11).

import { useLocale, tf } from "../i18n";

import { openUrl } from "@tauri-apps/plugin-opener";
import { CircleAlert, CircleCheck, CircleMinus, Info } from "lucide-react";
import { useEffect, useState } from "react";
import { middleTruncate } from "../agents/Cards";
import { cost, duration, percent, PROVIDER_NAMES, PROVIDER_TINTS, relative, utcDay } from "../agents/format";
import { CLAUDE_FRESHNESS, orderedCards } from "../agents/model";
import { Mark } from "../agents/Parts";
import { emptySnapshot } from "../agents/store";
import { A, cardName, focusName, readoutName } from "../agents/strings";
import {
  type AgentReadout,
  type AgentSnapshot,
  type ClaudeFailure,
  type CodexProfile,
  type LimitDisplay,
  type LimitFocus,
  PROVIDERS,
  type Provider,
} from "../agents/types";
import "../agents/agents-settings.css";
import { bridge, inTauri } from "../shared/bridge";
import { CODEX_PROFILE_NAME_MAXIMUM, CODEX_PROFILES_MAXIMUM, codexHomeKey, isBlank } from "../shared/prefs";
import { useReorder } from "../shared/ui/reorder";
import type { IslandSettings } from "../island/settings";
import { useSettings } from "./state";
import { T } from "./strings";
import { Button, Segmented, Select, SubRow, Toggle } from "./ui";

const FINISH_MINIMUMS = [0, 30, 60, 120, 300];
const LIMIT_THRESHOLDS = [50, 75, 80, 90, 95];
const BUDGETS = [0, 5, 10, 25, 50, 100, 250];
const CLAUDE_DOWNLOAD = "https://claude.ai/download";

const PROVIDER_KEYS: Record<Provider, keyof IslandSettings> = {
  claude: "agentsClaude",
  codex: "agentsCodex",
  opencode: "agentsOpenCode",
  copilot: "agentsCopilot",
};

/** The usage service's snapshot, for what was found and how Claude's limits stand. */
function useSnapshot(): AgentSnapshot {
  const [snapshot, setSnapshot] = useState<AgentSnapshot>(emptySnapshot);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let live = true;
    void bridge.agents.onSnapshot(setSnapshot).then((u) => (live ? (unlisten = u) : u()));
    void bridge.agents.snapshot().then((s) => s && live && setSnapshot(s));
    return () => {
      live = false;
      unlisten?.();
    };
  }, []);
  return snapshot;
}

/** Re-evaluated every 30 s, as the status reads relative times. */
function useNow(): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

/** "30 sec", "1 min" and so on, for the finish minimum. */
const shortDuration = (seconds: number) => (seconds < 60 ? A.seconds(seconds) : A.minutes(seconds / 60));

export function AgentsOptions() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const snapshot = useSnapshot();
  const on = PROVIDERS.filter((p) => island[PROVIDER_KEYS[p]] as boolean);

  return (
    <>
      <p className="st-note ag-st-lead">{A.settingsDescription}</p>
      {PROVIDERS.map((provider) => {
        const enabled = island[PROVIDER_KEYS[provider]] as boolean;
        return (
          <div key={provider} className="st-subrow ag-st-provider">
            <span
              className="ag-st-mark"
              style={{ background: `color-mix(in srgb, ${PROVIDER_TINTS[provider]} 14%, transparent)` }}
              aria-hidden
            >
              <Mark provider={provider} size={13} />
            </span>
            <div className="st-card-text">
              <div className="st-card-title">{PROVIDER_NAMES[provider]}</div>
              <div className="st-card-caption">{snapshot.found[provider] ? A.found : A.notFound}</div>
            </div>
            <div className="st-card-control">
              {/* One agent stays on; turning the section off stops all. */}
              <Toggle
                label={PROVIDER_NAMES[provider]}
                checked={enabled}
                disabled={enabled && on.length <= 1}
                onChange={(v) => set(PROVIDER_KEYS[provider], v)}
              />
            </div>
          </div>
        );
      })}
      {island.agentsCodex ? <CodexAccounts snapshot={snapshot} /> : null}

      <CardsEditor />
      {island.agentsCodex && !island.agentsHiddenCards.includes("resets") ? (
        <p className="st-note ag-st-note">{A.resetsHelp}</p>
      ) : null}

      <SubRow title={A.limitsAs}>
        <Segmented<LimitDisplay>
          label={A.limitsAs}
          value={island.agentsLimitDisplay}
          options={[
            { value: "remaining", label: A.remaining },
            { value: "used", label: A.usedChoice },
          ]}
          onChange={(v) => set("agentsLimitDisplay", v)}
        />
      </SubRow>
      <SubRow title={A.limitFocus}>
        <Select<LimitFocus>
          label={A.limitFocus}
          value={island.agentsLimitFocus}
          options={(["mostUsed", "session", "weekly"] as const).map((v) => ({ value: v, label: focusName(v) }))}
          onChange={(v) => set("agentsLimitFocus", v)}
        />
      </SubRow>

      <h3 className="ag-st-heading">{A.liveTitle}</h3>
      <SubRow title={A.liveActivity}>
        <Toggle
          label={A.liveActivity}
          checked={island.agentsLiveActivity}
          onChange={(v) => set("agentsLiveActivity", v)}
        />
      </SubRow>
      {island.agentsLiveActivity ? (
        <SubRow title={A.readout}>
          <Select<AgentReadout>
            label={A.readout}
            value={island.agentsReadout}
            options={(["elapsed", "tokens", "cost", "limit"] as const).map((v) => ({
              value: v,
              label: readoutName(v),
            }))}
            onChange={(v) => set("agentsReadout", v)}
          />
        </SubRow>
      ) : null}

      <h3 className="ag-st-heading">{A.alerts}</h3>
      <SubRow title={A.finishAlert}>
        <Toggle
          label={A.finishAlert}
          checked={island.agentsFinishAlert}
          onChange={(v) => set("agentsFinishAlert", v)}
        />
      </SubRow>
      {island.agentsFinishAlert ? (
        <SubRow title={A.finishAfter}>
          <Select<string>
            label={A.finishAfter}
            value={String(island.agentsFinishMinimum)}
            options={choices(FINISH_MINIMUMS, island.agentsFinishMinimum).map((s) => ({
              value: String(s),
              label: s === 0 ? A.anyLength : FINISH_MINIMUMS.includes(s) ? shortDuration(s) : duration(s),
            }))}
            onChange={(v) => set("agentsFinishMinimum", Number(v))}
          />
        </SubRow>
      ) : null}
      <SubRow title={A.limitAlert}>
        <Toggle label={A.limitAlert} checked={island.agentsLimitAlert} onChange={(v) => set("agentsLimitAlert", v)} />
      </SubRow>
      {island.agentsLimitAlert ? (
        <SubRow title={A.limitAt}>
          <Select<string>
            label={A.limitAt}
            value={String(island.agentsLimitThreshold)}
            options={choices(LIMIT_THRESHOLDS, island.agentsLimitThreshold).map((v) => ({
              value: String(v),
              label: A.used(percent(v / 100)),
            }))}
            onChange={(v) => set("agentsLimitThreshold", Number(v))}
          />
        </SubRow>
      ) : null}
      <SubRow title={A.budget} caption={A.valueNote}>
        <Select<string>
          label={A.budget}
          value={String(island.agentsDailyBudget)}
          options={choices(BUDGETS, island.agentsDailyBudget).map((v) => ({
            value: String(v),
            label: v === 0 ? A.off : cost(v),
          }))}
          onChange={(v) => set("agentsDailyBudget", Number(v))}
        />
      </SubRow>
      <SubRow
        title={A.priceUpdates}
        caption={
          snapshot.priceDay ? `${A.priceUpdatesHint} ${A.pricesFrom(utcDay(snapshot.priceDay))}` : A.priceUpdatesHint
        }
      >
        <Toggle
          label={A.priceUpdates}
          checked={island.agentsPriceUpdates}
          onChange={(v) => set("agentsPriceUpdates", v)}
        />
      </SubRow>

      {island.agentsClaude ? <ClaudeLimitsStatus snapshot={snapshot} /> : null}
    </>
  );
}

/** The Codex homes that get limits cards of their own on the AI page (CodexAccountsSettings.swift). */
function CodexAccounts({ snapshot }: { snapshot: AgentSnapshot }) {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const patch = useSettings((s) => s.patch);
  const profiles = island.agentsCodexProfiles;
  const [invalidFolder, setInvalidFolder] = useState(false);
  const [busy, setBusy] = useState(false);
  const full = profiles.length >= CODEX_PROFILES_MAXIMUM;
  // Hiding the Limits card stops the checks, so Check limits greys out with it.
  const checks =
    island.enabled &&
    island.sections.agents !== false &&
    !island.hiddenModules.includes("agents") &&
    !island.agentsHiddenCards.includes("limits");
  const states = snapshot.codexAccounts;
  const allChecking = profiles.every((p) => states.find((s) => s.id === p.id)?.checking);
  const save = (list: CodexProfile[]) => {
    setInvalidFolder(false);
    void patch({ island: { agentsCodexProfiles: list } });
  };
  // What is saved now: a rename or removal made while Find or the folder picker ran stays.
  const latest = () => useSettings.getState().prefs.island.agentsCodexProfiles;
  const has = (directory: string) => latest().some((p) => codexHomeKey(p.directory) === codexHomeKey(directory));

  const find = async () => {
    setBusy(true);
    try {
      const found = (await bridge.agents.discoverCodexHomes(A.defaultName)) ?? [];
      const added = found.filter(
        (p, i) => !has(p.directory) && found.findIndex((q) => q.directory === p.directory) === i,
      );
      save([...latest(), ...added].slice(0, CODEX_PROFILES_MAXIMUM));
    } finally {
      setBusy(false);
    }
  };
  const add = async () => {
    setBusy(true);
    try {
      const chosen = await bridge.agents.chooseCodexHome(A.chooseCodexFolder, A.defaultName);
      if (!chosen) return;
      if ("invalid" in chosen) {
        setInvalidFolder(true);
        return;
      }
      if (has(chosen.directory)) return;
      if (latest().length >= CODEX_PROFILES_MAXIMUM) return;
      save([...latest(), { id: crypto.randomUUID(), name: chosen.name, directory: chosen.directory }]);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ag-st-accounts" role="group" aria-label={A.codexAccountsTitle}>
      <div className="st-card-title">{A.codexAccountsTitle}</div>
      <div className="st-card-caption">{A.codexAccountsExplanation}</div>
      {profiles.length > 0 ? (
        <ul className="ag-st-account-list">
          {profiles.map((profile) => (
            <AccountRow
              key={profile.id}
              profile={profile}
              onRename={(name) => save(profiles.map((p) => (p.id === profile.id ? { ...p, name } : p)))}
              onRemove={() => save(profiles.filter((p) => p.id !== profile.id))}
            />
          ))}
        </ul>
      ) : null}
      <div className="ag-st-account-actions">
        <Button onClick={() => void find()} disabled={full} busy={busy}>
          {A.findAccounts}
        </Button>
        <Button onClick={() => void add()} disabled={full} busy={busy}>
          {A.addFolder}
        </Button>
        {profiles.length > 0 ? (
          <Button onClick={() => void bridge.agents.refreshCodexAccount(null)} disabled={!checks || allChecking}>
            {A.accountCheck}
          </Button>
        ) : null}
      </div>
      {invalidFolder ? (
        <div className="ag-st-status is-warn ag-st-account-error" role="alert">
          <CircleAlert size={16} aria-hidden />
          <div className="ag-st-status-text">
            <span>{A.chooseCodexFolder}</span>
            <span className="is-secondary">{A.invalidCodexFolder}</span>
          </div>
        </div>
      ) : null}
      <div className="st-card-caption">{profiles.length === 0 ? A.accountsFallback : A.accountsResetsNote}</div>
    </div>
  );
}

/** A name kept as typed while it is blank, so clearing it to type another never saves an empty one. */
function AccountRow(props: { profile: CodexProfile; onRename(name: string): void; onRemove(): void }) {
  useLocale();
  const { profile } = props;
  const [draft, setDraft] = useState(profile.name);
  useEffect(() => setDraft(profile.name), [profile.name]);
  return (
    <li className="ag-st-account">
      <div className="ag-st-account-text">
        <input
          className="ag-st-input"
          type="text"
          value={draft}
          placeholder={A.accountName}
          aria-label={tf("windows.settings.codexValueValue", A.accountName, profile.name)}
          spellCheck={false}
          onChange={(e) => {
            const value = [...e.target.value].slice(0, CODEX_PROFILE_NAME_MAXIMUM).join("");
            setDraft(value);
            if (!isBlank(value)) props.onRename(value);
          }}
          onBlur={() => setDraft(profile.name)}
        />
        <span className="ag-st-account-path" title={profile.directory}>
          {middleTruncate(profile.directory, 72)}
        </span>
      </div>
      <button
        type="button"
        className="ag-st-account-remove"
        aria-label={`${A.removeAccount}: ${profile.name}`}
        title={A.removeAccount}
        onClick={props.onRemove}
      >
        <CircleMinus size={16} aria-hidden />
      </button>
    </li>
  );
}

/** The listed choices, plus a stored value they leave out (a hand-edited file). */
function choices(list: number[], value: number): number[] {
  return list.includes(value) ? list : [...list, value].sort((a, b) => a - b);
}

/** Cards to include, dragged into order; charts take the island's full width. */
function CardsEditor() {
  useLocale();
  const island = useSettings((s) => s.prefs.island);
  const patch = useSettings((s) => s.patch);
  const order = orderedCards(island.agentsCardOrder);
  const reorder = useReorder({
    ids: order,
    axis: "grid",
    onReorder: (ids) => patch({ island: { agentsCardOrder: ids } }),
  });
  const toggle = (card: (typeof order)[number]) => {
    const hidden = island.agentsHiddenCards.includes(card)
      ? island.agentsHiddenCards.filter((c) => c !== card)
      : [...island.agentsHiddenCards, card].sort();
    patch({ island: { agentsHiddenCards: hidden } });
  };
  const move = (card: (typeof order)[number], by: -1 | 1) => {
    const next = [...order];
    const from = next.indexOf(card);
    const to = from + by;
    if (to < 0 || to >= next.length) return;
    [next[from], next[to]] = [next[to]!, next[from]!];
    patch({ island: { agentsCardOrder: next } });
  };
  return (
    <div className="ag-st-cards">
      <div className="st-card-title">{A.cardsTitle}</div>
      <div className="ag-st-grid" role="list">
        {order.map((card) => {
          const included = !island.agentsHiddenCards.includes(card);
          return (
            <button
              key={card}
              type="button"
              role="listitem"
              className={`ag-st-tile${included ? " is-included" : ""}${reorder.moving === card ? " is-moving" : ""}`}
              aria-pressed={included}
              aria-label={`${cardName(card)}: ${included ? T.agents.cardShown : T.agents.cardHidden}`}
              title={cardName(card)}
              onClick={() => toggle(card)}
              onKeyDown={(e) => {
                if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowUp")) {
                  e.preventDefault();
                  move(card, -1);
                } else if (e.altKey && (e.key === "ArrowRight" || e.key === "ArrowDown")) {
                  e.preventDefault();
                  move(card, 1);
                }
              }}
              {...reorder.itemProps(card)}
            >
              <span className="ag-st-check" aria-hidden>
                {included ? <CircleCheck size={14} /> : null}
              </span>
              {cardName(card)}
            </button>
          );
        })}
      </div>
      <div className="st-card-caption">
        {A.cardsHint} {T.agents.cardsKeys}
      </div>
    </div>
  );
}

function claudeFailureText(failure: ClaudeFailure): string {
  switch (failure) {
    case "missing":
      return A.claudeCodeMissing;
    case "needsSignIn":
      return A.claudeCodeSignedOut;
    case "outdated":
      return A.claudeCodeOutdated;
    case "unreachable":
      return A.claudeCodeUnreachable;
  }
}

/** Where Claude's plan limits stand (NotchAgentsSettings.claudeLimitsStatus). */
function ClaudeLimitsStatus({ snapshot }: { snapshot: AgentSnapshot }) {
  useLocale();
  const now = useNow();
  const { accountAt, appAt, appInstalled, failure } = snapshot.claude;
  const fresh = (at: number | null) => at !== null && now - at < CLAUDE_FRESHNESS;
  const failureText = fresh(accountAt) || !failure ? null : claudeFailureText(failure);
  let tone: "ok" | "warn" | "info";
  let lines: string[];
  let action: { label: string; url: string } | null = null;
  if (accountAt !== null) {
    tone = fresh(accountAt) ? "ok" : "warn";
    lines = [A.claudeCodeUpdated(relative(Math.min(accountAt, now), now))];
  } else if (fresh(appAt)) {
    tone = "ok";
    lines = [A.claudeLimitsCurrent(relative(Math.min(appAt!, now), now))];
  } else if (!appInstalled && appAt === null) {
    tone = "info";
    lines = [A.claudeLimitsNoApp];
    action = { label: A.getClaude, url: CLAUDE_DOWNLOAD };
  } else {
    tone = "warn";
    lines = [...(appAt !== null ? [A.claudeLimitsStale(relative(Math.min(appAt, now), now))] : []), A.claudeLimitsTray];
  }
  const Icon = tone === "ok" ? CircleCheck : tone === "warn" ? CircleAlert : Info;
  return (
    <div className="ag-st-claude">
      <h3 className="ag-st-heading">{A.claudeLimitsTitle}</h3>
      <div className={`ag-st-status is-${tone}`}>
        <Icon size={16} aria-hidden />
        <div className="ag-st-status-text">
          {lines.map((line) => (
            <span key={line}>{line}</span>
          ))}
          {failureText ? <span className="is-secondary">{failureText}</span> : null}
        </div>
        {action ? (
          <button
            type="button"
            className="st-button"
            aria-label={action.label}
            title={action.label}
            onClick={() => void (inTauri ? openUrl(action.url) : window.open(action.url))}
          >
            {action.label}
          </button>
        ) : null}
      </div>
      <p className="st-note ag-st-note">{A.claudeLimitsPrivacy}</p>
    </div>
  );
}
