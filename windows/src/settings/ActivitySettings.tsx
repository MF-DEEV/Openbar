// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Settings › Activity (NotchSettings.swift:540-639): the indicators the
// closed island shows. Only those whose sections exist on Windows yet.

import { Music, ArrowDownToLine, Mic, Sparkles } from "lucide-react";
import { useSettings } from "./state";
import { T } from "./strings";
import { Group, Page, Row, Toggle } from "./ui";

export function ActivitySettings() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const musicShown = !island.hiddenModules.includes("music") && island.sections.music !== false;
  const agentsShown = !island.hiddenModules.includes("agents") && island.sections.agents !== false;
  const downloadsShown = !island.hiddenModules.includes("downloads") && island.sections.downloads !== false;

  return (
    <Page title={T.nav.activity} lead={T.activity.indicatorsLead}>
      <Group title={T.activity.indicators}>
        <Row
          icon={Mic}
          title="Microphone Status"
          caption="Red indicators when your microphone or Discord headphones are muted."
        >
          <Toggle
            label="Microphone Status"
            checked={island.microphoneStatus}
            onChange={(on) => set("microphoneStatus", on)}
          />
        </Row>
        <Row
          icon={ArrowDownToLine}
          title={T.activity.downloadProgress}
          caption={T.activity.downloadProgressCaption}
          disabled={!downloadsShown}
        >
          <Toggle
            label={T.activity.downloadProgress}
            checked={island.downloadProgress && downloadsShown}
            disabled={!downloadsShown}
            onChange={(on) => set("downloadProgress", on)}
          />
        </Row>
        <Row icon={Sparkles} title={T.activity.agents} caption={T.activity.agentsCaption} disabled={!agentsShown}>
          <Toggle
            label={T.activity.agents}
            checked={island.agentsLiveActivity && agentsShown}
            disabled={!agentsShown}
            onChange={(on) => set("agentsLiveActivity", on)}
          />
        </Row>
        <Row icon={Music} title={T.activity.trackChange} caption={T.activity.trackChangeCaption} disabled={!musicShown}>
          <Toggle
            label={T.activity.trackChange}
            checked={island.trackChange && musicShown}
            disabled={!musicShown}
            onChange={(on) => set("trackChange", on)}
          />
        </Row>
        <p className="st-note">{T.activity.more}</p>
      </Group>
    </Page>
  );
}
