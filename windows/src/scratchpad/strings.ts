// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// English copy for the Scratchpad section (ScratchpadStrings.swift, enUS).

export const S = {
  pageTitle: "Scratchpad",
  placeholder: "Type anything. It saves by itself.",
  copyAll: "Copy all",
  copied: "Copied",
  exportAction: "Save as file",
  exportFailed: "The file could not be saved",
  loadFailed: "Your notes could not be opened. They were left unchanged.",
  saveFailed: "Your notes could not be saved. Copy them elsewhere before quitting.",
  clearAction: "Clear",
  newPad: "New scratchpad",
  padActions: "Scratchpad actions",
  renamePad: "Rename scratchpad",
  closePad: "Close scratchpad",
  saveName: "Save",
  cancel: "Cancel",
  name: "Name",
  deletePad: (name: string) => `Delete “${name}” and everything in it?`,
  padLimit: (count: number) => `You can keep up to ${count} scratchpads`,
  previewFormatting: "Show formatting",
  editText: "Edit text",
  taskDone: "Done",
  taskNotDone: "Not done",
} as const;
