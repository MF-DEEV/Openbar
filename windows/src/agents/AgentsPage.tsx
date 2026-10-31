// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../i18n";

// The AI page (NotchAgentsView.swift:6-155): the cards the person picked,
// paired across the island, as tall as they are up to the preset's budget,
// scrolling beyond it. The period the money cards cover sits in the top row.

import { ChevronDown, LayoutGrid, LoaderCircle, Sparkles } from "lucide-react";
import { useEffect, useMemo } from "react";

import { contentBudget } from "../island/geometry";
import { geometryOf, useIsland } from "../island/store";
import { EmptyState, IconButton } from "../island/views/Controls";
import type { PageProps } from "../island/views/Expanded";
import { bridge } from "../shared/bridge";
import { setPrefs } from "../shared/prefs";
import { MenuButton } from "../shared/ui/Menu";
import { ActivityCard, LimitsCard, LiveCard, ShareCard, SpendCard, TrendCard } from "./Cards";
import {
  contentHeight,
  EMPTY_HEIGHT,
  followsPeriod,
  fullyPriced,
  pageProviders,
  rowHeight,
  rows as pairRows,
  showsCodexAccounts,
  shownCards,
  SPACING,
  type Tile,
  tiles,
} from "./model";
import { ResetsCard } from "./ResetsCard";
import { useAgents } from "./store";
import { A, periodName } from "./strings";
import type { AgentPeriod, Provider } from "./types";
import { useClock } from "./useClock";

/** The agents switched on in Settings, by provider. */
export function useEnabledProviders(): Record<Provider, boolean> {
  const s = useIsland((st) => st.settings);
  return useMemo(
    () => ({ claude: s.agentsClaude, codex: s.agentsCodex, opencode: s.agentsOpenCode, copilot: s.agentsCopilot }),
    [s.agentsClaude, s.agentsCodex, s.agentsOpenCode, s.agentsCopilot],
  );
}

/** The cards the page shows now, before they pair into rows. */
function usePageTiles(): { providers: Provider[]; tiles: Tile[] } {
  const snapshot = useAgents((s) => s.snapshot);
  const enabled = useEnabledProviders();
  const order = useIsland((s) => s.settings.agentsCardOrder);
  const hidden = useIsland((s) => s.settings.agentsHiddenCards);
  return useMemo(() => {
    // Added Codex homes have cards in the Limits card's place, so none while it is hidden.
    const accounts = showsCodexAccounts(enabled.codex, hidden) ? snapshot.codexAccounts.map((a) => a.id) : [];
    const providers = pageProviders(snapshot, enabled, accounts.length > 0);
    return { providers, tiles: tiles(shownCards(order, hidden), providers, accounts) };
  }, [snapshot, enabled, order, hidden]);
}

/** How tall the page asks the island to be (NotchService.agentsContentHeight); null fills the budget. */
export function pageHeight(loaded: boolean, providers: number, rowsHeight: number, budget: number): number | null {
  if (!loaded) return null;
  if (providers === 0 || rowsHeight === 0) return Math.min(budget, EMPTY_HEIGHT);
  return Math.min(budget, rowsHeight);
}

export function AgentsPage({ width, height }: PageProps) {
  useLocale();
  const snapshot = useAgents((s) => s.snapshot);
  const { providers, tiles: list } = usePageTiles();
  const period = useIsland((s) => s.settings.agentsPeriod);
  const display = useIsland((s) => s.settings.agentsLimitDisplay);
  const setPageContent = useIsland((s) => s.setPageContent);
  const budget = useIsland((s) => contentBudget(geometryOf(s), true));
  const now = useClock(15_000);
  const rows = useMemo(() => pairRows(list, width), [list, width]);
  const wanted = pageHeight(snapshot.loaded, providers.length, contentHeight(rows), budget);

  useEffect(() => setPageContent("agents", wanted), [wanted, setPageContent]);
  // Claude's limits are checked every minute while the page shows, five otherwise.
  useEffect(() => {
    void bridge.agents.pageVisible(true);
    return () => void bridge.agents.pageVisible(false);
  }, []);

  if (!snapshot.loaded) {
    return (
      <div className="ag-state">
        <LoaderCircle className="ag-spin" size={16} aria-hidden />
        <span>{A.loading}</span>
      </div>
    );
  }
  // The Mac's empty view: the glyph above one 12 pt secondary line, 250 wide.
  if (providers.length === 0)
    return (
      <div className="ag-empty">
        <EmptyState icon={Sparkles} title={A.empty} />
      </div>
    );
  if (rows.length === 0)
    return (
      <div className="ag-empty">
        <EmptyState icon={LayoutGrid} title={A.noCards} />
      </div>
    );

  return (
    <div className="ag-page" style={{ height, overflowY: contentHeight(rows) > height + 0.5 ? "auto" : "hidden" }}>
      {rows.map((row) => (
        <div key={row[0]!.id} className="ag-row" style={{ height: rowHeight(row), gap: SPACING }}>
          {row.map((tile) => (
            <TileView key={tile.id} tile={tile} providers={providers} period={period} display={display} now={now} />
          ))}
        </div>
      ))}
    </div>
  );
}

function TileView(props: {
  tile: Tile;
  providers: Provider[];
  period: AgentPeriod;
  display: "remaining" | "used";
  now: number;
}) {
  useLocale();
  const snapshot = useAgents((s) => s.snapshot);
  const { tile, providers, period, now } = props;
  const usage = snapshot.periods[period];
  switch (tile.card) {
    case "limits":
      return (
        <LimitsCard
          provider={tile.provider!}
          snapshot={snapshot}
          now={now}
          display={props.display}
          account={tile.account ? snapshot.codexAccounts.find((a) => a.id === tile.account) : undefined}
        />
      );
    case "spend":
      return <SpendCard snapshot={snapshot} providers={providers} period={period} />;
    case "live":
      return <LiveCard snapshot={snapshot} providers={providers} now={now} />;
    case "trend":
      return <TrendCard snapshot={snapshot} providers={providers} period={period} />;
    case "models":
      return <ShareCard title={A.modelsCard} kind="models" shares={usage.models} byCost={fullyPriced(usage)} />;
    case "projects":
      return <ShareCard title={A.projectsCard} kind="projects" shares={usage.projects} byCost={fullyPriced(usage)} />;
    case "activity":
      return <ActivityCard snapshot={snapshot} />;
    case "resets":
      return <ResetsCard resets={snapshot.resets} now={now} namesDefaultAccount={tile.namesDefaultAccount} />;
  }
}

/**
 * The top row's left side: the sections button where no Explore circle
 * stands in for it, then the period menu while a card follows the period.
 */
export function AgentsLeading() {
  useLocale();
  const loaded = useAgents((s) => s.snapshot.loaded);
  const { tiles: list } = usePageTiles();
  const period = useIsland((s) => s.settings.agentsPeriod);
  const hasExplore = useIsland((s) => s.settings.quickAccess.some((b) => b.action.kind === "explore"));
  const showsPeriod = loaded && list.some((tile) => followsPeriod(tile.card));
  const choose = (next: AgentPeriod) => {
    void setPrefs({ island: { agentsPeriod: next } });
  };
  return (
    <>
      {hasExplore ? null : (
        <IconButton
          icon={LayoutGrid}
          label={t("island.sections")}
          onClick={() => useIsland.getState().toggleSections()}
        />
      )}
      {showsPeriod ? (
        <MenuButton
          className="ag-band-menu"
          label={`${A.period}: ${periodName(period)}`}
          alignRight={false}
          entries={() =>
            (["today", "week", "month"] as const).map((option) => ({
              label: periodName(option),
              checked: option === period,
              onSelect: () => choose(option),
            }))
          }
        >
          <span>{periodName(period)}</span>
          <ChevronDown size={10} strokeWidth={2.75} aria-hidden />
        </MenuButton>
      ) : null}
    </>
  );
}
