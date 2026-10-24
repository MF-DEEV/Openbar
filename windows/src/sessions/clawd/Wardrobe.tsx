// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Where a session's Clawd is dressed (NotchClawdWardrobeView.swift; spec
// 02-sessions §5.7): the Clawd itself on a stage at the left, doing what its
// session does, and a row for each part of its costume on the right. A
// choice dresses it at once and it hops for it.

import { ChevronLeft, ChevronRight, Dices, Slash, Undo2 } from "lucide-react";
import { useState, type KeyboardEvent, type ReactNode } from "react";
import { t } from "../../i18n";
import { Clawd, ClawdPicture } from "./Clawd";
import {
  CLASSIC,
  EXTRAS,
  EYEWEAR,
  HATS,
  keptCode,
  randomCostume,
  sameCostume,
  seed,
  seeded,
  SHELL_RGB,
  SHELLS,
  type Character,
  type ClawdCostume,
} from "./costume";
import { CLAWD_SIZES, FRAME, HEIGHT, WIDTH, type ClawdMotion } from "./sprite";
import { clawdStrings, type ClawdStrings } from "./strings";
import "./clawd.css";

export interface WardrobeProps {
  sessionId: string;
  /** Who is dressed: Clawd for Claude Code, the robot for Codex. Its lot follows. */
  character?: Character;
  /** What it wears now: the kept choice, or its own lot (resolvedCostume). */
  costume: ClawdCostume;
  /**
   * A new costume was chosen. `kept` is what to store for the session
   * (keptCode): null when it is the session's own lot, so the choice follows
   * the lot; Reset sends the lot with null.
   */
  onChange: (costume: ClawdCostume, kept: string | null) => void;
  /** Back to the session page; also on Escape. Without it the band has no back action. */
  onBack?: () => void;
  /** The session's name and project, under the stage. */
  name?: string;
  project?: string;
  /** What the session does, for the live Clawd on the stage. */
  motion?: ClawdMotion;
  since?: number | Date | null;
  /** When the session's work ended, which the Clawd hops for. */
  cheer?: number | Date | null;
  /** Whether the person dressed it by hand (enables Reset). Default: it differs from the lot. */
  dressedByHand?: boolean;
  /** The island's width; the stage grows with it, from 150 to 230. */
  islandWidth?: number;
  /** The session started waiting meanwhile: a bar on top that returns to the page. */
  waiting?: { label: string; tint: string; icon?: ReactNode; onGo: () => void } | null;
  /** Draws the band (back, Surprise me, Reset) here; off when the island draws its own band. */
  band?: boolean;
  strings?: ClawdStrings;
}

type Part = "hat" | "eyewear" | "extra";

/** The project's color, the same for the same folder name (NotchClaudeProjectTag). */
const PROJECT_PALETTE = [
  "rgb(107, 163, 255)",
  "rgb(184, 140, 255)",
  "rgb(255, 128, 189)",
  "rgb(69, 209, 214)",
  "rgb(158, 214, 92)",
  "rgb(214, 173, 133)",
  "rgb(97, 219, 255)",
  "rgb(219, 199, 255)",
];

export function projectColor(project: string): string {
  return PROJECT_PALETTE[Number(seed(project.toLowerCase()) % BigInt(PROJECT_PALETTE.length))];
}

export function ProjectTag(props: { project: string }) {
  const tint = projectColor(props.project);
  return (
    <span
      className="clawd-project"
      style={{ color: tint, background: `color-mix(in srgb, ${tint} 16%, transparent)` }}
      title={props.project}
    >
      {props.project}
    </span>
  );
}

export function Wardrobe(props: WardrobeProps) {
  const { sessionId, costume, onChange } = props;
  const character = props.character ?? "clawd";
  const text = props.strings ?? clawdStrings;
  const band = props.band ?? true;
  // When the costume last changed here, which the Clawd hops for.
  const [changed, setChanged] = useState<number | null>(null);
  const dressedByHand = props.dressedByHand ?? !sameCostume(costume, seeded(sessionId, null, character));

  const stageWidth = Math.min(230, Math.max(150, (props.islandWidth ?? 0) * 0.3));
  const stageHeight = stageWidth * 0.92;
  // The whole canvas fits the card, so a tall hat, a raised arm and the
  // highest jump all stay inside it.
  const margin = 10;
  const pixel = Math.min((stageWidth - margin * 2) / WIDTH, (stageHeight - margin * 2) / HEIGHT);
  const figure = { width: pixel * FRAME.columns, height: pixel * FRAME.rows };

  const names = {
    shell: (v: ClawdCostume["shell"]) => text.shells[v],
    hat: (v: ClawdCostume["hat"]) => (v === "none" ? text.nothing : text.hats[v]),
    eyewear: (v: ClawdCostume["eyewear"]) => (v === "none" ? text.nothing : text.eyewears[v]),
    extra: (v: ClawdCostume["extra"]) => (v === "none" ? text.nothing : text.extras[v]),
  };
  const outfit = [
    names.shell(costume.shell),
    costume.hat === "none" ? null : names.hat(costume.hat),
    costume.eyewear === "none" ? null : names.eyewear(costume.eyewear),
    costume.extra === "none" ? null : names.extra(costume.extra),
  ]
    .filter(Boolean)
    .join(", ");

  const wear = (next: ClawdCostume, kept: string | null) => {
    onChange(next, kept);
    setChanged(Date.now());
  };
  const dress = (next: ClawdCostume) => {
    if (!sameCostume(next, costume)) wear(next, keptCode(next, sessionId, character));
  };

  const onKeyDown = (event: KeyboardEvent) => {
    // Escape closes the wardrobe first, then the page, then the island.
    if (event.key === "Escape" && props.onBack) {
      event.stopPropagation();
      props.onBack();
    }
  };

  const tile = (part: Part, value: string, selected: boolean, label: string, onClick: () => void) => {
    const wearing = value === "none" ? null : { ...CLASSIC, shell: costume.shell, [part]: value };
    return (
      <button
        key={value}
        type="button"
        className={`clawd-tile${selected ? " is-selected" : ""}`}
        aria-label={label}
        aria-pressed={selected}
        title={label}
        onClick={onClick}
      >
        {wearing ? (
          <ClawdPicture
            character={character}
            costume={wearing as ClawdCostume}
            width={CLAWD_SIZES.wardrobeTile.width}
            height={CLAWD_SIZES.wardrobeTile.height}
            style={{ transform: "translateY(3px)" }}
          />
        ) : (
          <Slash size={15} strokeWidth={1.5} className="clawd-tile-none" />
        )}
      </button>
    );
  };

  const section = (title: string, chosen: string, children: ReactNode, grid = "clawd-tiles") => (
    <section className="clawd-part" aria-label={title}>
      <div className="clawd-heading">
        <span>{title}</span>
        <span className="clawd-heading-chosen">{chosen}</span>
      </div>
      <div className={grid}>{children}</div>
    </section>
  );

  return (
    <div className="clawd-wardrobe" onKeyDown={onKeyDown}>
      {band ? (
        <div className="clawd-band">
          <button type="button" className="clawd-back" onClick={props.onBack} aria-label={t("island.back")}>
            <ChevronLeft size={14} strokeWidth={2.5} />
            <span>{text.wardrobe}</span>
          </button>
          <div className="clawd-band-trailing">
            <button
              type="button"
              className="clawd-surprise"
              title={text.surpriseMe}
              onClick={() => {
                const next = randomCostume(costume);
                wear(next, keptCode(next, sessionId, character));
              }}
            >
              <Dices size={13} strokeWidth={2.25} />
              <span>{text.surpriseMe}</span>
            </button>
            <button
              type="button"
              className="clawd-reset"
              title={text.ownOutfit}
              aria-label={text.ownOutfit}
              disabled={!dressedByHand}
              onClick={() => wear(seeded(sessionId, null, character), null)}
            >
              <Undo2 size={13} strokeWidth={2.25} />
            </button>
          </div>
        </div>
      ) : null}

      {props.waiting ? (
        <button
          type="button"
          className="clawd-waiting"
          style={{
            color: props.waiting.tint,
            background: `color-mix(in srgb, ${props.waiting.tint} 14%, transparent)`,
          }}
          onClick={props.waiting.onGo}
        >
          {props.waiting.icon}
          <span className="clawd-waiting-label">{props.waiting.label}</span>
          <span className="clawd-waiting-go">{text.goToSession}</span>
          <ChevronRight size={11} strokeWidth={3} />
        </button>
      ) : null}

      <div className="clawd-wardrobe-body">
        <div className="clawd-stage-column" style={{ width: stageWidth }}>
          <div
            className="clawd-stage"
            role="img"
            aria-label={outfit}
            style={{ width: stageWidth, height: stageHeight, paddingBottom: margin + pixel }}
          >
            {/* The floor it stands on, a soft light under its feet. */}
            <div className="clawd-floor" style={{ width: figure.width * 0.9, height: pixel * 2, bottom: margin }} />
            <Clawd
              character={character}
              costume={costume}
              motion={props.motion ?? "rest"}
              seed={sessionId}
              since={props.since}
              cheer={changed ?? props.cheer}
              width={figure.width}
              height={figure.height}
            />
          </div>
          {props.name ? <div className="clawd-stage-name">{props.name}</div> : null}
          {props.project ? <ProjectTag project={props.project} /> : null}
        </div>

        <div className="clawd-parts">
          {section(
            text.shellTitle,
            names.shell(costume.shell),
            SHELLS.map((shell) => {
              const selected = shell === costume.shell;
              return (
                <button
                  key={shell}
                  type="button"
                  className={`clawd-swatch${selected ? " is-selected" : ""}`}
                  style={{ color: `rgb(${SHELL_RGB[shell].join(",")})` }}
                  aria-label={names.shell(shell)}
                  aria-pressed={selected}
                  title={names.shell(shell)}
                  onClick={() => dress({ ...costume, shell })}
                />
              );
            }),
            "clawd-swatches",
          )}
          {section(
            text.hatTitle,
            names.hat(costume.hat),
            HATS.map((hat) => tile("hat", hat, hat === costume.hat, names.hat(hat), () => dress({ ...costume, hat }))),
          )}
          {section(
            text.eyewearTitle,
            names.eyewear(costume.eyewear),
            EYEWEAR.map((eyewear) =>
              tile("eyewear", eyewear, eyewear === costume.eyewear, names.eyewear(eyewear), () =>
                dress({ ...costume, eyewear }),
              ),
            ),
          )}
          {section(
            text.extrasTitle,
            names.extra(costume.extra),
            EXTRAS.map((extra) =>
              tile("extra", extra, extra === costume.extra, names.extra(extra), () => dress({ ...costume, extra })),
            ),
          )}
        </div>
      </div>
    </div>
  );
}
