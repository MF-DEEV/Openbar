// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The companion's own page (NotchMascotSettings.swift). It lives at the top in
// a slice of the island, bigger than life, and acts out what it does on
// request; below are how it looks and how it behaves. The Mac's Command Bar
// card has nothing to point at here: Windows has no Command Bar yet.

import {
  AlarmClock,
  ArrowDownToLine,
  BatteryLow,
  CalendarDays,
  Camera,
  Clock,
  Coffee,
  EyeOff,
  Footprints,
  Hand,
  Hourglass,
  LockOpen,
  type LucideIcon,
  MicOff,
  Music,
  Palette,
  PersonStanding,
  Shapes,
  Smile,
  Sparkles,
  TriangleAlert,
  VenetianMask,
  Zap,
} from "lucide-react";
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { islandPath, makeGeometry, restingSize } from "../island/geometry";
import { MascotFace, MascotTrackView } from "../mascot/MascotView";
import {
  type MascotFrequency,
  type MascotLook,
  type MascotMoment,
  MASCOT_FREQUENCIES,
  MASCOT_MOMENTS,
  MASCOT_PALETTES,
  MASCOT_SHAPES,
  MASCOT_SIDES,
  MASCOT_STYLES,
  type MascotReaction,
  type MascotReactionEvent,
  type MascotSide,
  MOMENT_REACTION,
  momentFollowUp,
  paletteColors,
  REACTION_LENGTH,
} from "../mascot/model";
import {
  makeTrack,
  type MascotTrack,
  type MascotVisit,
  scaledTrack,
  type VisitKind,
  visitDuration,
} from "../mascot/motion";
import { mediaTime } from "../mascot/rig";
import { bridge } from "../shared/bridge";
import { useSettings } from "./state";
import { T } from "./strings";
import { Button, Group, Page, Row, Select, Toggle } from "./ui";

const S = T.mascot;

const MOMENT_ICONS: Record<MascotMoment, LucideIcon> = {
  visit: Footprints,
  timerStarted: Hourglass,
  timeIsUp: AlarmClock,
  music: Music,
  agents: Sparkles,
  eventStarts: CalendarDays,
  downloadFinished: ArrowDownToLine,
  downloadFailed: TriangleAlert,
  screenshot: Camera,
  micMuted: MicOff,
  keepAwake: Coffee,
  charging: Zap,
  lowBattery: BatteryLow,
  unlocked: LockOpen,
};

const REACTING_MOMENTS = MASCOT_MOMENTS.filter((moment) => MOMENT_REACTION[moment] !== null);
const STAGE_HEIGHT = 104;
const HIDE_AWAY: VisitKind = { type: "linger", reaction: "yawn" };

export function MascotSettings() {
  const island = useSettings((s) => s.prefs.island);
  const set = useSettings((s) => s.island);
  const look: MascotLook = { style: island.mascotStyle, shape: island.mascotShape, palette: island.mascotPalette };
  const notch = island.silhouette === "notch";
  return (
    <Page title={S.title} lead={S.hint}>
      <Group>
        <Row icon={Smile} title={S.title} caption={S.switchCaption}>
          <Toggle label={S.title} checked={island.mascotEnabled} onChange={(on) => set("mascotEnabled", on)} />
        </Row>
        {!island.enabled ? (
          <Row icon={TriangleAlert} title={S.livesInIsland}>
            <Button onClick={() => set("enabled", true)}>{S.turnOnIsland}</Button>
          </Row>
        ) : null}
      </Group>
      <StageCard look={look} />
      <Group title={S.appearance}>
        <div className="st-card st-mascot-card">
          <div className="st-mascot-heading">
            <Palette className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden />
            <span className="st-card-title">{S.style}</span>
          </div>
          <div className="st-mascot-tiles" role="radiogroup" aria-label={S.style}>
            {MASCOT_STYLES.map((style) => (
              <Tile
                key={style}
                title={S.styles[style]}
                selected={island.mascotStyle === style}
                onSelect={() => set("mascotStyle", style)}
              >
                <span className="st-mascot-sample">
                  <MascotFace look={{ ...look, style }} size={30} />
                </span>
              </Tile>
            ))}
          </div>
          {island.mascotStyle === "minimal" ? (
            <>
              <div className="st-mascot-heading">
                <Shapes className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden />
                <span className="st-card-title">{S.shape}</span>
                <span className="st-mascot-value">{S.shapes[island.mascotShape]}</span>
              </div>
              <div className="st-mascot-shapes" role="radiogroup" aria-label={S.shape}>
                {MASCOT_SHAPES.map((shape) => (
                  <button
                    key={shape}
                    type="button"
                    role="radio"
                    className="st-mascot-shape"
                    aria-checked={island.mascotShape === shape}
                    aria-label={S.shapes[shape]}
                    title={S.shapes[shape]}
                    onClick={() => set("mascotShape", shape)}
                  >
                    <MascotFace look={{ ...look, style: "minimal", shape }} size={24} />
                  </button>
                ))}
              </div>
            </>
          ) : null}
          <div className="st-mascot-heading">
            <Palette className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden />
            <span className="st-card-title">{S.color}</span>
            <span className="st-mascot-value">{S.palettes[island.mascotPalette]}</span>
          </div>
          <div className="st-mascot-swatches" role="radiogroup" aria-label={S.color}>
            {MASCOT_PALETTES.map((palette) => {
              const colors = paletteColors(palette);
              return (
                <button
                  key={palette}
                  type="button"
                  role="radio"
                  className="st-mascot-swatch"
                  aria-checked={island.mascotPalette === palette}
                  aria-label={S.palettes[palette]}
                  title={S.palettes[palette]}
                  onClick={() => set("mascotPalette", palette)}
                >
                  <span style={{ background: `linear-gradient(${colors.light}, ${colors.shade})` }} />
                </button>
              );
            })}
          </div>
        </div>
      </Group>
      <Group title={S.behavior}>
        {notch ? (
          <div className="st-card st-mascot-card">
            <div className="st-mascot-heading">
              <PersonStanding className="st-card-icon" size={18} strokeWidth={1.75} aria-hidden />
              <span className="st-card-title">{S.side}</span>
            </div>
            <div className="st-mascot-tiles" role="radiogroup" aria-label={S.side}>
              {MASCOT_SIDES.map((side) => (
                <Tile
                  key={side}
                  title={S.sides[side]}
                  selected={island.mascotSide === side}
                  onSelect={() => set("mascotSide", side)}
                >
                  <span className={`st-mascot-side is-${side}`}>
                    <MascotFace look={look} size={13} />
                  </span>
                </Tile>
              ))}
            </div>
          </div>
        ) : null}
        <Row icon={EyeOff} title={S.hidesWhenIdle} caption={S.hidesWhenIdleHint}>
          <Toggle
            label={S.hidesWhenIdle}
            checked={island.mascotHidesWhenIdle}
            onChange={(on) => set("mascotHidesWhenIdle", on)}
          />
        </Row>
        <Row icon={Footprints} title={S.visits} caption={S.visitsHint}>
          <Toggle label={S.visits} checked={island.mascotVisits} onChange={(on) => set("mascotVisits", on)} />
        </Row>
        {island.mascotVisits ? (
          <Row icon={Clock} title={S.frequency}>
            <Select<MascotFrequency>
              label={S.frequency}
              value={island.mascotVisitFrequency}
              options={MASCOT_FREQUENCIES.map((value) => ({ value, label: S.frequencies[value] }))}
              onChange={(value) => set("mascotVisitFrequency", value)}
            />
          </Row>
        ) : null}
        <Row icon={VenetianMask} title={S.reactions} caption={S.reactionsHint}>
          <Toggle label={S.reactions} checked={island.mascotReactions} onChange={(on) => set("mascotReactions", on)} />
        </Row>
      </Group>
    </Page>
  );
}

function Tile(props: { title: string; selected: boolean; onSelect: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={props.selected}
      className="st-mascot-tile"
      onClick={props.onSelect}
    >
      {props.children}
      <span>{props.title}</span>
    </button>
  );
}

// MARK: Stage

/**
 * The companion in a slice of the island as it rests there, bigger: it blinks,
 * its eyes follow the pointer, a moment's rest of the pointer on it pets it, and
 * it acts out a visit or a moment on request. Switched off, it sleeps.
 */
function StageCard({ look }: { look: MascotLook }) {
  const island = useSettings((s) => s.prefs.island);
  const awake = island.mascotEnabled;
  const { mascotVisits: visits, mascotReactions: reactions, mascotHidesWhenIdle: hides, mascotSide: side } = island;
  const [visit, setVisit] = useState<MascotVisit | null>(null);
  const [reaction, setReaction] = useState<MascotReactionEvent | null>(null);
  const [playing, setPlaying] = useState<MascotMoment | null>(null);
  const [width, setWidth] = useState(0);
  const nextMoment = useRef(0);
  const nextId = useRef(1);
  const playingTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const lookTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const visitNow = useRef<MascotVisit | null>(null);
  visitNow.current = visit;

  const react = useCallback((next: MascotReaction) => {
    setReaction({ id: nextId.current++, reaction: next, start: mediaTime() });
  }, []);

  /** Starts a stroll and clears it once it's over, returning how long it takes. */
  const startVisit = useCallback((kind: VisitKind, greeting: MascotVisit["greeting"]) => {
    const started: MascotVisit = { id: nextId.current++, kind, greeting, start: mediaTime() };
    setVisit(started);
    const duration = visitDuration(kind);
    setTimeout(() => setVisit((current) => (current?.id === started.id ? null : current)), duration * 1000);
    return duration;
  }, []);

  const play = useCallback(
    (moment: MascotMoment) => {
      if (!awake || visitNow.current) return;
      clearTimeout(playingTimer.current);
      setPlaying(moment);
      const first = MOMENT_REACTION[moment];
      let length: number;
      if (first) {
        react(first);
        length = REACTION_LENGTH[first] + 0.15;
      } else {
        length = startVisit({ type: "lap" }, "wink");
      }
      const followUp = momentFollowUp(moment);
      if (!followUp) {
        playingTimer.current = setTimeout(() => setPlaying(null), length * 1000);
        return;
      }
      // The second beat, then the caption goes once it's over too.
      playingTimer.current = setTimeout(
        () => {
          react(followUp);
          playingTimer.current = setTimeout(() => setPlaying(null), (REACTION_LENGTH[followUp] + 0.15) * 1000);
        },
        (length + 0.2) * 1000,
      );
    },
    [awake, react, startVisit],
  );

  const playNext = () => {
    if (!awake || visit) return;
    play(REACTING_MOMENTS[nextMoment.current++ % REACTING_MOMENTS.length]!);
  };

  // Each setting it answers to shows what it brings, only after a change.
  const before = useRef({ awake, side, visits, reactions, hides, look });
  useEffect(() => {
    const was = before.current;
    before.current = { awake, side, visits, reactions, hides, look };
    if (was.awake !== awake) {
      // Switched on, it wakes up with a stretch, and off, it yawns and dozes off.
      clearTimeout(playingTimer.current);
      setPlaying(null);
      setVisit(null);
      react(awake ? "wakeUp" : "yawn");
      return;
    }
    if (!awake || visitNow.current) return;
    if (was.side !== side) {
      startVisit({ type: "cross" }, "wink");
    } else if (!was.visits && visits) {
      play("visit");
    } else if (!was.reactions && reactions) {
      react("perk");
    } else if (!was.hides && hides) {
      // It shows how it goes into the island, and comes back out, as the preview keeps it in sight.
      clearTimeout(playingTimer.current);
      setPlaying(null);
      const length = startVisit(HIDE_AWAY, "idle");
      playingTimer.current = setTimeout(() => startVisit({ type: "arrive" }, "wink"), (length + 0.8) * 1000);
    } else if (was.look !== look && JSON.stringify(was.look) !== JSON.stringify(look)) {
      // A new look, and it's glad of it, once the choosing settles.
      clearTimeout(lookTimer.current);
      lookTimer.current = setTimeout(() => react("celebrate"), 350);
    }
  }, [awake, side, visits, reactions, hides, look, play, react, startVisit]);

  useEffect(
    () => () => {
      clearTimeout(playingTimer.current);
      clearTimeout(lookTimer.current);
    },
    [],
  );

  const measure = useCallback((element: HTMLDivElement | null) => {
    if (!element) return undefined;
    const observer = new ResizeObserver(([entry]) => setWidth(entry!.contentRect.width));
    observer.observe(element);
    setWidth(element.clientWidth);
    return () => observer.disconnect();
    return () => observer.disconnect();
  }, []);

  const greet = () => {
    // Out of room in the island just now, it says hello here instead.
    void bridge.mascot.greet().then((greeted) => {
      if (!greeted) play("visit");
    });
  };

  return (
    <div className="st-group">
      <div className="st-mascot-stage-card">
        <div
          ref={measure}
          className="st-mascot-stage"
          role="button"
          tabIndex={awake ? 0 : -1}
          aria-label={S.preview}
          aria-disabled={!awake}
          onClick={playNext}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              playNext();
            }
          }}
        >
          {width > 0 ? (
            <Stage
              look={look}
              side={side}
              awake={awake}
              visit={visit}
              reaction={reaction}
              width={width}
              moment={playing}
            />
          ) : null}
        </div>
        <div className="st-mascot-moments-head">
          <h2 className="st-group-title">{S.momentsTitle}</h2>
          <Button onClick={greet} disabled={!awake || !island.enabled}>
            <span className="st-mascot-hi" title={S.sayHiHint}>
              <Hand size={14} aria-hidden />
              {S.sayHi}
            </span>
          </Button>
        </div>
        <div className="st-mascot-moments">
          {MASCOT_MOMENTS.map((moment) => {
            const Icon = MOMENT_ICONS[moment];
            return (
              <button
                key={moment}
                type="button"
                className="st-mascot-chip"
                aria-pressed={playing === moment}
                disabled={!awake}
                style={{ opacity: visit && playing !== moment ? 0.45 : undefined }}
                onClick={() => play(moment)}
              >
                <Icon size={13} strokeWidth={2} aria-hidden />
                {S.moments[moment]}
              </button>
            );
          })}
        </div>
        <p className={`st-mascot-tip${awake ? "" : " is-asleep"}`}>{`${S.previewHint} ${S.petTip}`}</p>
      </div>
    </div>
  );
}

function stageModel(island: ReturnType<typeof useSettings.getState>["prefs"]["island"], side: MascotSide) {
  const geometry = makeGeometry({
    screen: { width: 1920, height: 1080 },
    devicePixelRatio: 1,
    silhouette: island.silhouette,
    layout: island.layout,
    notchWidth: island.notchWidth,
    notchHeight: island.notchHeight,
    roundness: island.roundness,
    shoulderWidth: island.shoulderWidth,
    shoulderHeight: island.shoulderHeight,
    capsuleFit: island.capsuleFit,
  });
  const floats = geometry.floatingGap !== null;
  // A crowded display can leave the closed island no wings, but the preview always shows where it would rest.
  const wing = 44;
  const strip = floats
    ? restingSize(geometry)
    : { width: geometry.cameraWidth + wing * 2, height: geometry.stripHeight };
  const track: MascotTrack = makeTrack({
    stripWidth: strip.width,
    stripHeight: strip.height,
    wing: floats ? 0 : wing,
    cameraWidth: floats ? 0 : geometry.cameraWidth,
    floats,
    bodyHeight: floats ? strip.height - 2 * (geometry.floatingGap ?? 0) : strip.height,
    side,
  });
  return { geometry, strip, track, menuBar: geometry.menuBarHeight };
}

/** The slice of screen the stage draws: a bar with the island hanging from it, the companion in its place. */
function Stage(props: {
  look: MascotLook;
  side: MascotSide;
  awake: boolean;
  visit: MascotVisit | null;
  reaction: MascotReactionEvent | null;
  width: number;
  moment: MascotMoment | null;
}) {
  const island = useSettings((s) => s.prefs.island);
  const model = stageModel(island, props.side);
  // As large as leaves the bar room on both sides, at most twice its size.
  const scale = Math.max(1, Math.min(2, (props.width * 0.66) / model.strip.width, (STAGE_HEIGHT - 36) / model.menuBar));
  const track = scaledTrack(model.track, scale);
  const size = { width: model.strip.width * scale, height: model.strip.height * scale };
  const Icon = props.moment ? MOMENT_ICONS[props.moment] : null;
  return (
    <>
      <span className="st-mascot-bar" style={{ height: model.menuBar * scale }} aria-hidden />
      <svg
        className="st-mascot-island"
        width={size.width}
        height={size.height}
        viewBox={`0 0 ${model.strip.width} ${model.strip.height}`}
        aria-hidden
      >
        <path d={islandPath(model.geometry, model.strip)} fill="#000" />
      </svg>
      <span
        className="st-mascot-track"
        style={{ width: size.width, height: size.height, clipPath: `path("${scaledOutline(model, scale)}")` }}
      >
        <MascotTrackView
          look={props.look}
          track={track}
          rests
          visit={props.visit}
          reaction={props.reaction}
          awake={props.awake}
        />
      </span>
      {!props.awake ? <SleepZs track={track} /> : null}
      {props.moment && Icon ? (
        <span className="st-mascot-caption" key={props.moment}>
          <Icon size={12} strokeWidth={2} aria-hidden />
          {S.moments[props.moment]}
        </span>
      ) : null}
    </>
  );
}

function scaledOutline(model: ReturnType<typeof stageModel>, scale: number) {
  return islandPath(
    { ...model.geometry, floatingGap: model.geometry.floatingGap === null ? null : model.geometry.floatingGap * scale },
    { width: model.strip.width * scale, height: model.strip.height * scale },
  );
}

/** Asleep, little z's drift up from its head and fade. With reduced motion two of them stay still. */
function SleepZs({ track }: { track: MascotTrack }) {
  return (
    <span
      className="st-mascot-zs"
      style={{
        left: `calc(50% - ${track.width / 2}px + ${track.rest + (track.mirrored ? -1 : 1) * track.size * 0.6}px)`,
        top: track.baseline - track.size * 0.3,
        fontSize: track.size * 0.42,
        ["--mr-toward" as string]: track.mirrored ? -1 : 1,
      }}
      aria-hidden
    >
      <i>z</i>
      <i>z</i>
      <i>z</i>
    </span>
  );
}
