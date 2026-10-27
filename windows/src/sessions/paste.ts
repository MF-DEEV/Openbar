// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

export const IMAGE_LIMIT = 5;
export const IMAGE_BYTES = 20 * 1024 * 1024;

export function imageFiles(data: Pick<DataTransfer, "files" | "items">): File[] {
  const files = Array.from(data.files).filter((file) => file.type.startsWith("image/"));
  if (files.length) return files;
  return Array.from(data.items)
    .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
    .map((item) => item.getAsFile())
    .filter((file): file is File => file !== null);
}

export function isImagePasteShortcut(
  event: Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey" | "repeat" | "isComposing">,
) {
  return (
    event.key.toLowerCase() === "v" &&
    event.altKey &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.shiftKey &&
    !event.repeat &&
    !event.isComposing
  );
}

export function imageDataUrl(file: File): Promise<string> {
  if (file.size > IMAGE_BYTES) return Promise.reject(new Error("Image must be smaller than 20 MB"));
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("Cannot read image"));
    reader.onerror = () => reject(new Error("Cannot read image"));
    reader.readAsDataURL(file);
  });
}
