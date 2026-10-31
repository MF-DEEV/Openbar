// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the closed island shows: configured idle content, a notice, or the hover
// preview row (NotchView.surface; spec 01-shell §2.2).

import {
  Battery,
  BatteryCharging,
  BatteryFull,
  BatteryLow,
  BatteryMedium,
  ChevronDown,
  LayoutGrid,
} from "lucide-react";
import { useContext } from "react";
import { t } from "../../i18n";
import { contentInsets, edgeInset, capsuleBody, capsuleRadius, type Geometry } from "../geometry";
import type { Size } from "../motion";
import { type Notice, useIsland } from "../store";
import { IconButton } from "./Controls";
import { DrawnSize } from "../surface/DrawnSize";
import { AudioIcons } from "../../microphone/Microphone";
import { SessionNoticeMark } from "../../sessions/SessionAvatar";
import { CLAWD_SIZES } from "../../sessions/clawd/sprite";
import type { IdleContent } from "../settings";
import { useBattery } from "../battery";
import { IdleAgents } from "./IdleAgents";
import { useMusic } from "../../music/store";
import { EqualizerBars } from "../../music/EqualizerBars";
import {
  mascotAtRest,
  mascotEnabled,
  mascotLook,
  mascotShows,
  reactsOn,
  restingMood,
  useMascot,
} from "../../mascot/service";
import { restingTrack, takesOnlyItsWing } from "../../mascot/motion";
import { MascotFace, MascotTrackView } from "../../mascot/MascotView";
import { NOTICE_SIZE } from "../../mascot/model";

export function Resting(props: { size: Size; geometry: Geometry; idleContent: IdleContent }) {
  const size = useContext(DrawnSize) ?? props.size;
  const { geometry, idleContent } = props;
  const battery = useBattery((s) => s.battery);
  const playback = useMusic((s) => s.playback);
  const tint = useMusic((s) => s.tint) ?? "#fff";
  const mascot = useMascot();
  const island = useIsland();
  const showsMascot = mascotShows(island, geometry, mascot);
  const rests = mascotAtRest(island, mascot);
  const track = restingTrack(geometry, props.size.width, mascot.side);
  // What it walks over steps aside; reacting beside a camera, it takes only its own wing.
  const ownWing = mascot.visit !== null && takesOnlyItsWing(mascot.visit.kind) && track.hidden !== null;
  const aside = (leading: boolean) =>
    mascot.stepsAside && !rests && (!ownWing || leading === (mascot.side === "left")) ? 0 : 1;
  const capsule = geometry.floatingGap !== null;
  const hasWings = capsule || (size.width - geometry.cameraWidth) / 2 >= 44;
  const side = Math.max(0, Math.min(22, geometry.stripHeight - 6));
  const BatteryIcon =
    battery?.charging || battery?.pluggedIn
      ? BatteryCharging
      : battery?.percent === null
        ? Battery
        : (battery?.percent ?? 0) <= 20
          ? BatteryLow
          : (battery?.percent ?? 100) < 85
            ? BatteryMedium
            : BatteryFull;
  const inset = capsule ? 12 : Math.min(16, edgeInset(size, 12, 0, 4, geometry.input));
  const readingWidth = capsule ? (size.width - 2 * inset - 5) / 2 : (size.width - geometry.cameraWidth) / 2 - inset;
  const label =
    idleContent === "battery" && battery
      ? `${battery.percent == null ? "Battery" : `${battery.percent}%`}${battery.charging ? ", charging" : battery.pluggedIn ? ", plugged in" : ", on battery"}`
      : idleContent === "agents"
        ? "AI Agents"
        : idleContent === "music" && playback
          ? `${playback.title}, ${playback.artist}`
          : undefined;
  return (
    <button
      type="button"
      className={`n-activation${idleContent !== "none" && hasWings ? " n-idle" : ""}${capsule ? " is-capsule" : ""}`}
      style={{
        width: size.width,
        height: size.height,
        columnGap: capsule ? 5 : geometry.cameraWidth,
        paddingInline: inset,
      }}
      aria-label={[t("island.open"), label].filter(Boolean).join(": ")}
      title={label}
      onClick={() => useIsland.getState().clickClosed()}
    >
      {idleContent !== "none" && hasWings ? (
        <>
          <span className="n-idle-wing is-leading" style={{ opacity: aside(true) }}>
            {idleContent === "battery" && battery ? <BatteryIcon size={14} aria-hidden /> : null}
            {idleContent === "agents" ? <IdleAgents leading width={readingWidth} /> : null}
            {idleContent === "music" && playback?.artwork ? (
              <img
                key={playback.artwork}
                src={playback.artwork}
                alt=""
                style={{ width: side, height: side, borderRadius: 5 }}
                onError={(event) => {
                  event.currentTarget.style.visibility = "hidden";
                }}
              />
            ) : null}
          </span>
          <span className="n-idle-wing" style={{ opacity: aside(false) }}>
            {idleContent === "battery" && battery?.percent != null ? `${battery.percent}%` : null}
            {idleContent === "agents" ? <IdleAgents leading={false} width={readingWidth} /> : null}
            {idleContent === "music" && playback?.isPlaying ? (
              <EqualizerBars count={3} width={2} height={11} color={tint} moving />
            ) : null}
          </span>
        </>
      ) : null}
      {showsMascot ? (
        <span className="mr-track" style={{ left: (size.width - track.width) / 2 }}>
          <MascotTrackView
            look={mascotLook(island.settings)}
            track={track}
            rests={rests}
            visit={mascot.visit}
            mood={restingMood(mascot)}
            reaction={mascot.reaction}
          />
        </span>
      ) : null}
    </button>
  );
}

/** Match the notice to the strip while leaving space around the curved ends. */
function noticeMetrics(geometry: Geometry, size: Size) {
  const body = geometry.floatingGap !== null ? capsuleBody(size.width, size.height, geometry.floatingGap) : null;
  const height = body?.height ?? size.height;
  const imageSize = Math.min(22, Math.max(12, height - 10));
  const fontSize = Math.min(12, Math.max(10, height * 0.44));
  const padding = body
    ? Math.max(20, body.x + 10 + capsuleRadius(height) * 0.3)
    : Math.max(20, edgeInset(size, imageSize, 4, 8, geometry.input) + 4);
  return { imageSize, fontSize, padding };
}

/** One line of text either side of the camera: title left, detail right. */
export function NoticeView(props: { notice: Notice; geometry: Geometry; size: Size }) {
  const { notice, geometry } = props;
  const size = useContext(DrawnSize) ?? props.size;
  const camera = geometry.floatingGap === null ? geometry.cameraWidth : 0;
  const metrics = noticeMetrics(geometry, size);
  const settings = useIsland((s) => s.settings);
  const keepAwake = useMascot((s) => s.keepAwake);
  // With the companion on, it stands in for the symbol and reacts, unless its reactions are off.
  const mascot = mascotEnabled(settings) ? notice.mascot : undefined;
  return (
    <button
      type="button"
      className="n-notice"
      aria-label={[notice.title, notice.detail].filter(Boolean).join(": ")}
      title={[notice.title, notice.detail].filter(Boolean).join("\n")}
      style={{
        width: size.width,
        height: size.height,
        columnGap: camera,
        paddingInline: metrics.padding,
        fontSize: metrics.fontSize,
        lineHeight: "1.25",
      }}
      onClick={() => useIsland.getState().clickClosed()}
    >
      <span className="n-notice-title" style={{ color: notice.tint }}>
        {mascot ? (
          <MascotFace
            key={notice.id}
            look={mascotLook(settings)}
            size={Math.min(NOTICE_SIZE, metrics.imageSize)}
            mood={restingMood({ keepAwake })}
            reaction={reactsOn(settings) ? mascot : undefined}
          />
        ) : null}
        {notice.audio && !mascot ? <AudioIcons muted={notice.audio.muted} deafened={notice.audio.deafened} /> : null}
        {notice.icon && !mascot ? <notice.icon size={14} strokeWidth={2} aria-hidden="true" /> : null}
        {notice.session ? (
          <SessionNoticeMark key={notice.session.id} session={notice.session} height={metrics.imageSize + 4} />
        ) : null}
        {notice.image && !mascot ? (
          <img
            key={notice.image}
            className="n-notice-image"
            src={notice.image}
            alt=""
            onError={(event) => {
              const image = event.currentTarget;
              if (notice.imageFallback && image.getAttribute("src") !== notice.imageFallback) {
                image.src = notice.imageFallback;
              } else {
                image.style.visibility = "hidden";
              }
            }}
            style={{ width: metrics.imageSize, height: metrics.imageSize }}
          />
        ) : null}
        <span>{notice.title}</span>
      </span>
      {notice.detail ? (
        <span
          className="n-notice-detail"
          style={{ fontSize: Math.max(9, metrics.fontSize - 1), color: notice.audio?.muted ? "#ff453a" : undefined }}
        >
          {notice.detail}
        </span>
      ) : null}
      {notice.progress !== undefined ? (
        <span
          className={`n-download-progress${notice.progress === null ? " is-indeterminate" : ""}`}
          aria-hidden="true"
        >
          <span style={notice.progress === null ? undefined : { width: `${notice.progress * 100}%` }} />
        </span>
      ) : null}
    </button>
  );
}

/** Measures a notice's natural width so the island fits it (NotchNotice.preferredWings). */
export function noticeWidth(notice: Notice, geometry: Geometry): number {
  const ctx = measureContext();
  const metrics = noticeMetrics(geometry, { width: geometry.screen.width - 24, height: geometry.stripHeight });
  ctx.font = `500 ${metrics.fontSize}px 'Inter Variable', Inter, sans-serif`;
  // The companion takes the symbol slot, whatever mark the notice would show.
  const marks =
    notice.mascot && mascotEnabled(useIsland.getState().settings)
      ? NOTICE_SIZE + 8
      : (notice.image ? metrics.imageSize + 8 : 0) +
        (notice.icon ? 14 + 8 : 0) +
        (notice.audio ? (notice.audio.deafened ? 47 : 24) : 0);
  const title = ctx.measureText(notice.title).width + marks + (notice.session ? CLAWD_SIZES.notice.width + 8 : 0);
  ctx.font = `500 ${Math.max(9, metrics.fontSize - 1)}px 'Inter Variable', Inter, sans-serif`;
  const detail = notice.detail ? ctx.measureText(notice.detail).width : 0;
  const maxWing = (geometry.screen.width - 24 - geometry.cameraWidth) / 2;
  if (geometry.floatingGap !== null) {
    return Math.min(
      368,
      geometry.screen.width - 24,
      Math.ceil(2 * metrics.padding + title + (detail ? 12 + detail : 0)),
    );
  }
  const leading = Math.min(Math.max(Math.ceil(title + metrics.padding + 10), 36), Math.min(240, maxWing));
  const trailing = Math.min(Math.max(Math.ceil(detail + metrics.padding + 10), 36), Math.min(240, maxWing));
  return geometry.cameraWidth + 2 * Math.max(leading, trailing);
}

let canvas: HTMLCanvasElement | undefined;
export function measureContext(): CanvasRenderingContext2D {
  canvas ??= document.createElement("canvas");
  return canvas.getContext("2d")!;
}

/** The hover preview: where opening leads, and a chevron to open. */
export function Peek(props: { geometry: Geometry; size: Size }) {
  const size = useContext(DrawnSize) ?? props.size;
  const selected = useIsland((s) => s.selected);
  return (
    <div
      className="n-peek"
      style={{
        width: size.width,
        height: size.height,
        paddingTop: props.geometry.safeContentTop,
        paddingInline: contentInsets(props.geometry, size).horizontal,
      }}
    >
      <button
        type="button"
        className="n-peek-nav"
        onClick={() => useIsland.getState().open(undefined, { sections: true })}
      >
        <LayoutGrid size={14} strokeWidth={2} />
        <span>{t(`module.${selected}`)}</span>
      </button>
      <IconButton icon={ChevronDown} label={t("island.open")} onClick={() => useIsland.getState().open()} />
    </div>
  );
}
