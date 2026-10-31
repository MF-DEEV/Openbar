// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t, useLocale } from "../i18n";

import { useEffect, useState } from "react";
import { bridge } from "../shared/bridge";
import { playerChoices, usePlayerChoices } from "../music/SourcePicker";
import type { Source } from "../music/types";
import { useSettings } from "./state";
import { Select, SubRow, Toggle } from "./ui";

export function MusicOptions() {
  useLocale();
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
        title={t("windows.settings.preferredPlayer")}
        caption={t("windows.settings.theAppToOpenWhenNothingIsPlaying")}
      >
        <Select
          label={t("windows.settings.preferredMusicPlayer")}
          value={preferred}
          onChange={(value) => set("preferredPlayer", value)}
          options={[
            { value: "", label: t("notch.automatic") },
            ...playerChoices(players, sources, preferred).map((p) => ({ value: p.id, label: p.name })),
          ]}
        />
      </SubRow>
      <SubRow
        title={t("windows.settings.includeOtherPlayers")}
        caption={t("windows.settings.automaticMayFollowBrowsersAndOtherMediaApps")}
      >
        <Toggle
          label={t("windows.settings.includeOtherPlayers")}
          checked={includeOther}
          onChange={(on) => set("includeOtherPlayers", on)}
        />
      </SubRow>
      {unavailable ? (
        <p className="st-note">{t("windows.settings.installedPlayersCouldNotBeLoadedActiveMedia")}</p>
      ) : null}
    </>
  );
}
