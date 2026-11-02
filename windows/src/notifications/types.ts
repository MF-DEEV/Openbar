// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

export type Access = "unavailable" | "notAsked" | "denied" | "allowed";

export interface Item {
  id: number;
  app: string;
  aumid: string;
  /** Epoch seconds. */
  time: number;
  title: string;
  /** Message text supplied by Windows; apps may redact or omit it. */
  body?: string;
  icon?: string;
}

export interface Inbox {
  access: Access;
  items: Item[];
  error?: string | null;
}
