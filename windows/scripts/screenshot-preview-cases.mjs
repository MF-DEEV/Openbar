// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkScreenshotPreview({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?desktop&screenshot-preview` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.shot-preview--embedded img')?.complete`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(
    `(async () => { window.island = (await import('/src/island/store.ts')).useIsland; await document.fonts.ready; })()`,
  );
  let cases = 0;
  for (const dpi of [1, 1.25, 1.5, 2]) {
    await send("Emulation.setDeviceMetricsOverride", {
      width: 1440,
      height: 900,
      deviceScaleFactor: dpi,
      mobile: false,
    });
    for (const silhouette of ["notch", "capsule"]) {
      for (const [width, height] of [
        [560, 328],
        [360, 260],
        [600, 640],
        [240, 120],
      ]) {
        const result = await evaluate(`(async () => {
          island.setState(s => ({settings: {...s.settings, silhouette: ${JSON.stringify(silhouette)}, layout: 'custom', customWidth: ${width}, customHeight: ${height}}}));
          for(let i=0;i<8;i++) await new Promise(requestAnimationFrame);
          const root = [...document.querySelectorAll('.n-capture-preview')].at(-1);
          const box = e => {const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom}};
          const inside=(a,b)=>a.x>=b.x-1 && a.y>=b.y-1 && a.right<=b.right+1 && a.bottom<=b.bottom+1;
          const frame=box(root), canvas=box(root.querySelector('.shot-canvas')), image=box(root.querySelector('img'));
          const toolbar=box(root.querySelector('.shot-toolbar'));
          const buttons=[...root.querySelectorAll('.shot-toolbar button')].map(box);
          return {fits:inside(box(root.querySelector('.n-expanded-layout')),frame), imageFits:inside(image,canvas), toolbarAbove:toolbar.bottom<=image.y+1, buttonsFit:buttons.every(b=>inside(b,toolbar)), overlap:buttons.some((b,i)=>i && b.x<buttons[i-1].right-1), actions:root.querySelectorAll('.shot-toolbar button').length, natural:root.querySelector('img').naturalWidth/root.querySelector('img').naturalHeight, displayed:image.width/image.height};
        })()`);
        const description = `${dpi}x ${silhouette} ${width}x${height} ${JSON.stringify(result)}`;
        assert(
          result.fits && result.imageFits && result.toolbarAbove && result.buttonsFit && !result.overlap,
          description,
        );
        assert.equal(result.actions, 9, description);
        assert(Math.abs(result.natural - result.displayed) < 0.02, description);
        cases++;
      }
    }
  }
  await send("Emulation.setDeviceMetricsOverride", {
    width: 1440,
    height: 900,
    deviceScaleFactor: 1.25,
    mobile: false,
  });
  await evaluate(`island.setState(s=>({settings:{...s.settings,silhouette:'notch',layout:'spacious'}}))`);
  await new Promise((r) => setTimeout(r, 500));
  assert.equal(
    await evaluate(`getComputedStyle(document.querySelector('.shot-preview--embedded')).paddingTop`),
    "40px",
    "Screenshot styles must not collide with scratchpad preview styles",
  );
  await mkdir(resolve("../build"), { recursive: true });
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  const artifact = resolve("../build/screenshot-preview-macos.png");
  await writeFile(artifact, Buffer.from(screenshot.data, "base64"));
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await new Promise((r) => setTimeout(r, 100));
  assert.equal(await evaluate("island.getState().capturePreview"), null, "Escape must close the screenshot");
  console.log(`${cases} screenshot preview layout cases passed; Escape passed. Screenshot: ${artifact}`);
}
