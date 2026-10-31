// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { useEffect, useState } from "react";
import { bridge } from "../shared/bridge";
import { playerChoices, usePlayerChoices } from "../music/SourcePicker";
import type { Source } from "../music/types";
import { useSettings } from "./state";
import { Select, SubRow, Toggle } from "./ui";

export function MusicOptions() {
  const preferred = useSettings((s) => s.prefs.island.preferredPlayer);
  const includeOther = useSettings((s) => s.prefs.island.includeOtherPlayers);
  const set = useSettings((s) => s.island);
  const { players, unavailable } = usePlayerChoices(preferred);
  const [sources, setSources] = useState<Source[]>([]);
  useEffect(() => {
    let stopped = false;
    let version = 0;
    const off = bridge.music.onNowPlaying((now) => {
      version++;
      if (!stopped) setSources(now.sources);
    });
    void off
      .then(async () => {
        const at = version;
        const now = await bridge.music.now();
        if (!stopped && version === at && now) setSources(now.sources);
      })
      .catch(() => {});
    return () => {
      stopped = true;
      void off.then((stop) => stop());
    };
  }, []);
  return (
    <>
      <SubRow
        title="Preferred player"
        caption="The app to open when nothing is playing. Automatic uses the last music app, then an installed player."
      >
        <Select
          label="Preferred music player"
          value={preferred}
          onChange={(value) => set("preferredPlayer", value)}
          options={[
            { value: "", label: "Automatic" },
            ...playerChoices(players, sources, preferred).map((p) => ({ value: p.id, label: p.name })),
          ]}
        />
      </SubRow>
      <SubRow
        title="Include other players"
        caption="Automatic may follow browsers and other media apps. A manual source choice always takes priority."
      >
        <Toggle
          label="Include other players"
          checked={includeOther}
          onChange={(on) => set("includeOtherPlayers", on)}
        />
      </SubRow>
      {unavailable ? (
        <p className="st-note">Installed players could not be loaded. Active media sessions are still available.</p>
      ) : null}
    </>
  );
}
