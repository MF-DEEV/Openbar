// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkActivityPicker({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?music&sessions` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.s-strip, .m-strip')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.music=(await import('/src/music/store.ts')).useMusic;
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    window.demoMusic=(await import('/src/music/demo.ts')).demoNowPlaying;
    window.demoSessions=(await import('/src/sessions/demo.ts')).demoSnapshot;
    music.getState().receive(demoMusic());sessions.getState().receive(demoSessions());
    island.setState({expanded:false,inside:false,hoverSuppressed:false,notice:null,autoPresentAllowed:true,activityPickerOpen:false});
  })()`);
  const frames = () => evaluate(`(async()=>{for(let i=0;i<6;i++)await new Promise(requestAnimationFrame)})()`);
  await frames();
  const restingSize = await evaluate(
    `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return[r.width,r.height]})()`,
  );
  await evaluate(`island.getState().pointerEnter()`);
  await frames();
  assert.equal(
    await evaluate(`document.querySelectorAll('.n-activity-choice').length`),
    0,
    "Hover must not reveal choices",
  );
  const hoverSize = await evaluate(
    `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return[r.width,r.height]})()`,
  );
  assert(
    Math.abs(hoverSize[0] / restingSize[0] - 1.03) < 0.001 && Math.abs(hoverSize[1] / restingSize[1] - 1.03) < 0.001,
    "Hover should grow the strip by only 3%",
  );
  await evaluate(`document.querySelector('.s-strip').click()`);
  await frames();
  assert.equal(await evaluate(`island.getState().expanded`), false);
  assert.deepEqual(await evaluate(`[...document.querySelectorAll('.n-activity-choice')].map(e=>e.textContent)`), [
    "Sessions",
    "Music",
  ]);
  const sizeBefore = await evaluate(
    `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return[r.width,r.height]})()`,
  );
  await evaluate(`document.querySelector('.n-activity-choice[aria-label="Music"]').click()`);
  await frames();
  assert.equal(await evaluate(`island.getState().pickedActivity`), "music");
  assert(await evaluate(`!!document.querySelector('.n-activity-picker-header .m-strip')`));
  assert.deepEqual(
    await evaluate(
      `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return[r.width,r.height]})()`,
    ),
    sizeBefore,
  );
  assert.equal(
    await evaluate(`getComputedStyle(document.querySelector('.n-activity-choice[aria-pressed="true"]')).color`),
    "rgb(0, 0, 0)",
  );
  await evaluate(`document.querySelector('.n-activity-choice[aria-label="Music"]').focus()`);
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "ArrowLeft", code: "ArrowLeft" });
  await frames();
  assert.equal(await evaluate(`island.getState().pickedActivity`), "sessions");
  await evaluate(`document.querySelector('.n-activity-choice[aria-label="Music"]').click()`);
  await frames();
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(resolve("../build/activity-picker.png"), Buffer.from(screenshot.data, "base64"));
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await new Promise((r) => setTimeout(r, 300));
  const closing = await evaluate(`(async()=>{
    const original=document.querySelector('.n-activity-picker-header .m-strip');
    const top=original.getBoundingClientRect().top;
    island.getState().pointerExit();
    const samples=[];
    for(let i=0;i<50;i++){
      await new Promise(requestAnimationFrame);
      const strip=document.querySelector('.n-activity-picker-header .m-strip');
      samples.push({layers:document.querySelectorAll('.island-layer').length,strips:document.querySelectorAll('.m-strip').length,same:strip===original,top:strip?.getBoundingClientRect().top,height:document.querySelector('.island-layer').getBoundingClientRect().height});
    }
    return {top,samples};
  })()`);
  assert(
    closing.samples.every((s) => s.layers === 1 && s.strips === 1 && s.same),
    "Closing duplicated or replaced the live strip",
  );
  assert(
    closing.samples.every((s) => Math.abs(s.top - closing.top) < 0.1),
    "Closing made the strip jump vertically",
  );
  assert(
    closing.samples.every((s, i) => !i || s.height <= closing.samples[i - 1].height + 0.5),
    "Closing ran the resize twice",
  );
  assert.equal(await evaluate(`document.querySelectorAll('.n-activity-picker').length`), 0);
  for (const activity of ["music", "sessions"]) {
    await evaluate(`island.setState({pickedActivity:${JSON.stringify(activity)}})`);
    await new Promise((r) => setTimeout(r, 250));
    const hover = await evaluate(`(async()=>{
      const samples=[];
      for(const action of ['pointerEnter','pointerExit','pointerEnter','pointerExit']) {
        island.getState()[action]();
        for(let i=0;i<12;i++) {
          await new Promise(requestAnimationFrame);
          const header=document.querySelector('.n-activity-picker-strip');
          const content=header.firstElementChild;
          const scale=new DOMMatrixReadOnly(getComputedStyle(content).transform);
          const surface=document.querySelector('.island-backdrop path[id]').getBBox();
          samples.push({x:scale.a,y:scale.d,width:surface.width/parseFloat(header.style.width),height:surface.height/parseFloat(header.style.height)});
        }
      }
      return samples;
    })()`);
    assert(
      hover.every((s) => Math.abs(s.x - s.width) < 0.001 && Math.abs(s.y - s.height) < 0.001),
      `${activity} icons must follow the backdrop on every hover frame, including interrupted motion`,
    );
    assert(
      hover.some((s) => s.x > 1.002 && s.x < 1.028),
      "Hover must include intermediate frames",
    );
    await new Promise((r) => setTimeout(r, 600));
  }
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await evaluate(`island.getState().activateActivity('music')`);
  await frames();
  await evaluate(`document.querySelector('.n-activity-picker-header .m-strip').click()`);
  await frames();
  assert(
    await evaluate(
      `island.getState().expanded && island.getState().selected==='music' && !!document.querySelector('.m-page')`,
    ),
  );
  await evaluate(`island.getState().collapse();music.setState(s=>({playback:{...s.playback,isPlaying:false}}));`);
  await frames();
  assert.equal(await evaluate(`document.querySelectorAll('.n-activity-picker').length`), 0);
  await evaluate(`document.querySelector('.s-strip').click()`);
  await frames();
  assert(await evaluate(`island.getState().expanded && island.getState().selected==='sessions'`));
  await evaluate(
    `island.getState().collapse();music.getState().receive(demoMusic());sessions.getState().receive(demoSessions());`,
  );
  for (const silhouette of ["notch", "capsule"])
    for (const roundness of [0, 100, 200])
      for (const notchHeight of [20, 64]) {
        await evaluate(
          `island.setState(s=>({expanded:false,inside:false,hoverSuppressed:false,activityPickerOpen:true,pickedActivity:'music',settings:{...s.settings,silhouette:${JSON.stringify(silhouette)},roundness:${roundness},notchHeight:${notchHeight},shoulderWidth:300}}))`,
        );
        await frames();
        const result = await evaluate(
          `(()=>{const frame=document.querySelector('.island-frame').getBoundingClientRect();const parts=[...document.querySelectorAll('.n-activity-choice,.n-activity-picker-strip')];return parts.every(e=>{const r=e.getBoundingClientRect();return r.left>=frame.left-1&&r.right<=frame.right+1&&r.top>=frame.top-1&&r.bottom<=frame.bottom+1})})()`,
        );
        assert(result, `Picker overflow: ${silhouette}, roundness ${roundness}, strip ${notchHeight}`);
      }
  await evaluate(
    `(async()=>{const{useDownloads}=await import('/src/downloads/Downloads.tsx');useDownloads.setState({items:[{id:'test',name:'example.zip',path:'',source:'Browser',received:100,total:1000,speed:100,status:'downloading'}]});island.setState(s=>({settings:{...s.settings}}))})()`,
  );
  await frames();
  assert.equal(await evaluate(`document.querySelectorAll('.n-activity-choice').length`), 3);
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape" });
  await new Promise((r) => setTimeout(r, 250));
  await frames();
  assert.equal(await evaluate(`island.getState().activityPickerOpen`), false);
  assert.equal(await evaluate(`document.querySelectorAll('.n-activity-picker').length`), 0);
  console.log(
    "Activity picker passed: real music and Codex/Claude sessions, first click chooser, stable size, selection and keyboard, full page opening, paused music, 12 contour variants, third live activity and Escape. Screenshot: build/activity-picker.png",
  );
}
