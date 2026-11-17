// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkSettingsSelect({ send, evaluate, base }) {
  await send("Emulation.setDeviceMetricsOverride", { width: 1050, height: 760, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: `${base}settings.html#content` });
  for (let attempt = 0; attempt < 100; attempt++) {
    if (await evaluate(`document.querySelectorAll('.st-quick-action [role="combobox"]').length === 5`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  assert.equal(await evaluate(`document.querySelectorAll('select').length`), 0, "A native popup remains in settings.");
  await evaluate(
    `(() => {const preview=[...document.querySelectorAll('.is-editable [role="combobox"]')], rows=[...document.querySelectorAll('.st-quick-action [role="combobox"]')]; window.menuTriggers=[preview[0],preview[2],rows[0],rows[2]];})()`,
  );
  const frames = () => evaluate(`(async () => {for(let i=0;i<4;i++) await new Promise(requestAnimationFrame)})()`);
  const open = async (index) => {
    await evaluate(`(() => {const b=menuTriggers[${index}]; b.scrollIntoView({block:'center'}); b.focus();})()`);
    await frames();
    await evaluate(`menuTriggers[${index}].click()`);
    await frames();
    const result = await evaluate(
      `(() => {const m=document.querySelector('[role="listbox"]'), r=m?.getBoundingClientRect(); return {count: document.querySelectorAll('[role="listbox"]').length, options: m?.querySelectorAll('[role="option"]').length, fits: r && r.left>=0 && r.right<=innerWidth && r.top>=0 && r.bottom<=innerHeight, selected: m?.querySelectorAll('[aria-selected="true"]').length};})()`,
    );
    assert.equal(result.count, 1);
    assert.equal(result.options, 20);
    assert.equal(result.selected, 1);
    assert(result.fits, `Menu escapes viewport for trigger ${index}`);
  };
  const key = async (key, code = key) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key, code });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key, code });
    await frames();
  };

  // Both preview circles and both settings rows use the same popup.
  for (let index = 0; index < 4; index++) {
    await open(index);
    await key("w", "KeyW");
    assert.equal(await evaluate(`document.querySelector('.st-select-option.is-active')?.textContent.trim()`), "Watch");
    await key("Enter");
    assert.equal(await evaluate(`document.querySelectorAll('[role="listbox"]').length`), 0);
    assert(
      await evaluate(
        `(async () => {const {useSettings}=await import('/src/settings/state.ts'); const slot=${index % 2}; return useSettings.getState().prefs.island.quickAccess.filter(b=>b.side === (slot===0?'left':'right') && b.action.kind==='module').some(b=>b.action.module==='watch');})()`,
      ),
      "Selection was not saved.",
    );
  }

  await open(2);
  await key("Home");
  await key("ArrowDown");
  assert.equal(await evaluate(`document.querySelector('.st-select-option.is-active')?.textContent.trim()`), "Mixer");
  await key("Escape");
  assert.equal(await evaluate(`document.querySelectorAll('[role="listbox"]').length`), 0);
  assert(await evaluate(`document.activeElement?.getAttribute('role') === 'combobox'`));
  await open(2);
  await key("m", "KeyM");
  assert.equal(await evaluate(`document.querySelector('.st-select-option.is-active')?.textContent.trim()`), "Mixer");
  await key("Tab");
  assert.equal(await evaluate(`document.querySelectorAll('[role="listbox"]').length`), 0);
  await open(2);
  await evaluate(
    `document.querySelector('.st-page-title').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true}))`,
  );
  await frames();
  assert.equal(await evaluate(`document.querySelectorAll('[role="listbox"]').length`), 0);

  // Light theme, smaller viewport, a menu opening upwards, and motion enabled.
  await send("Emulation.setDeviceMetricsOverride", { width: 780, height: 520, deviceScaleFactor: 1.25, mobile: false });
  await send("Emulation.setEmulatedMedia", {
    features: [
      { name: "prefers-color-scheme", value: "light" },
      { name: "prefers-reduced-motion", value: "reduce" },
    ],
  });
  await open(3);
  await key("Escape");
  await send("Emulation.setDeviceMetricsOverride", { width: 1050, height: 760, deviceScaleFactor: 1, mobile: false });
  await send("Emulation.setEmulatedMedia", {
    features: [
      { name: "prefers-color-scheme", value: "dark" },
      { name: "prefers-reduced-motion", value: "no-preference" },
    ],
  });
  await open(2);
  await new Promise((r) => setTimeout(r, 300));
  assert(await evaluate(`!!document.querySelector('[role="listbox"]')`));
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(resolve("../build/settings-select.png"), Buffer.from(screenshot.data, "base64"));
  await key("Escape");
  await new Promise((r) => setTimeout(r, 300));
  assert.equal(
    await evaluate(`document.querySelectorAll('[role="listbox"]').length`),
    0,
    "Exit animation did not remove the menu.",
  );

  await evaluate(`document.querySelector('[title="Behavior"]').click()`);
  await frames();
  await evaluate(`document.querySelector('[role="combobox"]').focus()`);
  await key("ArrowDown");
  await key("End");
  await key("Enter");
  assert.equal(
    await evaluate(
      `(async () => (await import('/src/settings/state.ts')).useSettings.getState().prefs.island.opening)()`,
    ),
    "hiddenUntilHover",
  );
  console.log(
    "Settings menus passed: preview and row choices, persistence, keyboard, typeahead, outside click, viewport fit, light theme, animated exit and Behavior selector. Screenshot: build/settings-select.png",
  );
}
