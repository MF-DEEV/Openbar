// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The switcher's shortcut row, shared by the island and Settings
// (ShortcutPreferenceRow): while a field records the backend lets go of the
// shortcut, and a key without a modifier or one the island already holds is
// refused with the Mac's message instead of being saved.

import { useState } from "react";
import { bridge } from "../shared/bridge";
import { reservedBy } from "../shared/shortcut";
import { S } from "./strings";

export function useSwitcherShortcut(save: (accelerator: string) => void) {
  const [recording, setRecording] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  return {
    recording,
    /** The invalid or conflict message, shown in place of the recording caption. */
    problem,
    field: {
      onRecording(on: boolean) {
        setRecording(on);
        if (on) setProblem(null);
        void bridge.audio.switcherRecording(on);
      },
      onInvalid: () => setProblem(S.shortcutInvalid),
      onChange(accelerator: string) {
        const who = reservedBy(accelerator);
        setProblem(who ? S.shortcutConflict(who) : null);
        if (!who) save(accelerator);
      },
    },
    reset(defaultShortcut: string) {
      setProblem(null);
      save(defaultShortcut);
    },
  };
}
