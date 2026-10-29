// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Grip, Move, Pin, QrCode, Save, Share2, TextSelect, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState, type PointerEvent } from "react";
import { inTauri } from "../shared/bridge";
import { usePageLayer } from "../island/pageLayer";
import { useIsland } from "../island/store";
import { PreviewCountdown } from "./countdown";
import "./preview.css";

export interface Info {
  image: string;
  saved: string | null;
  copied: boolean;
  duration: number;
  dismissOnBlur: boolean;
  pinned: boolean;
}

export function Preview({
  id,
  embedded = false,
  initialInfo,
  onClosed,
}: {
  id?: string;
  embedded?: boolean;
  initialInfo?: Info;
  onClosed?(): void;
}) {
  const native = inTauri ? getCurrentWindow() : null;
  const request = <T,>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    if (inTauri) return invoke<T>(command, { id, ...args });
    if (command === "preview_ready" || command === "preview_close") return Promise.resolve(undefined as T);
    return Promise.reject("This action is available in the Windows app.");
  };
  const [info, setInfo] = useState<Info | null>(initialInfo ?? null);
  const [pinned, setPinned] = useState(initialInfo?.pinned ?? false);
  const [hovered, setHovered] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState(false);
  const [qr, setQr] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const state = useRef({ pinned, busy });
  state.current = { pinned, busy };
  const countdown = useRef<PreviewCountdown | null>(null);
  const dragStart = useRef<{ x: number; y: number } | null>(null);

  const report = (e: unknown) => {
    setError(true);
    setStatus(String(e));
  };
  const close = () =>
    request("preview_close")
      .then(() => onClosed?.())
      .catch(report);
  usePageLayer(embedded, () => {
    if (qr !== null) setQr(null);
    else if (!state.current.busy) void close();
  });

  useEffect(() => {
    if (!embedded || useIsland.getState().capturePreview !== id) return;
    useIsland.getState().setHeld(busy);
    return () => {
      if (useIsland.getState().capturePreview === id) useIsland.getState().setHeld(false);
    };
  }, [busy, embedded, id]);

  useEffect(() => {
    let disposed = false;
    if (initialInfo) return;
    request<Info>("preview_info")
      .then((value) => {
        if (!disposed) {
          setInfo(value);
          setPinned(value.pinned);
        }
      })
      .catch(report);
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    if (!info || !ready) return;
    const timer = new PreviewCountdown(info.duration, () => {
      void close();
    });
    countdown.current = timer;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let hadFocus = false;
    native
      ?.onFocusChanged(({ payload: focused }) => {
        if (disposed) return;
        if (focused) hadFocus = true;
        else if (hadFocus && info.dismissOnBlur && !state.current.pinned && !state.current.busy) void close();
      })
      .then((off) => {
        if (disposed) off();
        else unlisten = off;
      })
      .catch(report);
    return () => {
      disposed = true;
      timer.dispose();
      countdown.current = null;
      unlisten?.();
    };
  }, [info, ready]);

  useEffect(() => {
    countdown.current?.update(hovered || busy || qr !== null, pinned);
  }, [hovered, busy, pinned, qr, ready]);

  const togglePin = async () => {
    if (state.current.busy) return;
    const next = embedded || !state.current.pinned;
    state.current.busy = true;
    setBusy(true);
    countdown.current?.update(true, state.current.pinned);
    try {
      await request("preview_pin", { pinned: next });
      state.current.pinned = next;
      setPinned(next);
      if (embedded) onClosed?.();
    } catch (e) {
      report(e);
    } finally {
      state.current.busy = false;
      setBusy(false);
    }
  };

  const action = async (name: string) => {
    if (state.current.busy) return;
    state.current.busy = true;
    setBusy(true);
    countdown.current?.update(true, state.current.pinned);
    setError(false);
    setStatus(name === "qr" ? "Looking for QR codes…" : name === "text" ? "Reading text…" : "");
    try {
      if (name === "share") {
        state.current.pinned = true;
        setPinned(true);
      }
      const result = await request<string>("preview_action", { action: name });
      if (name === "discard") {
        await close();
        return;
      }
      if (name === "qr") {
        setQr(result);
        setStatus("QR code detected");
      } else setStatus(result);
    } catch (e) {
      report(e);
    } finally {
      state.current.busy = false;
      setBusy(false);
    }
  };

  const pointerDown = (event: PointerEvent) => {
    if (event.button === 0 && !state.current.busy) dragStart.current = { x: event.clientX, y: event.clientY };
  };
  const pointerMove = async (event: PointerEvent) => {
    const start = dragStart.current;
    if (!start || !(event.buttons & 1) || state.current.busy) return;
    if (Math.hypot(event.clientX - start.x, event.clientY - start.y) < 5) return;
    dragStart.current = null;
    state.current.busy = true;
    setBusy(true);
    countdown.current?.update(true, state.current.pinned);
    try {
      const accepted = await request<boolean>("preview_drag");
      setError(false);
      setStatus(accepted ? "Screenshot dropped" : "Drag cancelled or destination did not accept the image");
    } catch (e) {
      report(e);
    } finally {
      state.current.busy = false;
      setBusy(false);
      // Native OLE takes the mouse, so a DOM pointerleave may never arrive.
      setHovered(false);
    }
  };

  return (
    <main
      className={`shot-preview${embedded ? " shot-preview--embedded" : ""}`}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => {
        setHovered(false);
      }}
      onKeyDown={(event) => {
        if (busy) return;
        if (event.key === "Escape") {
          event.stopPropagation();
          if (qr !== null) setQr(null);
          else void close();
        }
        if (event.ctrlKey && event.key.toLowerCase() === "c" && qr === null) {
          event.preventDefault();
          event.stopPropagation();
          void action("copy");
        }
        if (event.ctrlKey && event.key.toLowerCase() === "s") {
          event.preventDefault();
          event.stopPropagation();
          void action("save");
        }
      }}
    >
      {!embedded && (
        <header>
          <button
            className="move"
            title="Move preview"
            aria-label="Move preview"
            onPointerDown={(e) => {
              if (e.button === 0 && !busy) void native?.startDragging().catch(report);
            }}
          >
            <Move size={14} />
            <span>{pinned ? "Pinned screenshot" : "Screenshot"}</span>
          </button>
          <button
            title="Close preview (keeps saved files)"
            aria-label="Close preview"
            disabled={busy}
            onClick={() => void close()}
          >
            <X size={16} />
          </button>
        </header>
      )}
      {info && (
        <div className="shot-canvas">
          <div
            className="shot-picture"
            onPointerDown={pointerDown}
            onPointerMove={(e) => void pointerMove(e)}
            onPointerUp={() => {
              dragStart.current = null;
            }}
          >
            <img
              src={info.image}
              alt="Captured screenshot. Drag to another application."
              draggable={false}
              onLoad={() => {
                request("preview_ready")
                  .then(() => setReady(true))
                  .catch(report);
              }}
            />
          </div>
        </div>
      )}
      <nav className="shot-toolbar" aria-label="Screenshot actions">
        <button
          title="Discard screenshot (move automatic save to Recycle Bin)"
          aria-label="Discard screenshot"
          disabled={busy}
          onClick={() => void action("discard")}
        >
          <Trash2 size={17} />
        </button>
        <button
          title="Detect QR codes"
          aria-label="Detect QR codes"
          disabled={busy || !ready}
          onClick={() => void action("qr")}
        >
          <QrCode size={17} />
        </button>
        <button
          title="Copy text (OCR)"
          aria-label="Copy text"
          disabled={busy || !ready}
          onClick={() => void action("text")}
        >
          <TextSelect size={17} />
        </button>
        <button
          title="Save as… (Ctrl+S)"
          aria-label="Save as"
          disabled={busy || !ready}
          onClick={() => void action("save")}
        >
          <Save size={17} />
        </button>
        <button
          title="Copy image (Ctrl+C)"
          aria-label="Copy image"
          disabled={busy || !ready}
          onClick={() => void action("copy")}
        >
          <Copy size={17} />
        </button>
        <button
          title="Windows Share (pins preview)"
          aria-label="Share"
          disabled={busy || !ready}
          onClick={() => void action("share")}
        >
          <Share2 size={17} />
        </button>
        <button
          title={!embedded && pinned ? "Unpin" : "Pin on screen"}
          aria-label={!embedded && pinned ? "Unpin" : "Pin on screen"}
          aria-pressed={!embedded && pinned}
          disabled={busy}
          onClick={() => void togglePin()}
        >
          <Pin size={17} />
        </button>
        <button
          title="Drag screenshot into another application"
          aria-label="Drag screenshot"
          disabled={busy || !ready}
          onPointerDown={pointerDown}
          onPointerMove={(e) => void pointerMove(e)}
          onPointerUp={() => {
            dragStart.current = null;
          }}
        >
          <Grip size={17} />
        </button>
        {embedded && (
          <button
            title="Close preview (keeps saved files)"
            aria-label="Close preview"
            disabled={busy}
            onClick={() => void close()}
          >
            <X size={17} />
          </button>
        )}
      </nav>
      <p
        className={`shot-status${error ? " error" : ""}${embedded && !status ? " is-quiet" : ""}`}
        role="status"
        title={status || info?.saved || undefined}
      >
        {status ||
          (info
            ? `${info.saved ? "Saved" : "Automatic save failed"} · ${info.copied ? "Copied" : "Clipboard copy failed"}`
            : "Loading screenshot…")}
      </p>
      {qr !== null && (
        <section className="shot-qr-result" aria-label="QR code contents">
          <header>
            <strong>QR code contents</strong>
            <button aria-label="Close QR result" onClick={() => setQr(null)}>
              <X size={16} />
            </button>
          </header>
          <textarea readOnly value={qr} aria-label="Detected QR contents" />
          <button
            onClick={() => {
              invoke("tools_copy_text", { text: qr })
                .then(() => {
                  setError(false);
                  setStatus("QR contents copied");
                })
                .catch(report);
            }}
          >
            Copy contents
          </button>
        </section>
      )}
    </main>
  );
}
