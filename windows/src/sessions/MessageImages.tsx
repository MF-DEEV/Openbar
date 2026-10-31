// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t, tf } from "../i18n/index";

import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";

export function ImagePreview({ image, label, onClose }: { image: string; label: string; onClose(): void }) {
  useLocale();
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const [closing, setClosing] = useState(false);
  const [dimensions, setDimensions] = useState<{ width: number; height: number }>();
  const requestClose = useRef(() => setClosing(true)).current;
  useEffect(() => {
    if (!closing) return;
    const timer = window.setTimeout(onClose, window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? 0 : 180);
    return () => window.clearTimeout(timer);
  }, [closing, onClose]);
  useEffect(() => {
    const previousFocus = document.activeElement;
    const popup = dialog.current;
    const wasPinned = useIsland.getState().pinned;
    useIsland.getState().setPinned(true);
    void bridge.setCapture(true);
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      requestClose();
    };
    window.addEventListener("keydown", key, true);
    popup?.showModal();
    closeButton.current?.focus();
    return () => {
      window.removeEventListener("keydown", key, true);
      popup?.close();
      useIsland.getState().setPinned(wasPinned);
      void bridge.setCapture(false);
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
    };
  }, [requestClose]);
  return (
    <dialog
      ref={dialog}
      className={`s-image-preview${closing ? " is-closing" : ""}`}
      aria-label={label}
      onCancel={(event) => {
        event.preventDefault();
        requestClose();
      }}
      onClick={(event) => {
        event.stopPropagation();
        const rect = event.currentTarget.getBoundingClientRect();
        if (
          event.target === event.currentTarget &&
          (event.clientX < rect.left ||
            event.clientX > rect.right ||
            event.clientY < rect.top ||
            event.clientY > rect.bottom)
        )
          requestClose();
      }}
    >
      <button
        ref={closeButton}
        type="button"
        className="s-image-preview-close"
        aria-label={t("windows.sessions.closeImagePreview")}
        onClick={requestClose}
      >
        <X size={18} />
      </button>
      <div className="s-image-preview-viewport">
        <img
          src={image}
          alt={label}
          onLoad={(event) => {
            const { naturalWidth: width, naturalHeight: height } = event.currentTarget;
            setDimensions({ width, height });
          }}
        />
      </div>
      {dimensions ? (
        <span className="s-image-preview-dimensions">
          {dimensions.width} × {dimensions.height}
        </span>
      ) : null}
    </dialog>
  );
}

function Thumbnail({ sessionId, source, index }: { sessionId: string; source: string; index: number }) {
  useLocale();
  const [image, setImage] = useState<string>();
  const [failed, setFailed] = useState(false);
  const [preview, setPreview] = useState(false);
  const closePreview = useRef(() => setPreview(false)).current;
  useEffect(() => {
    let active = true;
    setImage(undefined);
    setFailed(false);
    void invoke<string>("sessions_image", { sessionId, source }).then(
      (value) => {
        if (active) setImage(value);
      },
      () => {
        if (active) setFailed(true);
      },
    );
    return () => {
      active = false;
    };
  }, [sessionId, source]);
  return (
    <span className="s-image-attachment">
      <button
        type="button"
        className="s-image-thumbnail"
        disabled={!image}
        aria-label={tf("windows.sessions.openImageValue", index + 1)}
        title={tf("windows.sessions.openImageValue", index + 1)}
        onClick={(event) => {
          event.stopPropagation();
          setPreview(true);
        }}
      >
        {image ? (
          <img
            src={image}
            alt={tf("windows.sessions.attachmentValue", index + 1)}
            onError={() => {
              setImage(undefined);
              setFailed(true);
            }}
          />
        ) : (
          <span>{failed ? t("notchClaudeSessions.imageUnavailable") : t("windows.sessions.loadingImage")}</span>
        )}
      </button>
      {preview && image
        ? createPortal(
            <ImagePreview
              image={image}
              label={tf("notchClaudeSessions.imageFormat", index + 1)}
              onClose={closePreview}
            />,
            document.body,
          )
        : null}
    </span>
  );
}

export function MessageImages({
  text,
  images = [],
  sessionId,
}: {
  text: string;
  images?: string[];
  sessionId: string;
}) {
  useLocale();
  // Preserve labels when an attachment is missing instead of silently removing content.
  const clean = text
    .replace(/\[Image\s*#(\d+)\]/gi, (label, number: string) => (images[Number(number) - 1] ? "" : label))
    .trim();
  return (
    <>
      {clean ? <span className="s-attachment-text">{clean}</span> : null}
      {images.length ? (
        <span className="s-image-attachments">
          {images.map((source, index) => (
            <Thumbnail key={`${index}:${source}`} sessionId={sessionId} source={source} index={index} />
          ))}
        </span>
      ) : null}
    </>
  );
}
