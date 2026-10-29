// SPDX-License-Identifier: GPL-3.0-or-later
import { beforeEach, describe, expect, it, vi } from "vitest";
vi.mock("../shared/bridge", () => ({
  bridge: {
    capture: {
      list: vi.fn(),
      open: vi.fn(),
      remove: vi.fn(),
      clear: vi.fn(),
      onChanged: vi.fn(),
    },
  },
}));
import { bridge } from "../shared/bridge";
import { subscribeCaptures, useCaptures } from "./store";

const first = { id: "first", createdAt: 1234, thumbnail: null };
const second = { ...first, id: "second", createdAt: 5678 };
beforeEach(() => {
  vi.resetAllMocks();
  useCaptures.setState({ entries: [], loading: true, busy: false, error: null, confirmingClear: false });
  vi.mocked(bridge.capture.list).mockResolvedValue([]);
});

describe("capture history", () => {
  it("discards stale list responses after a newer update", async () => {
    let finish!: (entries: (typeof first)[]) => void;
    vi.mocked(bridge.capture.list).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const older = useCaptures.getState().refresh();
    vi.mocked(bridge.capture.list).mockResolvedValue([second]);
    await useCaptures.getState().refresh();
    finish([first]);
    await older;
    expect(useCaptures.getState().entries).toEqual([second]);
  });

  it("refreshes on additions and unsubscribes when the page closes", async () => {
    let changed!: () => void;
    const stop = vi.fn();
    vi.mocked(bridge.capture.onChanged).mockImplementation(async (fn) => {
      changed = fn;
      return stop;
    });
    const dispose = subscribeCaptures();
    await vi.waitFor(() => expect(useCaptures.getState().loading).toBe(false));
    vi.mocked(bridge.capture.list).mockResolvedValue([first]);
    changed();
    await vi.waitFor(() => expect(useCaptures.getState().entries).toEqual([first]));
    dispose();
    expect(stop).toHaveBeenCalledOnce();
  });

  it("removes a capture and immediately reloads history", async () => {
    useCaptures.setState({ entries: [first] });
    await useCaptures.getState().perform("remove", first.id);
    expect(bridge.capture.remove).toHaveBeenCalledWith(first.id);
    expect(useCaptures.getState().entries).toEqual([]);
    expect(useCaptures.getState().busy).toBe(false);
  });

  it("clears all entries and closes confirmation", async () => {
    useCaptures.setState({ entries: [first], confirmingClear: true });
    await useCaptures.getState().perform("clear");
    expect(bridge.capture.clear).toHaveBeenCalledOnce();
    expect(useCaptures.getState().entries).toEqual([]);
    expect(useCaptures.getState().confirmingClear).toBe(false);
  });

  it("reports open failures and prunes missing captures on refresh", async () => {
    useCaptures.setState({ entries: [first] });
    vi.mocked(bridge.capture.open).mockRejectedValue(new Error("missing"));
    await useCaptures.getState().perform("open", first.id);
    expect(useCaptures.getState().error).toContain("Could not open");
    expect(useCaptures.getState().entries).toEqual([]);
  });

  it("preserves remaining entries and reports failed deletion", async () => {
    vi.mocked(bridge.capture.remove).mockRejectedValue(new Error("access denied"));
    vi.mocked(bridge.capture.list).mockResolvedValue([first]);
    await useCaptures.getState().perform("remove", first.id);
    expect(useCaptures.getState().error).toContain("Could not remove");
    expect(useCaptures.getState().entries).toEqual([first]);
  });

  it("handles history load failures without an unhandled rejection", async () => {
    vi.mocked(bridge.capture.list).mockRejectedValue(new Error("access denied"));
    await useCaptures.getState().refresh();
    expect(useCaptures.getState().error).toContain("Could not load");
    expect(useCaptures.getState().loading).toBe(false);
  });
});
