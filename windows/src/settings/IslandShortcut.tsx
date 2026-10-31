// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../i18n/index";

import { invoke } from "@tauri-apps/api/core";
import { Keyboard } from "lucide-react";
import { useEffect, useState } from "react";
import { bridge, inTauri } from "../shared/bridge";
import { ShortcutField } from "../shared/ui/ShortcutField";
import { formatAccelerator } from "../shared/shortcut";
import { useSettings } from "./state";
import { T } from "./strings";
import { Row, Toggle } from "./ui";

export function IslandShortcut() {
  useLocale();
  const { shortcut, shortcutEnabled } = useSettings((s) => s.prefs.island);
  const save = useSettings((s) => s.shortcut);
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [taken, setTaken] = useState(false);
  useEffect(() => {
    let live = true;
    if (!inTauri || !shortcutEnabled || recording) {
      setTaken(false);
      return;
    }
    void invoke<boolean>("shortcut_registered")
      .then((ok) => {
        if (live) setTaken(!ok);
      })
      .catch((e: unknown) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [shortcut, shortcutEnabled, recording, busy]);
  const change = async (value: { shortcut?: string; shortcutEnabled?: boolean }) => {
    setBusy(true);
    setError(null);
    try {
      await save(value);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Row
      icon={Keyboard}
      title={T.behavior.shortcut}
      caption={
        <span role="status" aria-live="polite">
          {error || taken ? (
            <span className="st-error">{error ?? T.behavior.shortcutTaken}</span>
          ) : recording ? (
            T.behavior.shortcutRecording
          ) : (
            T.behavior.shortcutCaption
          )}
        </span>
      }
    >
      <div className="st-island-shortcut">
        <span className="st-keys" aria-label={t("windows.settings.configuredShortcut")}>
          {formatAccelerator(shortcut)
            .split("+")
            .map((key, i) => (
              <kbd key={i}>{key}</kbd>
            ))}
        </span>
        <ShortcutField
          className="st-button st-shortcut"
          label={T.behavior.shortcut}
          actionLabel={T.behavior.shortcutChange}
          recordingLabel={T.behavior.shortcutPressKeys}
          value={shortcut}
          disabled={!shortcutEnabled || busy}
          onRecording={async (on) => {
            setRecording(on);
            if (on) setError(null);
            await bridge.audio.switcherRecording(on);
          }}
          onInvalid={() => setError(T.behavior.shortcutInvalid)}
          onError={(e) => setError(String(e))}
          onChange={(accelerator) => change({ shortcut: accelerator })}
        />
      </div>
      <Toggle
        label={T.behavior.shortcut}
        checked={shortcutEnabled}
        disabled={recording || busy}
        onChange={(on) => {
          void change({ shortcutEnabled: on }).catch((e: unknown) => setError(String(e)));
        }}
      />
    </Row>
  );
}
