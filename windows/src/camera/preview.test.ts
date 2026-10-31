// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it, vi } from "vitest";
import { resolve } from "../shared/prefs";
import { CameraPreview, cameraError } from "./preview";

function stream(id: string) {
  const track = Object.assign(new EventTarget(), {
    readyState: "live",
    stop: vi.fn(() => {
      track.readyState = "ended";
    }),
    getSettings: () => ({ deviceId: id }),
  });
  return {
    track,
    value: { getTracks: () => [track], getVideoTracks: () => [track] } as unknown as MediaStream,
  };
}

function setup() {
  const media = Object.assign(new EventTarget(), {
    getUserMedia: vi.fn(),
    enumerateDevices: vi.fn().mockResolvedValue([
      { kind: "audioinput", deviceId: "mic", label: "Microphone" },
      { kind: "videoinput", deviceId: "first", label: "Front camera" },
      { kind: "videoinput", deviceId: "second", label: "" },
    ]),
  });
  const changed = vi.fn();
  const preview = new CameraPreview(media as unknown as MediaDevices, changed);
  return { preview, media, changed };
}

describe("Camera preview", () => {
  it("opens only video and lists named cameras after permission is granted", async () => {
    const { preview, media } = setup();
    const first = stream("first");
    media.getUserMedia.mockResolvedValue(first.value);
    await preview.start();
    expect(media.getUserMedia.mock.calls[0][0]).toEqual({ audio: false, video: true });
    expect(preview.state.status).toBe("live");
    expect(preview.state.selected).toBe("first");
    expect(preview.state.devices).toEqual([
      { id: "first", name: "Front camera" },
      { id: "second", name: "Camera 2" },
    ]);
    preview.close();
    expect(first.track.stop).toHaveBeenCalledOnce();
  });

  it("releases the old camera before switching to the exact selected device", async () => {
    const { preview, media } = setup();
    const first = stream("first"),
      second = stream("second");
    media.getUserMedia.mockResolvedValueOnce(first.value).mockImplementationOnce(() => {
      expect(first.track.stop).toHaveBeenCalledOnce();
      return Promise.resolve(second.value);
    });
    await preview.start();
    await preview.start("second");
    expect(media.getUserMedia.mock.calls[1][0].video.deviceId).toEqual({ exact: "second" });
    expect(media.getUserMedia.mock.calls[1][0]).toEqual({ audio: false, video: { deviceId: { exact: "second" } } });
    expect(preview.state.stream).toBe(second.value);
    preview.close();
  });

  it("stops a late permission reply after leaving the page", async () => {
    const { preview, media, changed } = setup();
    const first = stream("first");
    let finish!: (stream: MediaStream) => void;
    media.getUserMedia.mockReturnValue(
      new Promise<MediaStream>((done) => {
        finish = done;
      }),
    );
    const pending = preview.start();
    await Promise.resolve();
    preview.close();
    changed.mockClear();
    finish(first.value);
    await pending;
    expect(first.track.stop).toHaveBeenCalledOnce();
    expect(changed).not.toHaveBeenCalled();
    expect(preview.state.stream).toBeNull();
  });

  it("serializes opening and releases a superseded permission reply before opening the selected camera", async () => {
    const { preview, media } = setup();
    const first = stream("first"),
      second = stream("second");
    let finish!: (stream: MediaStream) => void;
    media.getUserMedia
      .mockReturnValueOnce(
        new Promise<MediaStream>((done) => {
          finish = done;
        }),
      )
      .mockResolvedValueOnce(second.value);
    const pending = preview.start("first");
    await Promise.resolve();
    const selected = preview.start("second");
    expect(media.getUserMedia).toHaveBeenCalledOnce();
    finish(first.value);
    await Promise.all([pending, selected]);
    expect(first.track.stop).toHaveBeenCalledOnce();
    expect(preview.state.stream).toBe(second.value);
    preview.close();
  });

  it("does not open a camera for an immediately disposed page (React StrictMode)", async () => {
    const { preview, media } = setup();
    const pending = preview.start();
    preview.close();
    await pending;
    expect(media.getUserMedia).not.toHaveBeenCalled();
  });

  it("falls back once when the remembered camera is missing", async () => {
    const { preview, media } = setup();
    const first = stream("first");
    media.getUserMedia.mockRejectedValueOnce({ name: "OverconstrainedError" }).mockResolvedValueOnce(first.value);
    await preview.start("unplugged");
    expect(media.getUserMedia).toHaveBeenCalledTimes(2);
    expect(media.getUserMedia.mock.calls[1][0].video.deviceId).toBeUndefined();
    expect(preview.state.status).toBe("live");
    preview.close();
  });

  it.each(["NotAllowedError", "NotReadableError", "NotFoundError"])(
    "reports %s without an unhandled rejection or repeated prompts",
    async (name) => {
      const { preview, media } = setup();
      media.getUserMedia.mockRejectedValue({ name });
      await preview.start();
      expect(preview.state.status).toBe("error");
      expect(preview.state.error).toBe(cameraError({ name }));
      expect(media.getUserMedia).toHaveBeenCalledOnce();
      expect(preview.state.stream).toBeNull();
      preview.close();
    },
  );

  it("retains a usable preview if discovery fails", async () => {
    const { preview, media } = setup();
    media.getUserMedia.mockResolvedValue(stream("first").value);
    media.enumerateDevices.mockRejectedValue(new Error("discovery unavailable"));
    await preview.start();
    expect(preview.state.status).toBe("live");
    preview.close();
  });

  it("releases a disconnected stream and presents a recoverable error", async () => {
    const { preview, media } = setup();
    const first = stream("first");
    media.getUserMedia.mockResolvedValue(first.value);
    await preview.start();
    first.track.dispatchEvent(new Event("ended"));
    expect(preview.state.stream).toBeNull();
    expect(preview.state.status).toBe("error");
    expect(first.track.stop).toHaveBeenCalledOnce();
    preview.close();
  });

  it("updates devices on hotplug and removes the listener when closed", async () => {
    const { preview, media } = setup();
    const remove = vi.spyOn(media, "removeEventListener");
    media.enumerateDevices.mockResolvedValue([{ kind: "videoinput", deviceId: "usb", label: "USB camera" }]);
    media.dispatchEvent(new Event("devicechange"));
    await vi.waitFor(() => expect(preview.state.devices).toEqual([{ id: "usb", name: "USB camera" }]));
    preview.close();
    expect(remove).toHaveBeenCalledWith("devicechange", expect.any(Function));
  });

  it("keeps a selected device in the existing preferences and validates its type", () => {
    expect(resolve({ island: { cameraDeviceId: "usb-camera" } }).island.cameraDeviceId).toBe("usb-camera");
    expect(resolve({ island: { cameraDeviceId: 42 } } as never).island.cameraDeviceId).toBe("");
  });
});
