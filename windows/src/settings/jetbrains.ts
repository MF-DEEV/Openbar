// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

/** A restart note waits on the IDE processes that ran at the change
 * (`until`); it is done once none of them runs any more (#329). */
export function restartDone(until: readonly number[] | undefined, running: readonly number[]): boolean {
  return until !== undefined && !until.some((pid) => running.includes(pid));
}
