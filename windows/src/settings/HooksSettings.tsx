// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The Hooks page (#236; HooksSettings.swift): for every agent, each config
// file Openbar wrote into, whether its entries still work, and Reinstall and
// Remove, a copy of the file kept first. More config folders and tools with
// Claude Code style hooks can be added, lost hooks come back by themselves,
// and one button takes Openbar out of every file (hooks/page.rs).

import { t, tf, useLocale } from "../i18n";

import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, MinusCircle, RefreshCw, SquareTerminal, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ProviderMark } from "../sessions/glyphs";
import { PROVIDER_NAMES, type Provider } from "../sessions/types";
import { bridge, inTauri } from "../shared/bridge";
import { onLinux } from "../shared/platform";
import {
  FOLDERS_BY_HAND,
  HOOK_PAGE_ORDER,
  type HookRow,
  failureText,
  folderFailureText,
  shownPath,
  stateText,
  tone,
  toolFailureText,
} from "./hooks";
import { useSettings } from "./state";
import { Button, Group, Page, Row, Toggle } from "./ui";

/** Told by the app when the rows change (hooks/page_service.rs `CHANGED`). */
const CHANGED = "hooks://changed";

/** Rows for designing the page in a plain browser. */
const DEMO: HookRow[] = [
  {
    id: "claude:C:\\Users\\vors\\.claude\\settings.json",
    provider: "claude",
    name: "Default",
    folder: "C:\\Users\\vors\\.claude",
    file: "C:\\Users\\vors\\.claude\\settings.json",
    wholeFile: false,
    manual: false,
    present: true,
    derived: "installed",
    state: { kind: "installed" },
    canReinstall: true,
    canRemove: true,
  },
  {
    id: "agent:codex",
    provider: "codex",
    name: "Codex",
    folder: "C:\\Users\\vors\\.codex",
    file: null,
    wholeFile: false,
    manual: false,
    present: true,
    derived: "missing",
    state: { kind: "skipped", reason: "nothingNeeded" },
    canReinstall: false,
    canRemove: false,
  },
  {
    id: "agent:gemini",
    provider: "gemini",
    name: "Gemini CLI",
    folder: "C:\\Users\\vors\\.gemini",
    file: "C:\\Users\\vors\\.gemini\\settings.json",
    wholeFile: false,
    manual: false,
    present: true,
    derived: "changedByHand",
    state: { kind: "changedByHand" },
    canReinstall: true,
    canRemove: true,
  },
];

const samePath = (a: string, b: string) =>
  a.toLowerCase().replace(/[\\/]+$/, "") === b.toLowerCase().replace(/[\\/]+$/, "");

export function HooksSettings() {
  useLocale();
  const linux = onLinux();
  const [rows, setRows] = useState<HookRow[] | null>(null);
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [autoRepair, setAutoRepair] = useState(true);
  const [addFailure, setAddFailure] = useState<{ provider: Provider; text: string } | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [removedAll, setRemovedAll] = useState<number | null>(null);
  const saved = useSettings((s) => s.prefs.sessions.claudeAccounts);
  const set = useSettings((s) => s.sessions);

  const refresh = useCallback(async () => {
    if (!inTauri) {
      setRows((r) => r ?? DEMO);
      return;
    }
    try {
      setRows(await invoke<HookRow[]>("hooks_page_status"));
    } catch {
      setRows((r) => r ?? []);
    }
  }, []);

  useEffect(() => {
    void refresh();
    if (inTauri) void invoke<boolean>("hooks_page_auto_repair").then(setAutoRepair);
    // A file changed while the page shows reads again.
    const timer = window.setInterval(() => void refresh(), 5000);
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    let off: (() => void) | undefined;
    let gone = false;
    if (inTauri) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen(CHANGED, () => void refresh()).then((f) => (gone ? f() : (off = f))),
      );
    }
    return () => {
      gone = true;
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
      off?.();
    };
  }, [refresh]);

  const withBusy = async (ids: string[], work: () => Promise<unknown>) => {
    setBusy((b) => new Set([...b, ...ids]));
    try {
      await work();
    } catch {
      // The row shows why it failed, from the app.
    }
    await refresh();
    setBusy((b) => new Set([...b].filter((id) => !ids.includes(id))));
  };

  const change = (row: HookRow, command: "hooks_page_reinstall" | "hooks_page_remove" | "hooks_page_forget") => {
    setRemovedAll(null);
    void withBusy([row.id], () => (inTauri ? invoke(command, { id: row.id }) : Promise.resolve()));
  };

  const addFolder = async (provider: Provider) => {
    setAddFailure(null);
    const path = await bridge.sessions.pickFolder();
    if (!path) return;
    if (provider === "claude") {
      // Claude Code's accounts are added as on Settings › Sessions (#216).
      const ok = inTauri ? await invoke<boolean>("claude_accounts_can_add", { path }) : true;
      if (!ok) {
        const hidden = saved.find((s) => samePath(s.path, path) && s.hidden);
        if (hidden)
          set(
            "claudeAccounts",
            saved.map((s) => (s === hidden ? { ...s, hidden: undefined } : s)),
          );
        else setAddFailure({ provider, text: t("windows.claudeAccounts.notAdded") });
        return;
      }
      set("claudeAccounts", [...saved, { path, manual: true }]);
      window.setTimeout(() => void refresh(), 400);
      return;
    }
    try {
      if (inTauri) await invoke("hooks_page_add_folder", { agent: provider, path });
    } catch (e) {
      setAddFailure({ provider, text: folderFailureText(String(e)) });
    }
    await refresh();
  };

  const setRepair = (on: boolean) => {
    setAutoRepair(on);
    if (inTauri)
      void invoke("hooks_page_set_auto_repair", { on })
        .catch(() => setAutoRepair(!on))
        .then(() => refresh());
  };

  const removeAll = async () => {
    setConfirming(false);
    const ids = (rows ?? []).map((r) => r.id);
    let failed = 0;
    await withBusy(ids, async () => {
      failed = inTauri ? await invoke<number>("hooks_page_remove_all") : 0;
    });
    setRemovedAll(failed);
  };

  const list = rows ?? [];
  const tools = list.filter((r) => r.provider === "custom");

  return (
    <Page title={t("windows.hooks.title")} lead={t("windows.hooks.intro")}>
      <Group>
        <Row icon={RefreshCw} title={t("windows.hooks.repairTitle")} caption={t("windows.hooks.repairHint")}>
          <Toggle label={t("windows.hooks.repairTitle")} checked={autoRepair} onChange={setRepair} />
        </Row>
      </Group>
      {rows === null ? (
        <p className="st-note st-hooks-checking" role="status">
          {t("windows.hooks.checking")}
        </p>
      ) : (
        <>
          <Group>
            {HOOK_PAGE_ORDER.map((provider) => {
              const own = list.filter((r) => r.provider === provider);
              const folders = provider === "claude" || FOLDERS_BY_HAND.includes(provider);
              if (own.length === 0 && !folders) return null;
              return (
                <AgentCard
                  key={provider}
                  provider={provider}
                  rows={own}
                  busy={busy}
                  linux={linux}
                  failure={addFailure?.provider === provider ? addFailure.text : null}
                  onAddFolder={folders ? () => void addFolder(provider) : undefined}
                  onChange={change}
                />
              );
            })}
          </Group>
          <Group>
            <ToolsCard rows={tools} busy={busy} linux={linux} onChange={change} onAdded={() => void refresh()} />
          </Group>
          <Group>
            <div className="st-hooks-card">
              <div className={`st-card st-hooks-head${confirming || removedAll === 0 ? " has-rows" : ""}`}>
                <Trash2 className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden />
                <div className="st-card-text">
                  <div className="st-card-title">{t("windows.hooks.removeEverythingTitle")}</div>
                  <div className="st-card-caption">{t("windows.hooks.removeEverythingHint")}</div>
                </div>
                <div className="st-card-control">
                  <Button
                    onClick={() => setConfirming(true)}
                    disabled={confirming || busy.size > 0 || !list.some((r) => r.canRemove)}
                  >
                    {t("windows.hooks.removeEverything")}
                  </Button>
                </div>
              </div>
              {confirming ? (
                <div className="st-hooks-rows">
                  <div className="st-hook-confirm" role="alertdialog" aria-labelledby="st-hooks-confirm-title">
                    <div className="st-card-text">
                      <div className="st-card-title" id="st-hooks-confirm-title">
                        {t("windows.hooks.removeEverythingConfirm")}
                      </div>
                      <div className="st-card-caption">{t("windows.hooks.removeEverythingMessage")}</div>
                    </div>
                    <div className="st-card-control">
                      <Button onClick={() => setConfirming(false)}>{t("windows.hooks.cancel")}</Button>
                      <Button accent onClick={() => void removeAll()}>
                        {t("windows.hooks.remove")}
                      </Button>
                    </div>
                  </div>
                </div>
              ) : removedAll === 0 ? (
                <div className="st-hooks-rows">
                  <p className="st-note st-hooks-done" role="status">
                    {t("windows.hooks.removeEverythingDone")}
                  </p>
                </div>
              ) : null}
            </div>
          </Group>
        </>
      )}
    </Page>
  );
}

type Change = (row: HookRow, command: "hooks_page_reinstall" | "hooks_page_remove" | "hooks_page_forget") => void;

/** One agent: its mark and name, whether it is installed, and a row for each config file. */
function AgentCard(props: {
  provider: Provider;
  rows: HookRow[];
  busy: ReadonlySet<string>;
  linux: boolean;
  failure: string | null;
  onAddFolder?: () => void;
  onChange: Change;
}) {
  useLocale();
  const present = props.rows.some((r) => r.present);
  return (
    <div className="st-hooks-card">
      <div className={`st-card st-hooks-head${props.rows.length ? " has-rows" : ""}`}>
        <span className="st-agent-mark st-hooks-mark" aria-hidden>
          <ProviderMark provider={props.provider} size={15} />
        </span>
        <div className="st-card-text">
          <div className="st-card-title">{PROVIDER_NAMES[props.provider]}</div>
          <div className="st-card-caption">
            {present ? t("windows.hooks.installedApp") : t("windows.hooks.notInstalled")}
          </div>
          {props.failure ? <div className="st-error">{props.failure}</div> : null}
        </div>
        {props.onAddFolder ? (
          <div className="st-card-control">
            <span title={props.provider === "gemini" ? t("windows.hooks.geminiFolderHint") : undefined}>
              <Button onClick={props.onAddFolder}>{t("windows.hooks.addFolder")}</Button>
            </span>
          </div>
        ) : null}
      </div>
      {props.rows.length ? (
        <div className="st-hooks-rows">
          {props.rows.map((row) => (
            <HookRowView
              key={row.id}
              row={row}
              busy={props.busy.has(row.id)}
              linux={props.linux}
              onChange={props.onChange}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** One config file: its state and why, where it is, and what can be done. */
function HookRowView(props: { row: HookRow; busy: boolean; linux: boolean; onChange: Change }) {
  useLocale();
  const { row, busy, linux } = props;
  const own = row.provider === "codex" && row.file === null;
  const reveal = linux ? t("windows.hooks.showInFiles") : t("windows.hooks.showInExplorer");
  const approval = row.state.kind === "skipped" && row.state.reason === "needsApproval" && row.provider === "codex";
  return (
    <div className={`st-hook-row${busy ? " is-busy" : ""}`} aria-busy={busy || undefined}>
      <span className={`st-hook-dot is-${tone(row.state)}`} aria-hidden />
      <div className="st-card-text">
        <div className="st-hook-line">
          {row.provider === "claude" || row.manual ? <span className="st-hook-name">{row.name}</span> : null}
          <span className={`st-hook-state${row.state.kind === "installed" ? " is-on" : ""}`}>
            {stateText(row.state)}
          </span>
        </div>
        {own ? (
          <div className="st-card-caption">{t("windows.hooks.codexOwnFolder")}</div>
        ) : (
          <div className="st-hook-path" title={row.file ?? row.folder}>
            {shownPath(row.file ?? row.folder, linux)}
          </div>
        )}
        {approval ? (
          <div className="st-card-caption">{tf("windows.hooks.approvalNote", shownPath(row.folder, linux))}</div>
        ) : row.state.kind === "changedByHand" ? (
          <div className="st-card-caption">{t("windows.hooks.changedHint")}</div>
        ) : row.state.kind === "unreadable" ? (
          <div className="st-card-caption">{t("windows.hooks.unreadableHint")}</div>
        ) : null}
        {row.failure ? <div className="st-error">{failureText(row.failure)}</div> : null}
      </div>
      <div className="st-card-control">
        <button
          type="button"
          className="st-icon-button"
          title={reveal}
          aria-label={`${reveal}: ${row.file ?? row.folder}`}
          onClick={() => void (inTauri ? invoke("hooks_page_reveal", { id: row.id }) : undefined)}
        >
          <FolderOpen size={15} strokeWidth={1.75} aria-hidden />
        </button>
        {row.canReinstall ? (
          <Button busy={busy} onClick={() => props.onChange(row, "hooks_page_reinstall")}>
            {row.derived === "missing" ? t("windows.hooks.install") : t("windows.hooks.reinstall")}
          </Button>
        ) : null}
        {row.canRemove ? (
          <Button busy={busy} onClick={() => props.onChange(row, "hooks_page_remove")}>
            {t("windows.hooks.remove")}
          </Button>
        ) : null}
        {row.manual ? (
          <button
            type="button"
            className="st-icon-button"
            title={t("windows.hooks.forget")}
            aria-label={`${t("windows.hooks.forget")}: ${row.name}`}
            disabled={busy}
            onClick={() => props.onChange(row, "hooks_page_forget")}
          >
            <MinusCircle size={15} strokeWidth={1.75} aria-hidden />
          </button>
        ) : null}
      </div>
    </div>
  );
}

/** Tools with Claude Code style hooks: the ones added, and adding one by its
 * name and settings file. */
function ToolsCard(props: {
  rows: HookRow[];
  busy: ReadonlySet<string>;
  linux: boolean;
  onChange: Change;
  onAdded: () => void;
}) {
  useLocale();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [failure, setFailure] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const nameField = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (adding) nameField.current?.focus();
  }, [adding]);
  const close = () => {
    setAdding(false);
    setName("");
    setPath("");
    setFailure(null);
  };
  const choose = async () => {
    const picked = inTauri ? await invoke<string | null>("hooks_page_pick_file") : null;
    if (picked) setPath(picked);
  };
  const add = async () => {
    setSaving(true);
    setFailure(null);
    try {
      if (inTauri) await invoke("hooks_page_add_tool", { name, path });
      close();
      props.onAdded();
    } catch (e) {
      setFailure(toolFailureText(String(e)));
    } finally {
      setSaving(false);
    }
  };
  const open = adding || props.rows.length > 0;
  return (
    <div className="st-hooks-card">
      <div className={`st-card st-hooks-head${open ? " has-rows" : ""}`}>
        <SquareTerminal className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden />
        <div className="st-card-text">
          <div className="st-card-title">{t("windows.hooks.otherTools")}</div>
          <div className="st-card-caption">{t("windows.hooks.otherToolsHint")}</div>
          {!open ? <div className="st-card-caption st-hooks-empty">{t("windows.hooks.noTools")}</div> : null}
        </div>
        <div className="st-card-control">
          <Button onClick={() => setAdding(true)} disabled={adding}>
            {t("windows.hooks.addTool")}
          </Button>
        </div>
      </div>
      {open ? (
        <div className="st-hooks-rows">
          {adding ? (
            <form
              className="st-hook-form"
              onSubmit={(e) => {
                e.preventDefault();
                void add();
              }}
              onKeyDown={(e) => e.key === "Escape" && close()}
            >
              <input
                ref={nameField}
                className="ag-st-input st-hook-tool-name"
                value={name}
                maxLength={32}
                placeholder={t("windows.hooks.toolName")}
                aria-label={t("windows.hooks.toolName")}
                onChange={(e) => setName(e.target.value)}
              />
              <input
                className="ag-st-input st-hook-tool-path"
                value={path}
                spellCheck={false}
                placeholder={t("windows.hooks.chooseToolFile")}
                aria-label={t("windows.hooks.toolFile")}
                onChange={(e) => setPath(e.target.value)}
              />
              <Button onClick={() => void choose()}>{t("windows.hooks.chooseFile")}</Button>
              <div className="st-hook-form-actions">
                {failure ? (
                  <div className="st-error" role="alert">
                    {failure}
                  </div>
                ) : null}
                <Button onClick={close}>{t("windows.hooks.cancel")}</Button>
                <Button accent busy={saving} disabled={!name.trim() || !path.trim()} onClick={() => void add()}>
                  {t("windows.hooks.add")}
                </Button>
              </div>
            </form>
          ) : null}
          {props.rows.map((row) => (
            <HookRowView
              key={row.id}
              row={row}
              busy={props.busy.has(row.id)}
              linux={props.linux}
              onChange={props.onChange}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
