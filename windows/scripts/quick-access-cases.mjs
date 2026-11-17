// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkQuickAccessEditor({ send, evaluate, base }) {
  await send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 950, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: `${base}settings.html#content` });
  for (let n = 0; n < 100; n++) {
    if (await evaluate(`document.querySelectorAll('.st-quick-row').length===5`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(
    `(async () => {window.settings=(await import('/src/settings/state.ts')).useSettings; window.initialButtons=structuredClone(settings.getState().prefs.island.quickAccess)})()`,
  );
  const frames = () => evaluate(`(async()=>{for(let i=0;i<5;i++)await new Promise(requestAnimationFrame)})()`);
  const select = async (selector, text) => {
    await evaluate(
      `(()=>{const b=document.querySelector(${JSON.stringify(selector)});if(!b)throw Error('Missing trigger '+${JSON.stringify(selector)});b.scrollIntoView({block:'center'});b.focus()})()`,
    );
    await frames();
    await evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
    await frames();
    await evaluate(
      `(()=>{const o=[...document.querySelectorAll('[role="option"]')].find(e=>e.textContent.trim()===${JSON.stringify(text)});if(!o)throw Error('Missing option '+${JSON.stringify(text)});o.click()})()`,
    );
    await frames();
  };
  const read = () => evaluate(`structuredClone(settings.getState().prefs.island.quickAccess)`);
  await select(".st-quick-add button", "Sessions");
  let buttons = await read();
  assert.equal(buttons.length, 6);
  const added = buttons.at(-1);
  assert.equal(added.side, "left");
  assert.equal(added.action.module, "sessions");
  const row = `[data-quick-id="${added.id}"]`;
  await select(`${row} .st-quick-side button`, "Bottom");
  buttons = await read();
  assert.equal(buttons.find((b) => b.id === added.id).side, "bottom");
  await evaluate(`document.querySelector('${row} .st-quick-row-actions button').click()`);
  await frames();
  buttons = await read();
  assert.equal(buttons.filter((b) => b.side === "bottom")[0].id, added.id);
  await select(`${row} .st-quick-action button`, "Pin");
  buttons = await read();
  assert.equal(buttons.find((b) => b.id === added.id).action.kind, "pin");
  await evaluate(`document.querySelector('${row} .st-quick-remove').click()`);
  await frames();
  assert.equal((await read()).length, 5);
  for (const action of ["Sessions", "Downloads", "Notifications", "Pin"]) await select(".st-quick-add button", action);
  buttons = await read();
  assert.equal(buttons.length, 9);
  for (const side of ["left", "right", "bottom"]) assert.equal(buttons.filter((b) => b.side === side).length, 3);
  assert(await evaluate(`document.querySelector('.st-quick-add button').disabled`));
  assert.equal(await evaluate(`document.querySelectorAll('.is-editable [role="combobox"]').length`), 9);
  await evaluate(`document.querySelector('.st-quick-preview-card').scrollIntoView({block:'start'})`);
  await frames();
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(resolve("../build/quick-access-editor.png"), Buffer.from(screenshot.data, "base64"));
  await send("Emulation.setDeviceMetricsOverride", { width: 780, height: 600, deviceScaleFactor: 1.25, mobile: false });
  await frames();
  assert(
    await evaluate(
      `(()=>{const stage=document.querySelector('.st-quick-preview').getBoundingClientRect();return [...document.querySelectorAll('.st-quick-preview-circle')].every(e=>{const r=e.getBoundingClientRect();return r.left>=stage.left-1&&r.right<=stage.right+1&&r.top>=stage.top-1&&r.bottom<=stage.bottom+1})&&[...document.querySelectorAll('.st-quick-row-controls')].every(e=>e.getBoundingClientRect().right<=innerWidth)})()`,
    ),
    "Editor does not fit a small settings window",
  );
  await send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 950, deviceScaleFactor: 1, mobile: false });
  // Carry the edited configuration to the island and check all nine controls and hit areas.
  await send("Page.navigate", { url: `${base}island.html?sessions` });
  await new Promise((r) => setTimeout(r, 500));
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    const {bridge}=await import('/src/shared/bridge.ts');
    bridge.setHitRects=async(rects)=>{window.lastHitRects=rects};
    island.setState(s=>({expanded:true,pinned:true,selected:'sessions',settings:{...s.settings,quickAccess:${JSON.stringify(buttons)},layout:'custom',customWidth:440,customHeight:300}}));
  })()`);
  await frames();
  assert.equal(await evaluate(`document.querySelectorAll('.n-bubble').length`), 9);
  const removed = buttons.find((b) => b.side === "right");
  await evaluate(
    `island.setState(s=>({settings:{...s.settings,quickAccess:s.settings.quickAccess.filter(b=>b.id!==${JSON.stringify(removed.id)})}}))`,
  );
  await frames();
  const hitBefore = await evaluate(`JSON.stringify(lastHitRects)`);
  const moved = buttons.find((b) => b.side === "left" && b.action.kind === "module");
  await evaluate(
    `island.setState(s=>({settings:{...s.settings,quickAccess:s.settings.quickAccess.map(b=>b.id===${JSON.stringify(moved.id)}?{...b,side:'right'}:b)}}))`,
  );
  await frames();
  assert.notEqual(
    await evaluate(`JSON.stringify(lastHitRects)`),
    hitBefore,
    "Hit areas did not follow relocated circles",
  );
  assert.equal(await evaluate(`lastHitRects.length`), 9);
  await evaluate(`document.querySelector('.n-bubble[aria-label="Downloads"]').click()`);
  await frames();
  assert.equal(await evaluate(`island.getState().selected`), "downloads");
  await evaluate(`document.querySelector('.n-bubble[aria-label="Unpin"]').click()`);
  await frames();
  assert.equal(await evaluate(`island.getState().pinned`), false);
  console.log(
    "Quick access editor passed: add, change action, move side, reorder, remove, nine-circle capacity, island actions and relocated hit areas. Screenshot: build/quick-access-editor.png",
  );
}
