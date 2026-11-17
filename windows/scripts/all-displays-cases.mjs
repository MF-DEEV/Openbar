// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

// All displays: the island page sends the closed surface and a mirror page
// draws it the same, against a stand-in for the Tauri backend (spec 01-shell §6.1).
const BACKEND = `(()=>{
  const callbacks=new Map();let next=1;const listeners={};
  window.__calls=[];
  window.__frame=null;
  window.__emit=(event,payload)=>{for(const id of listeners[event]||[])callbacks.get(id)?.({event,id:0,payload})};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
  window.__TAURI_INTERNALS__={
    metadata:{currentWindow:{label:'x'},currentWebview:{windowLabel:'x',label:'x'}},
    transformCallback(fn){const id=next++;callbacks.set(id,fn);return id},
    unregisterCallback(id){callbacks.delete(id)},
    convertFileSrc:(p)=>p,
    async invoke(cmd,args){
      if(cmd==='plugin:event|listen'){(listeners[args.event]||=[]).push(args.handler);return next++}
      if(cmd.startsWith('plugin:'))return null;
      window.__calls.push(cmd);
      if(cmd==='prefs_get')return {island:{display:'all'}};
      if(cmd==='island_mirroring')return true;
      if(cmd==='island_mirror_update'){window.__frame=args.frame;return null}
      if(cmd==='island_mirror_frame')return window.__seed??null;
      return null;
    },
  };
})()`;

export async function checkAllDisplays({ send, evaluate, base }) {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  await send("Page.enable");
  await send("Page.addScriptToEvaluateOnNewDocument", { source: BACKEND });
  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100 && !(await evaluate(`!!__frame`)); i++) await wait(100);
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.timer=(await import('/src/timer/store.ts'));
  })()`);
  const shoot = async (name, height = 60) => {
    const shot = await send("Page.captureScreenshot", {
      format: "png",
      clip: { x: 720 - 300, y: 0, width: 600, height, scale: 2 },
    });
    await mkdir(resolve("../build"), { recursive: true });
    await writeFile(resolve(`../build/${name}.png`), Buffer.from(shot.data, "base64"));
  };
  const frame = () => evaluate(`__frame`);

  // At rest: the frame is the resting island, off-screen here.
  const resting = await frame();
  assert(resting.html.includes("island-frame"), "The frame holds the island's surface");
  assert(resting.html.includes("n-activation"), "At rest it holds the resting view");
  assert(resting.width > 0 && resting.hit.width > 0, "It says where its shape is");
  const source = await evaluate(
    `(()=>{const b=document.querySelector('.island-mirror-source .island-frame').getBoundingClientRect();return b.right<0})()`,
  );
  assert(source, "The source draws off-screen");

  // A strip running: the frame follows, canvases become pictures.
  await evaluate(`island.getState().setActivity('downloads',{wing:36,capsuleContent:68,opens:'downloads'})`);
  await wait(450);
  assert((await frame()).html.includes("dl-strip"), "A running activity reaches the mirrors");

  // Opening this island changes nothing on the mirrors.
  const before = (await frame()).html;
  await evaluate(`island.getState().open('downloads')`);
  await wait(450);
  assert.equal(await evaluate(`island.getState().expanded`), true);
  assert.equal((await frame()).html, before, "The mirrors keep the closed island");
  assert(!(await frame()).html.includes("n-expanded"));
  await evaluate(`island.getState().collapse()`);

  // A notice doesn't reach them either.
  await evaluate(
    `island.getState().showNotice({id:'n',kind:'x',title:'Only here',priority:3,duration:9,opens:'downloads'})`,
  );
  await wait(450);
  assert(!(await frame()).html.includes("Only here"), "Notices stay on the island");
  await evaluate(`island.getState().dismissNotice()`);

  // Hidden until hover sends nothing to draw.
  await evaluate(
    `island.setState({settings:{...island.getState().settings,opening:'hiddenUntilHover',hiddenShowsActivity:false}})`,
  );
  await wait(450);
  assert.equal(await frame(), null, "A hidden island hides on every display");
  await evaluate(`island.setState({settings:{...island.getState().settings,opening:'click'}})`);
  // Changing settings re-reads the real downloads, of which there are none.
  await evaluate(`island.getState().setActivity('downloads',{wing:36,capsuleContent:68,opens:'downloads'})`);
  await wait(450);
  await shoot("all-displays-island");

  // The mirror page draws that frame the same, and a click asks to open.
  const sent = await frame();
  const mainBox = await evaluate(
    `(()=>{const b=document.querySelector('.island-frame').getBoundingClientRect();return [b.left,b.top,b.width,b.height]})()`,
  );
  await send("Page.addScriptToEvaluateOnNewDocument", { source: `window.__seed=${JSON.stringify(sent)}` });
  await send("Page.navigate", { url: `${base}mirror.html` });
  for (let i = 0; i < 100 && !(await evaluate(`!!document.querySelector('.island-frame')`)); i++) await wait(100);
  await wait(600);
  const drawn = await evaluate(`(()=>{const f=document.querySelector('.island-frame');const b=f.getBoundingClientRect();
    return {box:[b.left,b.top,b.width,b.height],position:getComputedStyle(f).position,strip:!!document.querySelector('.dl-strip'),
      background:getComputedStyle(document.body).backgroundColor}})()`);
  assert.deepEqual(drawn.box, mainBox, "The mirror's island sits where the island does");
  assert.equal(drawn.position, "absolute", "The page's styles reached the mirror");
  assert.equal(drawn.strip, true);
  await shoot("all-displays-mirror");
  await evaluate(`document.querySelector('.island-frame').click()`);
  await wait(100);
  assert((await evaluate(`__calls`)).includes("island_mirror_open"), "A click brings the island here");
  await evaluate(`__emit('island://mirror-frame',null)`);
  assert.equal(await evaluate(`document.querySelector('.island-frame')`), null, "Nothing to draw clears it");
  console.log("All displays OK: frame, activity, open and notice ignored, hidden, mirror drawn, click");
}
