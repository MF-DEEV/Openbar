// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { ArrowDown, ArrowUp, LayoutGrid, Pin, Plus, RotateCcw, Settings, Trash2 } from "lucide-react";
import { capsulePath, makeGeometry, notchPath } from "../island/geometry";
import { MODULES, visibleModules } from "../island/modules";
import { freeQuickSide, placeButtons, QUICK_LAYOUT, QUICK_SIDES } from "../island/quickAccessLayout";
import { sectionEnabled, type QuickAction, type QuickButton, type QuickSide } from "../island/settings";
import { t } from "../i18n";
import { useSettings } from "./state";
import { T } from "./strings";
import { Button, Group, Row, Select, type Option } from "./ui";

const keyOf = (action: QuickAction) => (action.kind === "module" ? `module:${action.module}` : action.kind);
const SIDES: Option<QuickSide>[] = [
  { value: "left", label: T.content.sideLeft },
  { value: "right", label: T.content.sideRight },
  { value: "bottom", label: T.content.sideBottom },
];

export function QuickAccessSettings() {
  const island = useSettings((s) => s.prefs.island);
  const patch = useSettings((s) => s.patch);
  const buttons = island.quickAccess;
  const options = [
    ...MODULES.map((module) => ({
      value: `module:${module.id}`,
      label: t(`module.${module.id}`),
      icon: module.icon,
      action: { kind: "module", module: module.id } as QuickAction,
    })),
    { value: "explore", label: t("island.sections"), icon: LayoutGrid, action: { kind: "explore" } as QuickAction },
    { value: "settings", label: t("island.settings"), icon: Settings, action: { kind: "settings" } as QuickAction },
    { value: "pin", label: t("island.pin"), icon: Pin, action: { kind: "pin" } as QuickAction },
  ];
  const describe = (button: QuickButton) => options.find((option) => option.value === keyOf(button.action))!;
  const save = (next: QuickButton[], action?: QuickAction) =>
    patch({
      island: {
        quickAccess: next,
        ...(action?.kind === "module"
          ? {
              hiddenModules: island.hiddenModules.filter((id) => id !== action.module),
              sections: { ...island.sections, [action.module]: true },
            }
          : {}),
      },
    });
  const assign = (button: QuickButton, key: string) => {
    const action = options.find((option) => option.value === key)!.action;
    save(
      buttons.map((item) => (item.id === button.id ? { ...item, action, label: undefined } : item)),
      action,
    );
  };
  const add = (key: string) => {
    const side = freeQuickSide(buttons);
    if (!side) return;
    const action = options.find((option) => option.value === key)!.action;
    save([...buttons, { id: crypto.randomUUID(), side, action }], action);
  };
  const move = (button: QuickButton, direction: number) => {
    const siblings = buttons.filter((item) => item.side === button.side);
    const other = siblings[siblings.findIndex((item) => item.id === button.id) + direction];
    if (!other) return;
    const next = [...buttons];
    const from = next.findIndex((item) => item.id === button.id);
    const to = next.findIndex((item) => item.id === other.id);
    [next[from], next[to]] = [next[to]!, next[from]!];
    save(next);
  };
  const size = { width: 312, height: 170 };
  const geometry = makeGeometry({
    screen: { width: 480, height: 300 },
    devicePixelRatio: 1,
    silhouette: island.silhouette,
    layout: "spacious",
    roundness: island.roundness,
    shoulderWidth: island.shoulderWidth,
    shoulderHeight: island.shoulderHeight,
  });
  const shownModules = visibleModules(island.moduleOrder, island.hiddenModules, (id) => sectionEnabled(island, id));
  const placed = placeButtons(
    buttons.filter((button) => button.action.kind !== "module" || shownModules.includes(button.action.module)),
    size,
    geometry,
  );

  return (
    <Group title={T.content.quickAccess}>
      <div className="st-quick-preview-card">
        <div className="st-quick-preview" role="group" aria-label={T.content.quickPreview}>
          <svg viewBox="0 0 480 300" aria-hidden="true">
            <g transform="translate(84 20)">
              <path
                d={
                  island.silhouette === "notch"
                    ? notchPath(size.width, size.height, island)
                    : capsulePath(size.width, size.height, 0, island.roundness)
                }
                fill="#000"
              />
            </g>
            <rect x="176" y="68" width="128" height="7" rx="3.5" fill="#333" />
            <rect x="200" y="87" width="80" height="5" rx="2.5" fill="#222" />
            <path d="M234 113 L234 137 L252 125 Z" fill="#ddd" />
          </svg>
          {placed.map(({ button, cx, cy }) => {
            const info = describe(button);
            return (
              <Select
                key={button.id}
                className="st-quick-preview-circle is-editable"
                style={{ left: `${((84 + cx) / 480) * 100}%`, top: `${((20 + cy) / 300) * 100}%` }}
                label={`${T.content.quickPreview}: ${info.label}`}
                value={info.value}
                options={options}
                onChange={(key) => assign(button, key)}
              >
                <info.icon size={20} aria-hidden="true" />
              </Select>
            );
          })}
          <Select
            className="st-quick-preview-circle st-quick-add"
            style={{ left: "50%", top: "92.667%" }}
            label={T.content.addShortcut}
            value=""
            options={options}
            disabled={!freeQuickSide(buttons)}
            onChange={add}
          >
            <Plus size={24} aria-hidden="true" />
          </Select>
        </div>
        <p>{T.content.quickPreviewCaption}</p>
        <p className="st-quick-capacity">{T.content.quickCapacity}</p>
      </div>
      {QUICK_SIDES.flatMap((side) =>
        buttons
          .filter((button) => button.side === side)
          .map((button, index, siblings) => {
            const info = describe(button);
            const title = `${SIDES.find((option) => option.value === side)!.label} ${index + 1}`;
            return (
              <div className="st-card st-quick-row" key={button.id} data-quick-id={button.id}>
                <info.icon className="st-card-icon" size={18} aria-hidden="true" />
                <div className="st-card-text">
                  <div className="st-card-title">{title}</div>
                </div>
                <div className="st-quick-row-controls">
                  <Select
                    className="st-quick-action"
                    label={`${title}: ${T.content.shortcutAction}`}
                    value={info.value}
                    options={options}
                    onChange={(key) => assign(button, key)}
                  />
                  <Select
                    className="st-quick-side"
                    label={`${title}: ${T.content.shortcutSide}`}
                    value={side}
                    options={SIDES.filter(
                      (option) =>
                        option.value === side ||
                        buttons.filter((item) => item.side === option.value).length < QUICK_LAYOUT.perSide,
                    )}
                    onChange={(nextSide) =>
                      save(buttons.map((item) => (item.id === button.id ? { ...item, side: nextSide } : item)))
                    }
                  />
                  <div className="st-quick-row-actions">
                    <button
                      type="button"
                      className="st-icon-button"
                      aria-label={`${title}: ${T.content.moveUp}`}
                      title={T.content.moveUp}
                      disabled={index === 0}
                      onClick={() => move(button, -1)}
                    >
                      <ArrowUp size={15} />
                    </button>
                    <button
                      type="button"
                      className="st-icon-button"
                      aria-label={`${title}: ${T.content.moveDown}`}
                      title={T.content.moveDown}
                      disabled={index === siblings.length - 1}
                      onClick={() => move(button, 1)}
                    >
                      <ArrowDown size={15} />
                    </button>
                    <button
                      type="button"
                      className="st-icon-button st-quick-remove"
                      aria-label={`${title}: ${T.content.removeShortcut}`}
                      title={T.content.removeShortcut}
                      onClick={() => save(buttons.filter((item) => item.id !== button.id))}
                    >
                      <Trash2 size={15} />
                    </button>
                  </div>
                </div>
              </div>
            );
          }),
      )}
      <Row icon={RotateCcw} title={T.content.defaultShortcuts} caption={T.content.defaultShortcutsCaption}>
        <Button onClick={() => patch({ island: { quickAccess: null } })}>{T.content.restoreDefaults}</Button>
      </Row>
    </Group>
  );
}
