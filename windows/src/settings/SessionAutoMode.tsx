// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

// Sessions in their agent's no-ask mode, per agent, and the opt-in to let
// every request of an agent through (#234). Turning Always proceed on asks
// first and says what it means. The same as the Mac's Sessions settings.

import { useState } from "react";
import { useLocale } from "../i18n";
import type { SessionPrefs } from "../shared/prefs";
import { PROVIDER_NAMES, type Provider } from "../sessions/types";
import { T } from "./strings";
import { Button, SubRow, Toggle } from "./ui";

type Key = keyof SessionPrefs;

const AUTO_RUN: [Provider, Key][] = [
  ["claude", "autoRunClaude"],
  ["codex", "autoRunCodex"],
];
const ALWAYS: [Provider, Key][] = [
  ["claude", "alwaysProceedClaude"],
  ["codex", "alwaysProceedCodex"],
  ["opencode", "alwaysProceedOpencode"],
  ["vibe", "alwaysProceedVibe"],
];

export function SessionAutoMode({
  prefs,
  set,
}: {
  prefs: SessionPrefs;
  set: <K extends keyof SessionPrefs>(key: K, value: SessionPrefs[K]) => void;
}) {
  useLocale();
  const S = T.sessions;
  // The agent whose Always proceed waits on the person's yes.
  const [confirming, setConfirming] = useState<Provider | null>(null);
  return (
    <section className="st-session-sounds" aria-label={S.autoMode}>
      <h3 className="st-group-title">{S.autoMode}</h3>
      <p className="st-card-caption">{S.autoModeCaption}</p>
      {AUTO_RUN.map(([provider, key]) => (
        <SubRow key={key} title={PROVIDER_NAMES[provider]}>
          <Toggle
            label={`${S.autoMode}: ${PROVIDER_NAMES[provider]}`}
            checked={prefs[key] as boolean}
            onChange={(value) => set(key, value as never)}
          />
        </SubRow>
      ))}
      <h3 className="st-group-title">{S.alwaysProceed}</h3>
      <p className="st-card-caption">{S.alwaysProceedCaption}</p>
      {ALWAYS.map(([provider, key]) => {
        const on = prefs[key] as boolean;
        const name = PROVIDER_NAMES[provider];
        return (
          <div key={key}>
            <SubRow title={name} caption={on ? <span className="st-warning">{S.alwaysProceedOn}</span> : undefined}>
              <Toggle
                label={`${S.alwaysProceed}: ${name}`}
                checked={on}
                onChange={(value) => {
                  if (value) setConfirming(provider);
                  else {
                    setConfirming(null);
                    set(key, false as never);
                  }
                }}
              />
            </SubRow>
            {confirming === provider ? (
              <div className="st-confirm" role="alertdialog" aria-label={S.alwaysProceedConfirmTitle(name)}>
                <div className="st-card-title">{S.alwaysProceedConfirmTitle(name)}</div>
                <p className="st-card-caption">{S.alwaysProceedConfirmMessage(name)}</p>
                <div className="st-confirm-buttons">
                  <Button onClick={() => setConfirming(null)}>{S.cancel}</Button>
                  <Button
                    accent
                    onClick={() => {
                      set(key, true as never);
                      setConfirming(null);
                    }}
                  >
                    {S.alwaysProceedConfirm}
                  </Button>
                </div>
              </div>
            ) : null}
          </div>
        );
      })}
    </section>
  );
}
