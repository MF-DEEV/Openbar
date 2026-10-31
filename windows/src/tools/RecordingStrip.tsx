// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island while recording: a red mark and the time, and a click
// anywhere on it stops (RecorderIndicator).

import { useEffect, useState } from "react";
import { edgeInset } from "../island/geometry";
import type { ActivityViewProps } from "../island/views/Activity";
import { bridge } from "../shared/bridge";
import { S } from "./strings";
import { useRecording } from "./recording";

export const clockText = (seconds: number) => {
  const whole = Math.max(0, Math.floor(seconds));
  const two = (n: number) => String(n).padStart(2, "0");
  return `${Math.floor(whole / 60)}:${two(whole % 60)}`;
};

export function RecordingStrip({ geometry, size }: ActivityViewProps) {
  const startedAt = useRecording((s) => s.startedAt);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(timer);
  }, []);
  const capsule = geometry.floatingGap !== null;
  const inset = capsule ? 12 : Math.min(16, edgeInset(size, 12, 0, 4, geometry.input));
  return (
    <button
      type="button"
      className="n-activation n-idle n-recording"
      style={{
        width: size.width,
        height: size.height,
        columnGap: capsule ? 5 : geometry.cameraWidth,
        paddingInline: inset,
      }}
      aria-label={S.recordingStop}
      title={S.recordingStop}
      onClick={(e) => {
        e.stopPropagation();
        void bridge.capture.recorder.stop();
      }}
    >
      <span className="n-idle-wing is-leading">
        <span className="n-recording-dot" aria-hidden />
      </span>
      <span className="n-idle-wing">{clockText((now - (startedAt ?? now)) / 1000)}</span>
    </button>
  );
}
