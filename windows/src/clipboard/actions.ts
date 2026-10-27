// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Copying what Vorssaint makes (a screenshot, OCR text, a picked color). The
// copy goes through the clipboard history's own writer, so it joins the
// history with no app named, as the Mac's declareVorssaintSource does.

import { bridge } from "../shared/bridge";

/** True once the text is on the clipboard. */
export async function copyText(text: string): Promise<boolean> {
  return (await bridge.clipboard.copyText(text)) ?? false;
}

/** A `data:image/png;base64,…` URL, written as CF_DIB and "PNG". True once on the clipboard. */
export async function copyImagePng(dataUrl: string): Promise<boolean> {
  return (await bridge.clipboard.copyImage(dataUrl)) ?? false;
}
