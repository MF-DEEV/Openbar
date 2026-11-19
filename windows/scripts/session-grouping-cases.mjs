// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

// Press each grouping choice with a real pointer so :active styles are applied.
export async function checkSessionGrouping({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?sessions` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.s-strip, .n-activation')`)) break;
    await new Promise((done) => setTimeout(done, 100));
  }
  await evaluate(`(async()=>{
    const island=(await import('/src/island/store.ts')).useIsland;
    const sessions=(await import('/src/sessions/store.ts')).useSessions;
    const snapshot=(await import('/src/sessions/demo.ts')).demoSnapshot();
    sessions.setState(s=>({snapshot,focused:null,prefs:{...s.prefs,grouping:'none'}}));
    island.setState(s=>({expanded:true,pinned:true,selected:'sessions',notice:null,showingSections:false,
      settings:{...s.settings,layout:'custom',customWidth:600,customHeight:420}}));
    await document.fonts.ready;
    for(let i=0;i<12;i++) await new Promise(requestAnimationFrame);
  })()`);
  const boxes = () => evaluate(`[...document.querySelectorAll('.s-grouping button')].map(button=>{
    const r=button.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};
  })`);
  assert.equal((await boxes()).length, 3, "The grouping switch must show three choices");
  for (let i = 0; i < 3; i++) {
    const before = await boxes();
    const box = before[i];
    const x = box.x + box.width / 2;
    const y = box.y + box.height / 2;
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
    await new Promise((done) => setTimeout(done, 160));
    const pressed = await boxes();
    for (let j = 0; j < before.length; j++) {
      for (const key of ["x", "y", "width", "height"]) {
        assert(Math.abs(pressed[j][key] - before[j][key]) < 0.25, `Choice ${j} moved while pressing ${i}: ${key}`);
      }
    }
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
    await new Promise((done) => setTimeout(done, 250));
  }
  const frame = await evaluate(`(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();
    return {x:r.x,y:r.y,width:r.width,height:r.height}})()`);
  const screenshot = await send("Page.captureScreenshot", {
    format: "png",
    clip: { x: frame.x - 12, y: 0, width: frame.width + 24, height: Math.min(frame.height + 12, 550), scale: 1.5 },
  });
  const output = resolve("docs/assets/grouping-switch-stationary.png");
  await mkdir(resolve("docs/assets"), { recursive: true });
  await writeFile(output, Buffer.from(screenshot.data, "base64"));
  console.log(`Grouping switch icons stayed in place; screenshot: ${output}`);
}
