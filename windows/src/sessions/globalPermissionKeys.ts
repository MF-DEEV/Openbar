// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { permissionKeyTarget } from "./permissionKeys";
import { settled, useSessions } from "./store";

// Serialize native registrations across cards, including React StrictMode remounts.
let queue = Promise.resolve();
const editorSelector = "input, textarea, [contenteditable]:not([contenteditable='false'])";

/** Borrow Ctrl+Y/N only while this approval is displayed, without taking app focus. */
export function subscribeGlobalPermissionKeys(sessionId: string, requestId: string): () => void {
  let disposed = false;
  let desired = false;
  let sending = false;
  let hidden = false;
  const held = new Set<string>();
  const registered = new Set<"y" | "n">();
  const hasEditor = () => document.hasFocus() && !!document.activeElement?.closest(editorSelector);
  const eligible = () =>
    !disposed &&
    !hidden &&
    !!permissionKeyTarget(sessionId, requestId) &&
    !hasEditor() &&
    !document.querySelector("dialog[open]");

  const sync = () => {
    const next = eligible();
    if (next === desired) return;
    desired = next;
    queue = queue
      .then(async () => {
        for (const key of ["y", "n"] as const) {
          if (desired && !disposed && !registered.has(key)) {
            try {
              await bridge.sessions.registerPermissionKey(key, (event) => {
                if (event.state === "Released") {
                  held.delete(key);
                  return;
                }
                if (held.has(key)) return;
                held.add(key);
                if (!eligible() || sending) return;
                const session = permissionKeyTarget(sessionId, requestId);
                const state = useSessions.getState();
                if (!session || !settled(state, session)) return;
                sending = true;
                void state.decide(session, key === "y" ? "allow" : "deny").finally(() => {
                  sending = false;
                });
              });
              registered.add(key);
            } catch (error) {
              console.warn(`Could not register Ctrl+${key.toUpperCase()}`, error);
            }
          } else if ((!desired || disposed) && registered.has(key)) {
            await bridge.sessions.unregisterPermissionKey(key);
            registered.delete(key);
            held.delete(key);
          }
        }
      })
      .catch((error) => console.warn("Could not update permission shortcuts", error));
  };

  const island = useIsland.subscribe(sync);
  const sessions = useSessions.subscribe(sync);
  document.addEventListener("focusin", sync);
  document.addEventListener("focusout", sync);
  window.addEventListener("focus", sync);
  window.addEventListener("blur", sync);
  const fullscreen = bridge.onFullscreen((value) => {
    hidden = value;
    sync();
  });
  sync();
  return () => {
    disposed = true;
    sync();
    island();
    sessions();
    document.removeEventListener("focusin", sync);
    document.removeEventListener("focusout", sync);
    window.removeEventListener("focus", sync);
    window.removeEventListener("blur", sync);
    void fullscreen.then((unlisten) => unlisten());
  };
}
