// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Only when the island page is opened in a plain browser with ?clawd: a
// sheet of Clawds in every motion and size, and a working wardrobe, for
// designing and for comparing with the Mac.

import { useMemo, useState } from "react";
import { Clawd } from "./Clawd";
import { costumeCode, localCostumeStore, resolvedCostume, seeded, subagentKey, type ClawdCostume } from "./costume";
import { CLAWD_SIZES, DONE_SUBAGENT_OPACITY, DOZING_OPACITY, motionForPhase, type SessionPhase } from "./sprite";
import { ProjectTag, Wardrobe } from "./Wardrobe";

const SESSIONS: { id: string; name: string; project: string; phase: SessionPhase; dozing?: boolean }[] = [
  { id: "99c35831-5f0b-4c3e-9d0a-0f3c2a7b8e11", name: "Quickstart docs", project: "developer-docs", phase: "finished" },
  { id: "0f9a6c1e-2b7d-4e55-8a31-c4d2e6f7a890", name: "Settings overview", project: "ios-client", phase: "generating" },
  { id: "s/a1", name: "Hero headline rewrite", project: "landing-page", phase: "runningTool" },
  { id: "session-3", name: "Npm test suite failures", project: "checkout-api", phase: "idle", dozing: true },
  { id: "session-4", name: "Migrate auth tokens", project: "infra", phase: "waitingPermission" },
  { id: "session-5", name: "Which ORM?", project: "backend", phase: "waitingInput" },
  { id: "session-6", name: "Nightly benchmarks", project: "perf", phase: "background" },
  { id: "session-7", name: "Read the changelog", project: "vorsaint", phase: "idle" },
  { id: "session-8", name: "Lint pass", project: "web", phase: "runningTool" },
  { id: "session-9", name: "Fix flaky e2e", project: "extension", phase: "generating" },
  { id: "session-10", name: "Bump deps", project: "tools", phase: "finished" },
  { id: "session-11", name: "Write release notes", project: "docs", phase: "waitingInput" },
];

const store = localCostumeStore();

export function ClawdPreview() {
  const [kept, setKept] = useState(() => store.all());
  const [dressing, setDressing] = useState(SESSIONS[0].id);
  // Back-dated a little so the sheet shows a mix of moments, not one.
  const since = useMemo(() => Date.now(), []);
  const costume = (id: string) => resolvedCostume(id, kept[id]);
  const live = new Set(SESSIONS.map((s) => s.id));
  const session = SESSIONS.find((s) => s.id === dressing)!;

  const change = (id: string, _next: ClawdCostume, code: string | null) => {
    store.set(id, code, live);
    setKept(store.all());
  };

  return (
    <div className="clawd-preview">
      <style>{previewCss}</style>
      <h1>Clawd</h1>

      <h2>Sessions (list row 32×30, click to dress)</h2>
      <div className="clawd-preview-list">
        {SESSIONS.map((s, i) => (
          <button
            type="button"
            key={s.id}
            className={`clawd-preview-row${s.id === dressing ? " is-current" : ""}`}
            style={{ opacity: s.dozing ? DOZING_OPACITY : 1 }}
            onClick={() => setDressing(s.id)}
          >
            <Clawd
              costume={costume(s.id)}
              motion={motionForPhase(s.phase, s.dozing)}
              seed={s.id}
              since={since - i * 370}
              {...CLAWD_SIZES.listRow}
            />
            <span className="clawd-preview-text">
              <span className="clawd-preview-name">{s.name}</span>
              <span className="clawd-preview-meta">
                <ProjectTag project={s.project} />
                <span>
                  {s.phase}
                  {s.dozing ? " · asleep" : ""}
                </span>
                <code>{costumeCode(costume(s.id))}</code>
              </span>
            </span>
          </button>
        ))}
      </div>

      <h2>Sizes</h2>
      <div className="clawd-preview-sizes">
        {Object.entries(CLAWD_SIZES).map(([name, size]) => (
          <figure key={name}>
            <div className="clawd-preview-box" style={{ width: size.width, height: size.height }}>
              <Clawd costume={seeded("session-4")} motion="alert" seed="session-4" {...size} />
            </div>
            <figcaption>
              {name} {size.width}×{size.height}
            </figcaption>
          </figure>
        ))}
        <figure>
          <div className="clawd-preview-box" style={{ width: 96, height: 84 }}>
            <Clawd costume={seeded("session-10")} motion="content" seed="session-10" width={96} height={84} />
          </div>
          <figcaption>96×84</figcaption>
        </figure>
        <figure>
          <div className="clawd-preview-box" style={{ width: 48, height: 42 }}>
            <Clawd costume={seeded("session-3")} motion="doze" seed="session-3" width={48} height={42} still />
          </div>
          <figcaption>still doze</figcaption>
        </figure>
      </div>

      <h2>Subagents (18×14; green / blue / done)</h2>
      <div className="clawd-preview-sizes">
        {[
          ["a1", "green", true],
          ["a2", "blue", true],
          ["a3", null, false],
        ].map(([agent, color, running]) => {
          const k = subagentKey(SESSIONS[1].id, agent as string);
          return (
            <figure key={k}>
              <Clawd
                costume={seeded(k, color as string | null)}
                motion={running ? "work" : "done"}
                seed={k}
                {...CLAWD_SIZES.subagentRow}
                style={{ opacity: running ? 1 : DONE_SUBAGENT_OPACITY }}
              />
              <figcaption>{k}</figcaption>
            </figure>
          );
        })}
      </div>

      <h2>Wardrobe</h2>
      <div className="clawd-preview-island">
        <Wardrobe
          sessionId={session.id}
          costume={costume(session.id)}
          onChange={(next, code) => change(session.id, next, code)}
          onBack={() => setDressing(SESSIONS[(SESSIONS.indexOf(session) + 1) % SESSIONS.length].id)}
          name={session.name}
          project={session.project}
          motion={motionForPhase(session.phase, session.dozing)}
          since={since}
          islandWidth={560}
        />
      </div>
    </div>
  );
}

const previewCss = `
.clawd-preview { position: fixed; inset: 0; overflow: auto; background: #111; padding: 24px; box-sizing: border-box;
  user-select: text; font-size: 12px; }
.clawd-preview h1 { font-size: 20px; margin: 0 0 8px; }
.clawd-preview h2 { font-size: 13px; font-weight: 600; color: rgba(255,255,255,.7); margin: 24px 0 10px; }
.clawd-preview-list { display: grid; grid-template-columns: repeat(auto-fill, minmax(420px, 1fr)); gap: 8px; }
.clawd-preview-row { display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-radius: 16px;
  background: rgba(255,255,255,.065); text-align: left; }
.clawd-preview-row.is-current { box-shadow: inset 0 0 0 1px rgba(255,255,255,.5); }
.clawd-preview-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
.clawd-preview-name { font-size: 14px; font-weight: 600; }
.clawd-preview-meta { display: flex; align-items: center; gap: 8px; color: rgba(255,255,255,.55); }
.clawd-preview-meta code { font-size: 10px; color: rgba(255,255,255,.4); }
.clawd-preview-sizes { display: flex; flex-wrap: wrap; align-items: flex-end; gap: 28px; }
.clawd-preview-sizes figure { margin: 0; display: flex; flex-direction: column; align-items: center; gap: 6px; }
.clawd-preview-sizes figcaption { font-size: 10px; color: rgba(255,255,255,.5); }
.clawd-preview-box { outline: 1px dashed rgba(255,255,255,.18); }
.clawd-preview-island { width: 560px; height: 300px; background: #000; border-radius: 32px; padding: 16px 28px 0;
  box-sizing: border-box; }
`;
