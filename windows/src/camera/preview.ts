// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { tf, t } from "../i18n";

export interface CameraDevice {
  id: string;
  name: string;
}

export interface CameraState {
  devices: CameraDevice[];
  selected: string;
  status: "idle" | "loading" | "live" | "error";
  error: string;
  stream: MediaStream | null;
}

export const emptyCamera: CameraState = { devices: [], selected: "", status: "idle", error: "", stream: null };

// WebView2/drivers may reject simultaneous opens, including overlapping page transitions.
const acquisitions = new WeakMap<MediaDevices, Promise<void>>();

export function cameraError(error: unknown): string {
  const name = error && typeof error === "object" && "name" in error ? error.name : "";
  switch (name) {
    case "NotAllowedError":
    case "SecurityError":
      return t("windows.camera.cameraAccessWasDeniedAllowCameraAccessFor");
    case "NotFoundError":
    case "OverconstrainedError":
      return t("windows.camera.thisCameraIsUnavailableConnectACameraOr");
    case "NotReadableError":
    case "AbortError":
      return t("windows.camera.theCameraCouldNotStartItMayBe");
    default:
      return t("windows.camera.theCameraCouldNotStartCheckItsConnection");
  }
}

/** Owns only the current page's stream. Late permission replies never retain an open camera. */
export class CameraPreview {
  state: CameraState;
  private request = 0;
  private discovery = 0;
  private closed = false;
  private ended: (() => void) | null = null;

  constructor(
    private media: MediaDevices,
    private changed: (state: CameraState) => void,
    selected = "",
  ) {
    this.state = { ...emptyCamera, selected };
    media.addEventListener("devicechange", this.deviceChanged);
  }

  private update(patch: Partial<CameraState>) {
    if (this.closed) return;
    this.state = { ...this.state, ...patch };
    this.changed(this.state);
  }

  private release() {
    const stream = this.state.stream;
    if (stream) {
      for (const track of stream.getTracks()) {
        if (this.ended) track.removeEventListener("ended", this.ended);
        track.stop();
      }
    }
    this.ended = null;
  }

  async start(selected = this.state.selected, fallback = true): Promise<void> {
    if (this.closed) return;
    const request = ++this.request;
    this.release();
    this.update({ selected, status: "loading", error: "", stream: null });
    const previous = acquisitions.get(this.media) ?? Promise.resolve();
    let unlock!: () => void;
    acquisitions.set(
      this.media,
      new Promise<void>((done) => {
        unlock = done;
      }),
    );
    let retryDefault = false;
    try {
      await previous;
      if (this.closed || request !== this.request) return;
      const stream = await this.media.getUserMedia({
        audio: false,
        // Some Windows drivers advertise formats they cannot actually start.
        // Use the device's working default instead of negotiating a preview format.
        video: selected ? { deviceId: { exact: selected } } : true,
      });
      if (this.closed || request !== this.request) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      const track = stream.getVideoTracks()[0];
      if (!track || track.readyState === "ended") {
        stream.getTracks().forEach((track) => track.stop());
        this.update({ status: "error", error: "The camera disconnected. Choose another camera or try again." });
        return;
      }
      this.ended = () => {
        if (this.closed || this.state.stream !== stream) return;
        this.release();
        this.update({
          stream: null,
          status: "error",
          error: "The camera disconnected. Choose another camera or try again.",
        });
        void this.refresh();
      };
      track.addEventListener("ended", this.ended);
      this.update({ stream, selected: track.getSettings().deviceId || selected, status: "live" });
      await this.refresh();
    } catch (error) {
      if (this.closed || request !== this.request) return;
      const name = error && typeof error === "object" && "name" in error ? error.name : "";
      if (selected && fallback && (name === "NotFoundError" || name === "OverconstrainedError")) {
        retryDefault = true;
      } else {
        this.update({ status: "error", error: cameraError(error) });
        await this.refresh();
      }
    } finally {
      unlock();
    }
    if (retryDefault && !this.closed && request === this.request) await this.start("", false);
  }

  async refresh() {
    const discovery = ++this.discovery;
    try {
      const devices = await this.media.enumerateDevices();
      if (this.closed || discovery !== this.discovery) return;
      this.update({
        devices: devices
          .filter((device) => device.kind === "videoinput" && device.deviceId)
          .map((device, index) => ({
            id: device.deviceId,
            name: device.label || tf("windows.camera.refresh.devices.name", index + 1),
          })),
      });
    } catch {
      // A working preview remains usable even if device discovery is temporarily unavailable.
    }
  }

  private deviceChanged = async () => {
    await this.refresh();
    if (this.closed) return;
    if (
      this.state.status === "live" &&
      this.state.selected &&
      !this.state.devices.some((d) => d.id === this.state.selected)
    ) {
      await this.start("");
    }
  };

  close() {
    this.closed = true;
    this.request++;
    this.media.removeEventListener("devicechange", this.deviceChanged);
    this.release();
    this.state = { ...this.state, stream: null, status: "idle" };
  }
}
