// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkIslandShortcut({ send, evaluate, base }) {
  await send("Emulation.setDeviceMetricsOverride", { width: 860, height: 620, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: `${base}settings.html#behavior` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.st-shortcut')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async()=>{
    window.settings=(await import('/src/settings/state.ts')).useSettings;
    window.prefs=await import('/src/shared/prefs.ts');
    window.shortcutButton=()=>document.querySelector('.st-shortcut');
    window.shortcutToggle=()=>document.querySelector('input[aria-label="Global shortcut"]');
  })()`);
  const settle = () => new Promise((r) => setTimeout(r, 80));
  const saved = () => evaluate(`settings.getState().prefs.island.shortcut`);
  const start = async () => {
    await evaluate(`shortcutButton().click()`);
    await settle();
  };
  const key = async (code, key, modifiers = 0, release = true) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", code, key, modifiers });
    if (release) await send("Input.dispatchKeyEvent", { type: "keyUp", code, key, modifiers });
    await settle();
  };
  assert.equal(await saved(), "Ctrl+Alt+Space");
  assert.equal(await evaluate(`shortcutButton().textContent`), "Change shortcut");
  assert.equal(await evaluate(`document.querySelector('.st-keys').textContent`), "CtrlAltSpace");
  await start();
  assert.equal(await evaluate(`shortcutButton().getAttribute('aria-pressed')`), "true");
  await key("KeyX", "X", 10, false);
  assert.equal(await saved(), "Ctrl+Alt+Space", "A held key must not be saved or globally reactivated");
  await send("Input.dispatchKeyEvent", { type: "keyUp", code: "KeyX", key: "X", modifiers: 10 });
  await settle();
  assert.equal(await saved(), "Ctrl+Shift+KeyX");
  assert.equal(await evaluate(`document.querySelector('.st-keys').textContent`), "CtrlShiftX");
  assert.equal(await evaluate(`prefs.loadPrefs().then(p=>p.island.shortcut)`), "Ctrl+Shift+KeyX");
  await start();
  await key("Escape", "Escape");
  assert.equal(await saved(), "Ctrl+Shift+KeyX");
  assert.equal(await evaluate(`shortcutButton().getAttribute('aria-pressed')`), "false");
  await start();
  await key("KeyQ", "q");
  assert.equal(await saved(), "Ctrl+Shift+KeyX");
  assert.equal(await evaluate(`shortcutButton().getAttribute('aria-pressed')`), "true");
  assert.match(await evaluate(`document.querySelector('[role=status]').textContent`), /Use Ctrl or Alt/);
  await key("Escape", "Escape");
  await start();
  await evaluate(`window.dispatchEvent(new Event('blur'))`);
  await settle();
  assert.equal(await evaluate(`shortcutButton().getAttribute('aria-pressed')`), "false");
  // A backend refusal must remain visible without showing the candidate as saved.
  await evaluate(
    `window.realSave=settings.getState().shortcut; settings.setState({shortcut:async()=>{throw new Error('Shortcut conflict test')}})`,
  );
  await start();
  await key("KeyZ", "Z", 10);
  assert.equal(await saved(), "Ctrl+Shift+KeyX");
  assert.match(await evaluate(`document.querySelector('[role=status]').textContent`), /Shortcut conflict test/);
  await evaluate(`settings.setState({shortcut:window.realSave})`);
  await settle();
  await evaluate(`shortcutToggle().click()`);
  await settle();
  assert.equal(await evaluate(`settings.getState().prefs.island.shortcutEnabled`), false);
  assert.equal(await evaluate(`shortcutButton().disabled`), true);
  await evaluate(`shortcutToggle().click()`);
  await settle();
  assert.equal(await evaluate(`settings.getState().prefs.island.shortcutEnabled`), true);
  assert.equal(await saved(), "Ctrl+Shift+KeyX");
  await start();
  await evaluate(`document.querySelector('[title="Content"]').click()`);
  await settle();
  await key("KeyA", "A", 10);
  assert.equal(await saved(), "Ctrl+Shift+KeyX", "Unmount must remove keyboard listeners");
  await evaluate(`document.querySelector('[title="Behavior"]').click()`);
  await settle();
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  const artifact = resolve("../build/island-shortcut-settings.png");
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(artifact, Buffer.from(screenshot.data, "base64"));
  console.log(
    `Island shortcut browser checks passed: recording, key release, display, save, Escape, invalid input, blur, conflict, disable/re-enable, listener cleanup. Screenshot: ${artifact}`,
  );
}
