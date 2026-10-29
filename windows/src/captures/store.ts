// SPDX-License-Identifier: GPL-3.0-or-later
import { create } from "zustand";
import { bridge } from "../shared/bridge";
import type { RecentCapture } from "./types";

interface State {
  entries: RecentCapture[];
  loading: boolean;
  busy: boolean;
  error: string | null;
  confirmingClear: boolean;
  refresh(): Promise<void>;
  perform(action: "open" | "remove" | "clear", id?: string): Promise<void>;
}

let request = 0;
export const useCaptures = create<State>((set, get) => ({
  entries: [],
  loading: true,
  busy: false,
  error: null,
  confirmingClear: false,
  async refresh() {
    const at = ++request;
    try {
      const entries = await bridge.capture.list();
      if (at === request) set({ entries: entries ?? [], loading: false });
    } catch {
      if (at === request) set({ loading: false, error: "Could not load recent captures. Try again." });
    }
  },
  async perform(action, id) {
    if (get().busy) return;
    set({ busy: true, error: null, confirmingClear: false });
    try {
      if (action === "clear") await bridge.capture.clear();
      else if (id) await bridge.capture[action](id);
    } catch {
      set({
        error:
          action === "open"
            ? "Could not open this capture. It may be missing or have no associated application."
            : "Could not remove all selected captures. Try again.",
      });
    } finally {
      await get().refresh();
      set({ busy: false });
    }
  },
}));

/** Listen before loading; refresh requests discard older responses. */
export function subscribeCaptures(): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  void bridge.capture
    .onChanged(() => {
      if (!disposed) void useCaptures.getState().refresh();
    })
    .then((stop) => {
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
      void useCaptures.getState().refresh();
    })
    .catch(() => {
      if (!disposed) {
        void useCaptures.getState().refresh();
        useCaptures.setState({ error: "Live updates are unavailable. Reopen Captures to refresh." });
      }
    });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
