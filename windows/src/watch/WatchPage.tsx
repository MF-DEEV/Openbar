// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Watch page (NotchWatchView.swift; spec 04 §6.2): before anything is
// watched, what it does and the one step that starts it; then the live
// area with its reading, the rule, and since when it's watched. What is
// watched and its two actions sit in the island's top row.

import {
  ChevronDown,
  CircleCheck,
  CirclePause,
  CircleX,
  Eye,
  EyeOff,
  Flag,
  LayoutGrid,
  Lock,
  type LucideIcon,
  RefreshCw,
  Scan,
  TextSearch,
  X,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { t } from "../i18n";
import { useIsland } from "../island/store";
import { IconButton, PillButton } from "../island/views/Controls";
import type { PageProps } from "../island/views/Expanded";
import { type MenuEntry, useMenu } from "../shared/ui/Menu";
import { useWatch, watch } from "./store";
import { conditionTitle, outcomeTitle, S } from "./strings";
import { WATCH_CONDITIONS, type WatchCondition, type WatchSnapshot } from "./types";

/** The condition menu's glyphs (NotchWatchCondition.symbol). */
export const CONDITION_ICONS: Record<WatchCondition, LucideIcon> = {
  changes: RefreshCw,
  settles: CirclePause,
  contains: TextSearch,
  reaches: Flag,
};

/** Below this the setup's glyph sits beside its copy instead of above it (ViewThatFits). */
const STACKED_SETUP_HEIGHT = 176;

/** "14:05", in the person's own time format. */
export const clockTime = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/** A typed target as the field shows it again: without grouping (NotchWatchSupport.formatted). */
export const formatTarget = (value: number | null) =>
  value === null
    ? ""
    : new Intl.NumberFormat(undefined, { maximumFractionDigits: 2, useGrouping: false }).format(value);

/** "App · window title", or the app alone when the title says the same. */
export function watchedTitle(s: WatchSnapshot): string {
  const target = s.target;
  if (!target) return S.title;
  const app = target.appName || S.title;
  return target.windowTitle && target.windowTitle !== app ? `${app} · ${target.windowTitle}` : app;
}

export function WatchPage({ height }: PageProps) {
  const s = useWatch((state) => state.snapshot);
  useEffect(() => {
    watch.pageVisible(true);
    return () => watch.pageVisible(false);
  }, []);
  if (!s.target) return <Setup accessDenied={s.accessDenied} stacked={height >= STACKED_SETUP_HEIGHT} />;
  return (
    <div className="wt-page">
      <AreaCard s={s} />
      <RuleRow s={s} />
      <Footer s={s} />
    </div>
  );
}

/** The app's icon and name, then its window's title (the band's left). */
export function WatchLeading() {
  const s = useWatch((state) => state.snapshot);
  const hasExploreBubble = useIsland((state) => state.settings.quickAccess.some((b) => b.action.kind === "explore"));
  const target = s.target;
  if (!target) {
    // Setup leaves the band as every page has it.
    if (hasExploreBubble) return null;
    return (
      <IconButton
        icon={LayoutGrid}
        label={t("island.sections")}
        onClick={() => useIsland.getState().toggleSections()}
      />
    );
  }
  const app = target.appName || S.title;
  const title = target.windowTitle && target.windowTitle !== app ? target.windowTitle : null;
  return (
    <div className="wt-watched" title={watchedTitle(s)}>
      {target.icon ? (
        <img src={target.icon} alt="" width={16} height={16} />
      ) : (
        <Eye size={14} color="#bf5af2" aria-hidden />
      )}
      <span className="wt-watched-name">
        <strong>{app}</strong>
        {title ? <span> · {title}</span> : null}
      </span>
    </div>
  );
}

/** Choose again and Stop (the band's right). */
export function WatchTrailing() {
  const watching = useWatch((state) => state.snapshot.target !== null);
  if (!watching) return null;
  return (
    <>
      <IconButton icon={Scan} label={S.chooseAgain} onClick={() => void watch.choose()} />
      <IconButton icon={X} label={S.stop} onClick={watch.stop} />
    </>
  );
}

function Setup({ accessDenied, stacked }: { accessDenied: boolean; stacked: boolean }) {
  return (
    <div className={`wt-setup${stacked ? " is-stacked" : ""}`}>
      <div className="n-empty-glyph" aria-hidden>
        <Eye size={26} strokeWidth={1.5} />
      </div>
      <div className="wt-setup-copy">
        <p>{accessDenied ? S.permissionHint : S.setupHint}</p>
        <PillButton prominent onClick={() => void watch.choose()}>
          {S.choose}
        </PillButton>
      </div>
    </div>
  );
}

/** The live area with its reading below it, and what happened once the watch ended. */
function AreaCard({ s }: { s: WatchSnapshot }) {
  const hidden = s.phase === "hidden";
  const full = s.text.replace(/\n/g, " ");
  return (
    <div className="wt-card">
      <div className="wt-preview">
        {s.preview ? <FittedImage src={s.preview} dimmed={hidden} /> : <span className="wt-spinner" />}
        {hidden ? (
          <div className="wt-hidden-label" role="status">
            {s.accessDenied ? <Lock size={12} aria-hidden /> : <EyeOff size={12} aria-hidden />}
            <span>{s.accessDenied ? S.permissionHint : S.hidden}</span>
          </div>
        ) : null}
      </div>
      <div className="wt-reading">
        {s.phase === "finished" && s.outcome ? (
          <span className={`wt-outcome${s.outcome.kind === "closed" ? " is-closed" : ""}`}>
            {s.outcome.kind === "closed" ? <CircleX size={14} aria-hidden /> : <CircleCheck size={14} aria-hidden />}
            <span>{outcomeTitle(s.outcome)}</span>
          </span>
        ) : s.lines.length === 0 ? (
          <span className="wt-headline is-empty">{S.noText}</span>
        ) : (
          <HeadlineMenu s={s} />
        )}
      </div>
      {full && full !== s.headline ? <p className="wt-full">{full}</p> : null}
    </div>
  );
}

/** The picture fit inside its box, as large as fits, keeping its corners round. */
function FittedImage({ src, dimmed }: { src: string; dimmed: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [room, setRoom] = useState<{ w: number; h: number } | null>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => setRoom({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const scale = natural && room ? Math.min(room.w / natural.w, room.h / natural.h) : 0;
  return (
    <div ref={box} className="wt-preview-box">
      <img
        src={src}
        alt=""
        className={dimmed ? "is-dimmed" : undefined}
        style={scale > 0 ? { width: natural!.w * scale, height: natural!.h * scale } : { visibility: "hidden" }}
        onLoad={(e) => setNatural({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
      />
    </div>
  );
}

/** What the island shows, picked automatically or line by line when the automatic pick lands on the wrong words. */
function HeadlineMenu({ s }: { s: WatchSnapshot }) {
  const menu = useMenu();
  const entries: MenuEntry[] = [
    { kind: "header", label: S.inIsland },
    { label: S.automatic, checked: s.headlineLine === null, onSelect: () => watch.pinLine(null) },
    ...s.lines.map((line, index) => ({
      label: line,
      checked: s.headlineLine === index,
      onSelect: () => watch.pinLine(index),
    })),
  ];
  return (
    <>
      <button
        type="button"
        className="wt-headline"
        aria-label={`${S.inIsland}: ${s.headline}`}
        title={S.inIsland}
        aria-haspopup="menu"
        aria-expanded={menu.isOpen}
        onClick={(e) => menu.openBelow(e.currentTarget, entries, false)}
      >
        <span>{s.headline || S.noText}</span>
        <ChevronDown size={10} strokeWidth={2.5} aria-hidden />
      </button>
      {menu.menu}
    </>
  );
}

/**
 * When to speak up. A word or a number applies once Enter is pressed or the
 * field is left, so typing never ends the watch halfway. After a watch ends
 * the rule can still change, for the next Watch again.
 */
function RuleRow({ s }: { s: WatchSnapshot }) {
  const menu = useMenu();
  const [draftText, setDraftText] = useState(s.matchText);
  const [draftNumber, setDraftNumber] = useState(() => formatTarget(s.matchNumber));
  const number = s.condition === "reaches";
  const Icon = CONDITION_ICONS[s.condition];
  const apply = () => (number ? watch.setNumber(draftNumber) : watch.setText(draftText));
  const entries: MenuEntry[] = WATCH_CONDITIONS.map((condition) => ({
    label: conditionTitle(condition),
    icon: CONDITION_ICONS[condition],
    checked: condition === s.condition,
    onSelect: () => watch.setCondition(condition),
  }));
  return (
    <div className="wt-rule">
      <div className="wt-rule-row">
        <span className="wt-caption">{S.tellMe}</span>
        <button
          type="button"
          className="wt-condition"
          aria-haspopup="menu"
          aria-expanded={menu.isOpen}
          aria-label={`${S.tellMe}: ${conditionTitle(s.condition)}`}
          title={conditionTitle(s.condition)}
          onClick={(e) => menu.openBelow(e.currentTarget, entries, false)}
        >
          <Icon size={12} strokeWidth={2.25} aria-hidden />
          <span>{conditionTitle(s.condition)}</span>
          <ChevronDown size={9} strokeWidth={2.5} aria-hidden />
        </button>
        {s.condition === "contains" || number ? (
          <input
            className="wt-field"
            type="text"
            inputMode={number ? "decimal" : "text"}
            placeholder={number ? S.numberPlaceholder : S.textPlaceholder}
            aria-label={conditionTitle(s.condition)}
            value={number ? draftNumber : draftText}
            onChange={(e) => (number ? setDraftNumber(e.target.value) : setDraftText(e.target.value))}
            onKeyDown={(e) => {
              if (e.key === "Enter") apply();
            }}
            onBlur={apply}
            spellCheck={false}
          />
        ) : null}
      </div>
      {s.condition === "settles" ? <p className="wt-hint">{S.settlesHint}</p> : null}
      {menu.menu}
    </div>
  );
}

function Footer({ s }: { s: WatchSnapshot }) {
  if (s.phase === "finished") {
    return (
      <div className="wt-footer">
        {s.canWatchAgain ? (
          <PillButton prominent onClick={watch.again}>
            {S.watchAgain}
          </PillButton>
        ) : null}
        <PillButton prominent={!s.canWatchAgain} onClick={() => void watch.choose()}>
          {S.chooseAgain}
        </PillButton>
      </div>
    );
  }
  if (s.startedAt === null) return null;
  return <p className="wt-since">{S.since(clockTime(s.startedAt))}</p>;
}
