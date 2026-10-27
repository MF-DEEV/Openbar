// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A question asked inside the island (spec 05 §0, "confirmations above the
// island"): the island is topmost, so a system dialog without it as parent
// would open behind it. Covers the page, keeps the keyboard inside, and
// Escape cancels before anything else closes.

import { useEffect, useRef, useState } from "react";
import { usePageLayer } from "../island/pageLayer";

export function Sheet(props: {
  title: string;
  message?: string;
  confirm: string;
  cancel: string;
  destructive?: boolean;
  /** A text field (a rename): its value goes to `onConfirm`. */
  field?: { value: string; label: string; maxLength?: number };
  onConfirm(value: string): void;
  onCancel(): void;
}) {
  const [value, setValue] = useState(props.field?.value ?? "");
  const input = useRef<HTMLInputElement>(null);
  const confirmButton = useRef<HTMLButtonElement>(null);
  usePageLayer(true, props.onCancel);

  useEffect(() => {
    if (input.current) input.current.select();
    else confirmButton.current?.focus();
  }, []);

  const canConfirm = !props.field || value.trim().length > 0;
  const submit = () => {
    if (canConfirm) props.onConfirm(value);
  };

  return (
    <div className="tl-sheet-backdrop">
      <div
        className="tl-sheet"
        role="alertdialog"
        aria-modal="true"
        aria-label={props.title}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.nativeEvent.isComposing) {
            e.preventDefault();
            e.stopPropagation();
            submit();
          }
        }}
      >
        <div className="tl-sheet-title">{props.title}</div>
        {props.message ? <div className="tl-sheet-message">{props.message}</div> : null}
        {props.field ? (
          <input
            ref={input}
            className="tl-sheet-field"
            aria-label={props.field.label}
            value={value}
            maxLength={props.field.maxLength}
            spellCheck={false}
            onChange={(e) => setValue(e.target.value)}
          />
        ) : null}
        <div className="tl-sheet-buttons">
          <button type="button" className="tl-sheet-button" onClick={props.onCancel}>
            {props.cancel}
          </button>
          <button
            ref={confirmButton}
            type="button"
            className={`tl-sheet-button is-default${props.destructive ? " is-destructive" : ""}`}
            disabled={!canConfirm}
            onClick={submit}
          >
            {props.confirm}
          </button>
        </div>
      </div>
    </div>
  );
}
