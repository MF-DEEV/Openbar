// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings > Sessions > Containers (issue #330; ContainersSettings.swift on
// the Mac): the switch, the running containers to pick from, how each picked
// one stands, and setting one up or taking Openbar out of it, asked first in
// place. Nothing runs in a container the person didn't pick.

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { t, tf, useLocale } from "../i18n";
import { inTauri } from "../shared/bridge";
import {
  type ContainerInfo,
  type ContainerTarget,
  containerTitle,
  targetAlias,
  targetId,
  targetOf,
} from "../sessions/containers";
import { useSettings } from "./state";
import { Button, SubRow, Toggle } from "./ui";

/** How a container's link stands (sessions/remote.rs `State`). */
type ContainerState =
  | { state: "connecting" }
  | { state: "connected"; hasClient: boolean; hooksInstalled: boolean; container?: string }
  | { state: "disconnected"; reason: string };

interface ContainerStatus {
  alias: string;
  state: ContainerState | null;
  busy: boolean;
  failure: string | null;
}

/** Why a container set up before has no hooks now: rebuilt, or taken out. */
export function hooksGone(state: ContainerState | null, target: ContainerTarget): "rebuilt" | "gone" | null {
  if (state?.state !== "connected" || state.hooksInstalled || !target.setUp) return null;
  return state.container && state.container !== target.setUp ? "rebuilt" : "gone";
}

export function containerStateText(state: ContainerState | null, target: ContainerTarget): string {
  if (!state || state.state === "connecting") return t("windows.settings.sessions.remoteConnecting");
  if (state.state === "disconnected") return tf("windows.settings.sessions.remoteDisconnected", state.reason);
  if (state.hooksInstalled) return t("windows.settings.sessions.remoteReady");
  const gone = hooksGone(state, target);
  if (gone === "rebuilt") return t("windows.settings.sessions.containersRebuilt");
  if (gone === "gone") return t("windows.settings.sessions.containersHooksGone");
  return t("windows.settings.sessions.remoteNotSetUp");
}

export function Containers() {
  useLocale();
  const prefs = useSettings((s) => s.prefs.sessions);
  const set = useSettings((s) => s.sessions);
  const [statuses, setStatuses] = useState<ContainerStatus[]>([]);
  const [running, setRunning] = useState<ContainerInfo[]>([]);
  const [listFailure, setListFailure] = useState<string | null>(null);
  const [looking, setLooking] = useState(false);
  const [confirming, setConfirming] = useState<{ alias: string; install: boolean } | null>(null);
  const enabled = prefs.containersEnabled;
  const targets = prefs.containers;
  const aliases = targets.map(targetAlias);
  const key = aliases.join("\n");
  const refresh = useCallback(async () => {
    if (!inTauri) {
      setStatuses(
        aliases.map((alias) => ({
          alias,
          state: { state: "connected", hasClient: true, hooksInstalled: false },
          busy: false,
          failure: null,
        })),
      );
      return;
    }
    setStatuses(await invoke<ContainerStatus[]>("sessions_remote_hosts_status", { hosts: aliases }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  const look = useCallback(async () => {
    if (!inTauri) return;
    setLooking(true);
    try {
      setRunning(await invoke<ContainerInfo[]>("sessions_containers_list"));
      setListFailure(null);
    } catch (reason) {
      setRunning([]);
      setListFailure(String(reason));
    } finally {
      setLooking(false);
    }
  }, []);
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
  useEffect(() => {
    if (enabled) void look();
  }, [enabled, look]);
  const change = async (target: ContainerTarget, install: boolean) => {
    setConfirming(null);
    try {
      if (inTauri) {
        const id = await invoke<string | null>("sessions_containers_change", { alias: targetAlias(target), install });
        // The container set up is kept, to tell a rebuilt one later.
        set(
          "containers",
          targets.map((t) =>
            targetId(t) === targetId(target) ? { ...t, setUp: install ? (id ?? undefined) : undefined } : t,
          ),
        );
      }
    } catch {
      // The failure is shown with the container's status.
    }
    void refresh();
  };
  const picked = new Set(targets.map(targetId));
  const unpicked = running.filter((c) => !picked.has(targetId(targetOf(c))));
  return (
    <>
      <SubRow
        title={t("windows.settings.sessions.containersTitle")}
        caption={t("windows.settings.sessions.containersCaption")}
      >
        <Toggle
          label={t("windows.settings.sessions.containersTitle")}
          checked={enabled}
          onChange={(on) => set("containersEnabled", on)}
        />
      </SubRow>
      {enabled
        ? targets.map((target) => {
            const alias = targetAlias(target);
            const title = containerTitle(alias);
            const status = statuses.find((s) => s.alias === alias);
            const state = status?.state ?? null;
            const connected = state?.state === "connected" ? state : null;
            const asking = confirming?.alias === alias ? confirming : null;
            const gone = hooksGone(state, target);
            return (
              <SubRow
                key={alias}
                title={target.folder ? `${title} · ${t("windows.settings.sessions.containersDevcontainer")}` : title}
                caption={
                  <>
                    {target.folder ? <div className="st-container-folder">{target.folder}</div> : null}
                    <span className={gone ? "st-warning" : undefined}>{containerStateText(state, target)}</span>
                    {status?.failure ? <div className="st-error">{status.failure}</div> : null}
                    {asking ? (
                      <div className="st-remote-confirm">
                        {tf(
                          asking.install
                            ? "windows.settings.sessions.containersConfirmSetUp"
                            : "windows.settings.sessions.containersConfirmRemove",
                          title,
                        )}
                      </div>
                    ) : null}
                  </>
                }
              >
                {asking ? (
                  <>
                    <Button accent onClick={() => void change(target, asking.install)}>
                      {asking.install
                        ? t("windows.settings.sessions.remoteSetUp")
                        : t("windows.settings.sessions.containersRemoveHooks")}
                    </Button>
                    <Button onClick={() => setConfirming(null)}>{t("windows.settings.sessions.remoteCancel")}</Button>
                  </>
                ) : connected ? (
                  <Button
                    busy={status?.busy}
                    onClick={() => setConfirming({ alias, install: !connected.hooksInstalled })}
                  >
                    {connected.hooksInstalled
                      ? t("windows.settings.sessions.containersRemoveHooks")
                      : gone
                        ? t("windows.settings.sessions.containersSetUpAgain")
                        : t("windows.settings.sessions.remoteSetUp")}
                  </Button>
                ) : null}
                {asking ? null : (
                  <Button
                    onClick={() =>
                      set(
                        "containers",
                        targets.filter((t) => targetId(t) !== targetId(target)),
                      )
                    }
                  >
                    {t("windows.settings.sessions.containersDelete")}
                  </Button>
                )}
              </SubRow>
            );
          })
        : null}
      {enabled ? (
        <SubRow
          title={t("windows.settings.sessions.containersRunning")}
          caption={
            listFailure ? (
              <div className="st-error">{listFailure}</div>
            ) : unpicked.length === 0 && !looking ? (
              running.length === 0 ? (
                t("windows.settings.sessions.containersNone")
              ) : (
                t("windows.settings.sessions.containersAllPicked")
              )
            ) : undefined
          }
        >
          <Button busy={looking} onClick={() => void look()}>
            {t("windows.settings.sessions.containersRefresh")}
          </Button>
        </SubRow>
      ) : null}
      {enabled
        ? unpicked.map((container) => (
            <SubRow
              key={container.id}
              title={containerTitle(targetAlias(targetOf(container)))}
              caption={
                container.folder
                  ? `${t("windows.settings.sessions.containersDevcontainer")} · ${container.name}`
                  : container.image
              }
            >
              <Button onClick={() => set("containers", [...targets, targetOf(container)])}>
                {t("windows.settings.sessions.containersAdd")}
              </Button>
            </SubRow>
          ))
        : null}
    </>
  );
}
