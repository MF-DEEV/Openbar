// SPDX-License-Identifier: GPL-3.0-or-later
export interface RecentCapture {
  id: string;
  /** Unix milliseconds. */
  createdAt: number;
  thumbnail: string | null;
}
