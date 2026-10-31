// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { ChevronDown, FlipHorizontal2, LoaderCircle, RefreshCw, VideoOff, Webcam } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useIsland } from "../island/store";
import { IconButton, PillButton } from "../island/views/Controls";
import { MenuButton } from "../shared/ui/Menu";
import { setPrefs } from "../shared/prefs";
import { CameraPreview, emptyCamera } from "./preview";
import "./camera.css";

export function CameraPage() {
  const active = useIsland(
    (s) => s.expanded && s.selected === "camera" && !s.showingSections && !s.capturePreview && s.settings.enabled,
  );
  const preferred = useIsland((s) => s.settings.cameraDeviceId);
  const [visible, setVisible] = useState(() => document.visibilityState !== "hidden");
  const [state, setState] = useState(emptyCamera);
  const [mirrored, setMirrored] = useState(true);
  const [saveError, setSaveError] = useState(false);
  const video = useRef<HTMLVideoElement>(null);
  const preview = useRef<CameraPreview | null>(null);
  const initial = useRef(preferred);
  initial.current = preferred;

  useEffect(() => {
    const changed = () => setVisible(document.visibilityState !== "hidden");
    document.addEventListener("visibilitychange", changed);
    return () => document.removeEventListener("visibilitychange", changed);
  }, []);

  useEffect(() => {
    if (!active || !visible) return;
    if (!navigator.mediaDevices?.getUserMedia) {
      setState({ ...emptyCamera, status: "error", error: "Camera preview is unavailable in this window." });
      return;
    }
    const controller = new CameraPreview(navigator.mediaDevices, setState, initial.current);
    preview.current = controller;
    void controller.start();
    return () => {
      controller.close();
      if (preview.current === controller) preview.current = null;
    };
  }, [active, visible]);

  useEffect(() => {
    const element = video.current;
    if (!element) return;
    element.srcObject = state.stream;
    if (state.stream) void element.play().catch(() => {});
    return () => {
      element.srcObject = null;
    };
  }, [state.stream]);

  const select = (id: string) => {
    void preview.current?.start(id);
    setSaveError(false);
    void setPrefs({ island: { cameraDeviceId: id } }).catch(() => setSaveError(true));
  };
  const live = active && visible && state.status === "live";
  const selected = state.devices.find((d) => d.id === state.selected);

  return (
    <section className="cam-page" aria-label="Camera preview">
      <div className="cam-view">
        <video
          ref={video}
          autoPlay
          muted
          playsInline
          aria-label="Live camera preview"
          className={mirrored ? "is-mirrored" : undefined}
          style={{ visibility: live ? "visible" : "hidden" }}
        />
        {live ? (
          <span className="cam-live">
            <i />
            Live
          </span>
        ) : (
          <div className="cam-status" role="status">
            {state.status === "loading" ? (
              <LoaderCircle className="cam-spinner" size={26} />
            ) : (
              <VideoOff size={28} strokeWidth={1.5} />
            )}
            <strong>{state.status === "loading" ? "Starting camera…" : "Camera preview"}</strong>
            <p>{state.error || "Your camera preview appears here."}</p>
            {state.status === "error" ? (
              <PillButton onClick={() => void preview.current?.start()}>Try again</PillButton>
            ) : null}
          </div>
        )}
      </div>
      <div className="cam-bar">
        <Webcam size={15} aria-hidden="true" />
        <MenuButton
          label="Choose camera"
          className="cam-picker"
          alignRight={false}
          entries={() => [
            { label: "Default camera", checked: !preferred, onSelect: () => select("") },
            ...(state.devices.length ? [{ kind: "separator" as const }] : []),
            ...state.devices.map((device) => ({
              label: device.name,
              checked: preferred === device.id,
              onSelect: () => select(device.id),
            })),
          ]}
        >
          <span>{selected?.name || "Choose camera"}</span>
          <ChevronDown size={13} />
        </MenuButton>
        <IconButton
          icon={RefreshCw}
          label="Refresh cameras"
          onClick={() => {
            void preview.current?.refresh();
          }}
        />
        <IconButton
          icon={FlipHorizontal2}
          label="Mirror preview"
          selected={mirrored}
          onClick={() => setMirrored((value) => !value)}
        />
      </div>
      {saveError ? (
        <p className="cam-save-error" role="status">
          Camera changed, but the preference could not be saved.
        </p>
      ) : null}
    </section>
  );
}
