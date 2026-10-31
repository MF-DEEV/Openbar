// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Tools section (QuickLauncherView.swift, island branch; spec 05 §7.1):
// a rail of 76 × 72 tiles that fills the rows the page allows and then runs
// sideways; an edit mode with a five-column grid, red minus badges, the
// options card and the "Add back" tray; and Quick toggles hosted in place
// of the grid with the way back in the top row.

import { useLocale, t } from "../i18n";

import {
  Camera,
  Check,
  ChevronLeft,
  CirclePlus,
  Clipboard,
  LayoutGrid,
  Minus,
  type LucideIcon,
  Mic,
  MicOff,
  Pipette,
  Power,
  ScanText,
  Settings,
  SlidersHorizontal,
  StickyNote,
  X,
  Zap,
  Video,
} from "lucide-react";
import { type KeyboardEvent as ReactKeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { contentBudget } from "../island/geometry";
import { usePageLayer } from "../island/pageLayer";
import type { ColorPickerFormat } from "../island/settings";
import { geometryOf, useIsland } from "../island/store";
import type { PageProps } from "../island/views/Expanded";
import { IconButton } from "../island/views/Controls";
import { Switch } from "../shared/ui/controls";
import { setPrefs } from "../shared/prefs";
import { useReorder } from "../shared/ui/reorder";
import {
  EDIT_COLUMNS,
  EMPTY_HEIGHT,
  type RailLayout,
  TOOL_HEIGHT,
  TOOL_SPACING,
  TOOL_WIDTH,
  moveIndex,
  railHeight,
  railLayout,
} from "./model";
import { QuickToggles } from "./QuickToggles";
import { S } from "./strings";
import { useRecording } from "./recording";
import { reorderTools, resetTools, runTool, setToolHidden, toolLists, useTools } from "./store";
import { TOOLS_WITH_OPTIONS, type ToolId } from "./types";

// MARK: Item metadata (QuickLauncherView title/icon/isActive)

function title(id: ToolId, micMuted: boolean, recording: boolean): string {
  switch (id) {
    case "micMute":
      return micMuted ? S.micUnmute : S.micMute;
    case "keepAwake":
    case "toggles":
    case "screenOCR":
    case "colorPicker":
    case "clipboard":
    case "screenshot":
    case "scratchpad":
      return S[id];
    case "screenRecorder":
      return recording ? S.recordingStop : S.screenRecorder;
    default:
      return id;
  }
}

const ICONS: Partial<Record<ToolId, LucideIcon>> = {
  keepAwake: Zap,
  toggles: Power,
  screenOCR: ScanText,
  colorPicker: Pipette,
  clipboard: Clipboard,
  screenshot: Camera,
  screenRecorder: Video,
  scratchpad: StickyNote,
};

function useToolLook(id: ToolId) {
  const keepAwake = useTools((s) => s.keepAwake);
  const micMuted = useTools((s) => s.micMuted);
  const recording = useRecording((s) => s.startedAt !== null);
  const active =
    (id === "keepAwake" && keepAwake) || (id === "micMute" && micMuted) || (id === "screenRecorder" && recording);
  const Icon = id === "micMute" ? (micMuted ? MicOff : Mic) : (ICONS[id] ?? Settings);
  const tone = id === "micMute" && micMuted ? "is-muted" : active ? "is-active" : "";
  return { Icon, active, tone, title: title(id, micMuted, recording), filled: id === "keepAwake" && keepAwake };
}

// MARK: Page

function usePageHeight(count: number, width: number) {
  const editing = useTools((s) => s.editing);
  const utility = useTools((s) => s.utility);
  const budget = useIsland((s) => contentBudget(geometryOf(s), true));
  const setPageContent = useIsland((s) => s.setPageContent);
  const layout = railLayout(count, width, budget);
  const height = editing || utility ? null : Math.min(budget, count === 0 ? EMPTY_HEIGHT : railHeight(layout.rows));
  useEffect(() => setPageContent("tools", height), [height, setPageContent]);
  return layout;
}

export function ToolsPage({ width, height }: PageProps) {
  useLocale();
  const settings = useIsland((s) => s.settings);
  const editing = useTools((s) => s.editing);
  const utility = useTools((s) => s.utility);
  const optionsItem = useTools((s) => s.optionsItem);
  const lists = toolLists(settings);
  const layout = usePageHeight(lists.visible.length, width);

  useEffect(() => resetTools(), []);
  // Escape closes the options card, then edit mode, then the hosted utility.
  usePageLayer(utility !== null, () => useTools.setState({ utility: null }));
  usePageLayer(editing, () => useTools.setState({ editing: false, optionsItem: null }));
  usePageLayer(optionsItem !== null, () => useTools.setState({ optionsItem: null }));

  if (utility === "toggles") return <QuickToggles height={height} />;
  if (editing) {
    return (
      <div className="tl-page tl-edit" style={{ height }}>
        <p className="tl-hint">{S.editHint}</p>
        <EditGrid visible={lists.visible} />
        {optionsItem && TOOLS_WITH_OPTIONS.includes(optionsItem) ? <OptionsCard id={optionsItem} /> : null}
        {lists.hidden.length > 0 ? <HiddenTray hidden={lists.hidden} /> : null}
      </div>
    );
  }
  if (lists.visible.length === 0) {
    return (
      <div className="tl-page tl-empty" style={{ height }}>
        {S.empty}
      </div>
    );
  }
  return <Rail tools={lists.visible} layout={layout} width={width} />;
}

/** The trailing side: the customize toggle, or nothing while a utility is open. */
export function ToolsTrailing() {
  useLocale();
  const editing = useTools((s) => s.editing);
  const utility = useTools((s) => s.utility);
  if (utility) return null;
  return (
    <IconButton
      icon={editing ? Check : SlidersHorizontal}
      label={editing ? S.done : S.customize}
      selected={editing}
      onClick={() => useTools.setState((s) => ({ editing: !s.editing, optionsItem: null }))}
    />
  );
}

/** With a utility open, the way back names it (NotchBandBackButton); else the island's own sections button. */
export function ToolsLeading() {
  useLocale();
  const utility = useTools((s) => s.utility);
  const hasExploreBubble = useIsland((s) => s.settings.quickAccess.some((b) => b.action.kind === "explore"));
  if (!utility) {
    if (hasExploreBubble) return null;
    return (
      <IconButton
        icon={LayoutGrid}
        label={t("island.sections")}
        onClick={() => useIsland.getState().toggleSections()}
      />
    );
  }
  return (
    <button
      type="button"
      className="tl-back"
      aria-label={`${S.back}: ${S.toggles}`}
      title={S.back}
      onClick={() => useTools.setState({ utility: null })}
    >
      <ChevronLeft size={13} strokeWidth={2.5} />
      <span>{S.toggles}</span>
    </button>
  );
}

// MARK: Rail

function Rail({ tools, layout, width }: { tools: ToolId[]; layout: RailLayout; width: number }) {
  useLocale();
  const selectedIndex = useTools((s) => s.selectedIndex);
  const keyboardIndex = useTools((s) => s.keyboardIndex);
  const scroller = useRef<HTMLDivElement>(null);

  // Arrows walk the tiles as the rail draws them; Enter runs the chosen one.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const direction = ({ ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" } as const)[
        e.key as "ArrowLeft"
      ];
      const s = useTools.getState();
      if (s.editing || s.utility || document.querySelector(".tl-sheet")) return;
      if (direction) {
        e.preventDefault();
        const next = moveIndex(s.selectedIndex, tools.length, layout, direction);
        useTools.setState({ selectedIndex: next, keyboardIndex: next });
      } else if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
        const tool = tools[s.selectedIndex];
        if (tool) {
          e.preventDefault();
          runTool(tool);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tools, layout]);

  // A rail that scrolls brings the keyboard's column into view.
  useEffect(() => {
    if (layout.gap !== null || keyboardIndex === null) return;
    const column = scroller.current?.children[Math.floor(keyboardIndex / layout.rows)];
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    column?.scrollIntoView({ inline: "center", block: "nearest", behavior: reduce ? "auto" : "smooth" });
  }, [keyboardIndex, layout]);

  const cell = (tool: ToolId, index: number) => (
    <ToolCell
      key={tool}
      id={tool}
      selected={index === selectedIndex}
      titleInset={layout.titleInset}
      onHover={() => useTools.setState({ selectedIndex: index, keyboardIndex: null })}
    />
  );

  if (layout.gap !== null) {
    const { columns, gap } = layout;
    const cellWidth = (width - Math.max(0, columns - 1) * gap) / Math.max(1, columns);
    const rows: ToolId[][] = [];
    for (let i = 0; i < tools.length; i += columns) rows.push(tools.slice(i, i + columns));
    return (
      <div className="tl-page tl-rail" style={{ rowGap: TOOL_SPACING }}>
        {rows.map((row, r) => (
          <div key={r} className="tl-rail-row" style={{ columnGap: gap, gridAutoColumns: cellWidth }}>
            {row.map((tool, c) => cell(tool, r * columns + c))}
          </div>
        ))}
      </div>
    );
  }
  const columns: ToolId[][] = [];
  for (let i = 0; i < tools.length; i += layout.rows) columns.push(tools.slice(i, i + layout.rows));
  return (
    <div ref={scroller} className="tl-page tl-rail is-scrolling" style={{ columnGap: TOOL_SPACING }}>
      {columns.map((column, c) => (
        <div key={c} className="tl-rail-column" style={{ rowGap: TOOL_SPACING, width: TOOL_WIDTH }}>
          {column.map((tool, r) => cell(tool, c * layout.rows + r))}
        </div>
      ))}
    </div>
  );
}

function ToolCell(props: { id: ToolId; selected: boolean; titleInset: number; onHover(): void }) {
  useLocale();
  const look = useToolLook(props.id);
  return (
    <button
      type="button"
      className={`tl-cell${props.selected ? " is-selected" : ""}`}
      style={{ height: TOOL_HEIGHT }}
      aria-label={look.title}
      title={look.title}
      aria-pressed={props.id === "keepAwake" || props.id === "micMute" ? look.active : undefined}
      onPointerEnter={props.onHover}
      onClick={() => runTool(props.id)}
    >
      <ToolGlyph look={look} />
      <span className="tl-cell-title" style={{ paddingInline: props.titleInset }}>
        {look.title}
      </span>
    </button>
  );
}

function ToolGlyph({ look, children }: { look: ReturnType<typeof useToolLook>; children?: ReactNode }) {
  useLocale();
  const Icon = look.Icon;
  return (
    <span className={`tl-glyph ${look.tone}`}>
      <Icon size={16} strokeWidth={2.25} fill={look.filled ? "currentColor" : "none"} aria-hidden />
      {look.active ? <span className="tl-active-dot" aria-hidden /> : null}
      {children}
    </span>
  );
}

// MARK: Edit mode

function EditGrid({ visible }: { visible: ToolId[] }) {
  useLocale();
  const [draft, setDraft] = useState<ToolId[] | null>(null);
  const ids = draft ?? visible;
  const reorder = useReorder<ToolId>({
    ids,
    axis: "grid",
    onReorder(next) {
      setDraft(next);
      reorderTools(next);
    },
  });
  useEffect(() => {
    if (reorder.moving === null) setDraft(null);
  }, [reorder.moving]);
  return (
    <div className="tl-edit-grid" style={{ gridTemplateColumns: `repeat(${EDIT_COLUMNS}, 1fr)`, gap: TOOL_SPACING }}>
      {ids.map((id) => (
        <EditCell key={id} id={id} moving={reorder.moving === id} itemProps={reorder.itemProps(id)} />
      ))}
    </div>
  );
}

function EditCell(props: {
  id: ToolId;
  moving: boolean;
  itemProps: ReturnType<ReturnType<typeof useReorder>["itemProps"]>;
}) {
  useLocale();
  const look = useToolLook(props.id);
  const optionsItem = useTools((s) => s.optionsItem);
  const hasOptions = TOOLS_WITH_OPTIONS.includes(props.id);
  return (
    <div className={`tl-edit-cell${props.moving ? " is-moving" : ""}`} {...props.itemProps}>
      <button
        type="button"
        className="tl-cell"
        aria-label={hasOptions ? `${look.title}: ${S.options}` : look.title}
        title={look.title}
        aria-expanded={hasOptions ? optionsItem === props.id : undefined}
        onClick={() => {
          if (hasOptions) useTools.setState((s) => ({ optionsItem: s.optionsItem === props.id ? null : props.id }));
        }}
      >
        <ToolGlyph look={{ ...look, active: false }}>
          {hasOptions ? (
            <span className={`tl-gear${optionsItem === props.id ? " is-open" : ""}`} aria-hidden>
              <Settings size={9} strokeWidth={2.5} />
            </span>
          ) : null}
        </ToolGlyph>
        <span className="tl-cell-title">{look.title}</span>
      </button>
      <button
        type="button"
        className="tl-minus"
        aria-label={`${S.hideItem}: ${look.title}`}
        title={S.hideItem}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => setToolHidden(props.id, true)}
      >
        <Minus size={10} strokeWidth={3.5} aria-hidden />
      </button>
    </div>
  );
}

function HiddenTray({ hidden }: { hidden: ToolId[] }) {
  useLocale();
  const micMuted = useTools((s) => s.micMuted);
  return (
    <div className="tl-tray">
      <div className="tl-tray-title">{S.addSection}</div>
      <div className="tl-chips">
        {hidden.map((id) => (
          <button
            key={id}
            type="button"
            className="tl-chip"
            aria-label={S.showItem(title(id, micMuted, false))}
            onClick={() => setToolHidden(id, false)}
          >
            <CirclePlus size={11} strokeWidth={2.25} aria-hidden />
            {title(id, micMuted, false)}
          </button>
        ))}
      </div>
    </div>
  );
}

const FORMATS: { value: ColorPickerFormat; label: string }[] = [
  {
    value: "hex",
    get label() {
      return t("windows.tools.label");
    },
  },
  {
    value: "rgb",
    get label() {
      return t("windows.tools.label88b599");
    },
  },
  {
    value: "hsl",
    get label() {
      return t("windows.tools.labelfc770a");
    },
  },
];

/** The options card under the grid (QuickLauncherView.optionsCard): black, radius 12, 11 pt. */
function OptionsCard({ id }: { id: ToolId }) {
  useLocale();
  const look = useToolLook(id);
  const format = useIsland((s) => s.settings.colorPickerFormat);
  const bareHex = useIsland((s) => s.settings.colorPickerBareHex);
  const Icon = look.Icon;
  const pickFormat = (e: ReactKeyboardEvent, index: number) => {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    e.stopPropagation();
    const next = FORMATS[(index + step + FORMATS.length) % FORMATS.length]!;
    void setPrefs({ island: { colorPickerFormat: next.value } });
  };
  return (
    <div className="tl-options">
      <div className="tl-options-head">
        <span className="tl-options-title">
          <Icon size={11} strokeWidth={2.25} aria-hidden />
          {look.title}
        </span>
        <button
          type="button"
          className="tl-options-close"
          aria-label={S.close}
          title={S.close}
          onClick={() => useTools.setState({ optionsItem: null })}
        >
          <X size={10} strokeWidth={2.5} />
        </button>
      </div>
      <div className="tl-segmented" role="radiogroup" aria-label={S.colorFormat}>
        {FORMATS.map((f, index) => (
          <button
            key={f.value}
            type="button"
            role="radio"
            aria-checked={format === f.value}
            tabIndex={format === f.value ? 0 : -1}
            className={format === f.value ? "is-on" : ""}
            onClick={() => void setPrefs({ island: { colorPickerFormat: f.value } })}
            onKeyDown={(e) => pickFormat(e, index)}
          >
            {f.label}
          </button>
        ))}
      </div>
      {format === "hex" ? (
        <label className="tl-options-row">
          <span>{S.bareHex}</span>
          <Switch
            on={bareHex}
            label={S.bareHex}
            onChange={(on) => void setPrefs({ island: { colorPickerBareHex: on } })}
          />
        </label>
      ) : null}
    </div>
  );
}
