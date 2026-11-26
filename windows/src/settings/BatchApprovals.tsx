// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

// What the queue's Allow all let through (#281): every ask it answered, with
// its session, agent, tool and whole command, newest first and grouped by
// click. Asks allowed one by one aren't listed. The same as the Mac's
// Sessions settings.

import { useCallback, useEffect, useState } from "react";
import { dateTimeFormat, useLocale } from "../i18n";
import { ProviderMark } from "../sessions/glyphs";
import { PROVIDER_NAMES, type BatchApproval, type Provider } from "../sessions/types";
import { bridge, inTauri } from "../shared/bridge";
import { DEMO } from "./batchApprovalsDemo";
import { T } from "./strings";
import { Button } from "./ui";

/** The asks of one click, in the order they were kept. */
interface Batch {
  id: string;
  at: number;
  entries: BatchApproval[];
}

/** Consecutive entries of one click make one group; the list is newest first already. */
export function groupBatches(entries: BatchApproval[]): Batch[] {
  const out: Batch[] = [];
  for (const entry of entries) {
    const last = out[out.length - 1];
    if (last && last.id === entry.batch) {
      last.entries.push(entry);
    } else {
      out.push({ id: entry.batch, at: entry.at, entries: [entry] });
    }
  }
  return out;
}

function agentName(agent: string): string {
  return PROVIDER_NAMES[agent as Provider] ?? agent;
}

function when(at: number): string {
  try {
    return dateTimeFormat({ dateStyle: "medium", timeStyle: "short" }).format(new Date(at * 1000));
  } catch {
    return new Date(at * 1000).toLocaleString();
  }
}

export function BatchApprovals() {
  useLocale();
  const S = T.sessions;
  const [entries, setEntries] = useState<BatchApproval[] | null>(null);

  const refresh = useCallback(async () => {
    if (!inTauri) {
      setEntries((e) => e ?? DEMO);
      return;
    }
    try {
      setEntries((await bridge.sessions.batchApprovals()) ?? []);
    } catch {
      setEntries((e) => e ?? []);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    let off: (() => void) | undefined;
    let gone = false;
    void bridge.sessions.onBatchApprovals(() => void refresh()).then((f) => (gone ? f() : (off = f)));
    return () => {
      gone = true;
      window.removeEventListener("focus", onFocus);
      off?.();
    };
  }, [refresh]);

  const clear = async () => {
    if (inTauri) await bridge.sessions.clearBatchApprovals();
    setEntries([]);
  };

  const batches = groupBatches(entries ?? []);
  return (
    <section className="st-batch-log" aria-label={S.batchApprovals}>
      <div className="st-batch-log-head">
        <div className="st-card-text">
          <h3 className="st-group-title">{S.batchApprovals}</h3>
          <p className="st-card-caption">{S.batchApprovalsCaption}</p>
        </div>
        {batches.length > 0 ? <Button onClick={() => void clear()}>{S.batchApprovalsClear}</Button> : null}
      </div>
      {entries === null ? null : batches.length === 0 ? (
        <p className="st-batch-log-empty">{S.batchApprovalsEmpty}</p>
      ) : (
        <div className="st-batch-log-list" role="list" tabIndex={0} aria-label={S.batchApprovals}>
          {batches.map((batch) => (
            <div className="st-batch" role="listitem" key={`${batch.id}-${batch.at}`}>
              <div className="st-batch-when">
                <span>{when(batch.at)}</span>
                <span className="st-batch-count">{S.batchApprovalsCount(batch.entries.length)}</span>
              </div>
              {batch.entries.map((e, i) => (
                <div className="st-batch-entry" key={`${e.sessionId}-${i}`}>
                  <span className="st-batch-mark" aria-hidden>
                    <ProviderMark provider={(e.agent || "custom") as Provider} size={16} />
                  </span>
                  <div className="st-batch-text">
                    <div className="st-batch-who">
                      <span className="st-batch-agent">{agentName(e.agent)}</span>
                      {e.session ? <span>{e.session}</span> : null}
                      {e.project && e.project !== e.session ? <span>{e.project}</span> : null}
                    </div>
                    <div className="st-batch-tool">{e.tool}</div>
                    {e.detail ? <pre className="st-batch-detail">{e.detail}</pre> : null}
                    {e.paths?.length ? (
                      <pre className="st-batch-detail">{e.paths.filter((p) => p !== e.detail.trim()).join("\n")}</pre>
                    ) : null}
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
