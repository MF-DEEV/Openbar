// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The home page (NotchControlsView.swift:7-118; spec 05 §1.1): the levels
// in the band when they all fit, else in a row of their own; the player's
// card; and a rail of shortcut tiles that fills the rows the height allows,
// then scrolls sideways. The page reports its own height.

import { LayoutGrid, SlidersHorizontal } from "lucide-react";
import { type CSSProperties, type ReactNode, useEffect, useMemo } from "react";
import { create } from "zustand";
import { t } from "../i18n";
import { contentBudget } from "../island/geometry";
import type { ModuleId } from "../island/modules";
import { geometryOf, modulesOf, useIsland } from "../island/store";
import { EmptyState, IconButton } from "../island/views/Controls";
import type { PageProps } from "../island/views/Expanded";
import {
  controlsLayout,
  controlsOf,
  EMPTY_HEIGHT,
  isLevel,
  isShortcut,
  layoutHeight,
  LEVEL_ROW_HEIGHT,
  levelsFitBand,
  railColumns,
  railGap,
  ROW_SPACING,
  SECTIONS_BUTTON_ROOM,
  SHORTCUT_HEIGHT,
  SHORTCUT_LEAST_SPACING,
  SHORTCUT_SPACING,
  SHORTCUT_WIDTH,
} from "./layout";
import { BrightnessLevel, VolumeLevel } from "./Levels";
import { MusicCard } from "./MusicCard";
import { S } from "./strings";
import { ShortcutTile } from "./Tile";
import type { ControlItem } from "./types";

/** The levels the band holds, set by the page that measures them. */
/** The levels the band draws, and which copy of the page put them there. */
const useBand = create<{ levels: ControlItem[]; owner: number }>(() => ({ levels: [], owner: 0 }));
let nextOwner = 0;

function Level({ item }: { item: ControlItem }) {
  return item === "volume" ? <VolumeLevel /> : <BrightnessLevel />;
}

/** The island's way to its sections, where no Explore bubble stands in for it. */
function useSectionsButton(): boolean {
  return useIsland((s) => !s.settings.quickAccess.some((b) => b.action.kind === "explore"));
}

/** The items shown, in order (NotchSupport.controls). */
export function useControls(): ControlItem[] {
  const order = useIsland((s) => s.settings.controlOrder);
  const hidden = useIsland((s) => s.settings.hiddenControls);
  const modules = useIsland((s) => modulesOf(s).join(","));
  return controlsOf(order, hidden, modules.split(",") as ModuleId[]);
}

export function ControlsLeading() {
  const levels = useBand((s) => s.levels);
  const sections = useSectionsButton();
  return (
    <>
      {sections ? (
        <IconButton
          icon={LayoutGrid}
          label={t("island.sections")}
          onClick={() => useIsland.getState().toggleSections()}
        />
      ) : null}
      {levels.length > 0 ? (
        <div className="ct-band-levels">
          {levels.map((item) => (
            <Level key={item} item={item} />
          ))}
        </div>
      ) : null}
    </>
  );
}

export function ControlsPage({ width }: PageProps) {
  const items = useControls();
  const sections = useSectionsButton();
  const budget = useIsland((s) => contentBudget(geometryOf(s), true));
  const custom = useIsland((s) => s.settings.layout === "custom");
  const setPageContent = useIsland((s) => s.setPageContent);

  const levels = items.filter(isLevel);
  const shortcuts = items.filter(isShortcut);
  const hasCard = items.includes("music");
  const inBand = levelsFitBand(levels.length, width - (sections ? SECTIONS_BUTTON_ROOM : 0));
  const levelRow = levels.length > 0 && !inBand;
  // A preset adds the levels' own row to the island, as it adds the band;
  // a custom height holds them all (NotchGeometry, controls budget).
  const room = budget + (levelRow && !custom ? LEVEL_ROW_HEIGHT + ROW_SPACING : 0);
  const layout = controlsLayout({ hasCard, levelRow, shortcuts: shortcuts.length, width, height: room });
  const empty = items.length === 0;
  const height = Math.min(room, empty ? EMPTY_HEIGHT : layoutHeight(layout));
  const bandLevels = inBand ? levels.join(",") : "";

  // During a page transition the outgoing copy unmounts after the incoming
  // one filled the band, so a copy only clears what it put there itself.
  const owner = useMemo(() => ++nextOwner, []);
  useEffect(() => {
    useBand.setState({ levels: bandLevels ? (bandLevels.split(",") as ControlItem[]) : [], owner });
  }, [bandLevels, owner]);
  useEffect(() => setPageContent("controls", height), [height, setPageContent]);
  useEffect(
    () => () => {
      if (useBand.getState().owner === owner) useBand.setState({ levels: [] });
    },
    [owner],
  );

  if (empty) return <EmptyState icon={SlidersHorizontal} title={S.empty} />;
  return (
    <div className="ct-page">
      {layout.levelRow > 0 ? (
        <div className="ct-level-row" style={{ height: LEVEL_ROW_HEIGHT }}>
          {levels.map((item) => (
            <Level key={item} item={item} />
          ))}
        </div>
      ) : null}
      {layout.cardRow > 0 ? <MusicCard height={layout.cardRow} /> : null}
      {layout.shortcutRows > 0 ? (
        <Rail rows={layout.shortcutRows} rowGap={layout.shortcutRowSpacing} width={width}>
          {shortcuts.map((item) => (
            <ShortcutTile key={item} item={item} />
          ))}
        </Rail>
      ) : null}
    </div>
  );
}

/**
 * NotchRail: a rail whose columns fit lays its tiles out in reading order
 * and centres a short last row, closing its gaps down to none first; one
 * that doesn't fills its columns and scrolls sideways.
 */
function Rail(props: { rows: number; rowGap: number; width: number; children: ReactNode[] }) {
  const count = props.children.length;
  const columns = railColumns(count, props.rows);
  const gap = railGap(columns, SHORTCUT_WIDTH, SHORTCUT_SPACING, SHORTCUT_LEAST_SPACING, props.width);
  const style = {
    "--ct-rows": props.rows,
    "--ct-row-gap": `${props.rowGap}px`,
    "--ct-gap": `${gap ?? SHORTCUT_SPACING}px`,
    "--ct-tile-w": `${SHORTCUT_WIDTH}px`,
    "--ct-tile-h": `${SHORTCUT_HEIGHT}px`,
  } as CSSProperties;
  if (gap === null) {
    return (
      <div className="ct-rail is-scrolling" style={style}>
        {props.children}
      </div>
    );
  }
  return (
    <div className="ct-rail" style={{ ...style, width: columns * SHORTCUT_WIDTH + Math.max(0, columns - 1) * gap }}>
      {props.children}
    </div>
  );
}
