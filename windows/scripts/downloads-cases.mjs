// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

// The Downloads page's actions, its folder menu and the finished-download
// notice, against a stand-in for the Tauri backend that records each call
// (NotchDownloadsView.swift, NotchService "completed" notice).
const BACKEND = `(()=>{
  const callbacks=new Map();let next=1;const listeners={};
  window.__calls=[];
  window.__snapshot={items:[],folders:[]};
  window.__chosen=null;
  window.__emit=(event,payload)=>{for(const id of listeners[event]||[])callbacks.get(id)?.({event,id:0,payload})};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
  window.__TAURI_INTERNALS__={
    metadata:{currentWindow:{label:'island'},currentWebview:{windowLabel:'island',label:'island'}},
    transformCallback(fn){const id=next++;callbacks.set(id,fn);return id},
    unregisterCallback(id){callbacks.delete(id)},
    convertFileSrc:(p)=>p,
    async invoke(cmd,args){
      if(cmd==='plugin:event|listen'){(listeners[args.event]||=[]).push(args.handler);return next++}
      if(cmd.startsWith('plugin:'))return null;
      window.__calls.push({cmd,args});
      if(cmd==='downloads_snapshot')return window.__snapshot;
      if(cmd==='downloads_choose_folder')return window.__chosen;
      if(cmd==='prefs_get')return window.__prefs||{};
      if(cmd==='prefs_set'){window.__prefs={...window.__prefs,island:{...window.__prefs?.island,...args.patch.island}};
        window.__emit('prefs://changed',window.__prefs);return window.__prefs}
      if(cmd.startsWith('downloads_'))return true;
      return null;
    },
  };
})()`;

export async function checkDownloads({ send, evaluate, base }) {
  await send("Page.enable");
  await send("Page.addScriptToEvaluateOnNewDocument", { source: BACKEND });
  await send("Page.navigate", { url: `${base}island.html` });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  for (let i = 0; i < 100 && !(await evaluate(`!!document.querySelector('.n-activation')`)); i++) await wait(100);
  await wait(300);
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    island.setState({settings:{...island.getState().settings,mascotEnabled:false}});
  })()`);
  const shoot = async (name) => {
    await wait(700);
    const frame = await evaluate(`(()=>{const b=document.querySelector('.island-frame').getBoundingClientRect();
      return {left:b.left,width:b.width,bottom:b.bottom}})()`);
    const shot = await send("Page.captureScreenshot", {
      format: "png",
      clip: { x: frame.left - 20, y: 0, width: frame.width + 40, height: frame.bottom + 20, scale: 2 },
    });
    await mkdir(resolve("../build"), { recursive: true });
    await writeFile(resolve(`../build/${name}.png`), Buffer.from(shot.data, "base64"));
  };
  const calls = (name) => evaluate(`__calls.filter(c=>c.cmd===${JSON.stringify(name)}).map(c=>c.args)`);
  const row = (id, status, extra = {}) => ({
    id: `C:\\Users\\me\\Downloads\\${id}`,
    name: id,
    path: `C:\\Users\\me\\Downloads\\${id}`,
    source: "Chrome",
    received: 48_000_000,
    total: status === "downloading" ? 120_000_000 : 48_000_000,
    speed: status === "downloading" ? 3_200_000 : 0,
    status,
    ...extra,
  });
  const emit = (items, folders) => evaluate(`__emit('downloads://snapshot',${JSON.stringify({ items, folders })})`);
  const folders = ["C:\\Users\\me\\Downloads"];
  const real = (selector) =>
    evaluate(`(()=>{const e=document.querySelector(${JSON.stringify(selector)});if(!e)return null;
      const b=e.getBoundingClientRect();return {x:b.left+b.width/2,y:b.top+b.height/2}})()`);
  const click = async (selector) => {
    const at = await real(selector);
    assert(at, `Nothing to click at ${selector}`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
    for (const type of ["mousePressed", "mouseReleased"])
      await send("Input.dispatchMouseEvent", { type, x: at.x, y: at.y, button: "left", clickCount: 1 });
    await wait(150);
  };

  // A transfer running: the strip only, no banner.
  await emit([row("archive.zip", "downloading")], folders);
  await wait(200);
  assert.equal(await evaluate(`island.getState().notice`), null, "A running download shows no banner");
  assert(await evaluate(`!!island.getState().activities.downloads`), "A running download keeps its strip");

  // It finishes: the notice names it and a tap opens the page.
  await emit([row("archive.zip", "completed")], folders);
  await wait(100);
  const notice = await evaluate(
    `(()=>{const n=island.getState().notice;return n&&{title:n.title,detail:n.detail,opens:n.opens,duration:n.duration}})()`,
  );
  assert.deepEqual(notice, { title: "Download complete", detail: "archive.zip", opens: "downloads", duration: 6 });
  await shoot("downloads-notice");
  const glyph = await evaluate(`(()=>{const t=document.querySelector('.n-notice-title svg');
    const s=document.querySelector('.n-notice-title > span:last-child');
    return {glyph:!!t,text:s?.textContent,clipped:s?s.scrollWidth>s.getBoundingClientRect().width+0.5:null}})()`);
  assert.deepEqual(glyph, { glyph: true, text: "Download complete", clipped: false });

  // The same snapshot again says nothing new.
  await evaluate(`island.getState().dismissNotice()`);
  await emit([row("archive.zip", "completed")], folders);
  await wait(50);
  assert.equal(await evaluate(`island.getState().notice`), null, "One file, one notice");

  await emit([row("archive.zip", "completed")], folders);
  await evaluate(
    `island.getState().showNotice({id:'again',kind:'downloads',title:'Download complete',detail:'archive.zip',priority:0,duration:6,opens:'downloads'})`,
  );
  await wait(300);
  await click(".n-notice");
  await wait(300);
  assert.deepEqual(await evaluate(`({e:island.getState().expanded,s:island.getState().selected})`), {
    e: true,
    s: "downloads",
  });

  // A page with one of each: only the saved row has actions.
  await emit(
    [row("installer.exe", "downloading"), row("archive.zip", "completed"), row("broken-transfer.iso", "interrupted")],
    folders,
  );
  await wait(150);
  const rows = await evaluate(`[...document.querySelectorAll('.dl-item')].map(r=>({
    name:r.querySelector('strong').textContent,button:r.querySelector('.dl-open').tagName,
    actions:[...r.querySelectorAll('.dl-actions button')].map(b=>b.getAttribute('aria-label'))}))`);
  assert.deepEqual(rows, [
    { name: "installer.exe", button: "DIV", actions: [] },
    { name: "archive.zip", button: "BUTTON", actions: ["Show in File Explorer", "Add to Files"] },
    { name: "broken-transfer.iso", button: "DIV", actions: [] },
  ]);
  const fits = await evaluate(`(()=>{const page=document.querySelector('.dl-page');
    return {sideways:page.scrollWidth>page.clientWidth,
      inside:[...document.querySelectorAll('.dl-actions')].every(a=>a.getBoundingClientRect().right<=page.getBoundingClientRect().right+0.5)}})()`);
  assert.deepEqual(fits, { sideways: false, inside: true });
  await shoot("downloads-page");

  const savedId = "C:\\Users\\me\\Downloads\\archive.zip";
  await click("button.dl-open");
  assert.deepEqual(await calls("downloads_open"), [{ id: savedId }]);
  await click('.dl-actions button[aria-label="Show in File Explorer"]');
  assert.deepEqual(await calls("downloads_reveal"), [{ id: savedId }]);
  await click('.dl-actions button[aria-label="Add to Files"]');
  assert.deepEqual(await calls("downloads_add_to_shelf"), [{ id: savedId }]);
  const added = await evaluate(
    `(()=>{const b=document.querySelector('.dl-actions button[aria-label="Added to Files"]');return b?b.disabled:null})()`,
  );
  assert.equal(added, true, "A file added once can't be added twice by accident");
  assert.deepEqual(await evaluate(`({e:island.getState().expanded,s:island.getState().selected})`), {
    e: true,
    s: "downloads",
  });

  // With the Files section hidden there is no shelf to add to.
  await evaluate(`island.setState({settings:{...island.getState().settings,hiddenModules:['files']}})`);
  await wait(100);
  assert.deepEqual(
    await evaluate(`[...document.querySelectorAll('.dl-actions button')].map(b=>b.getAttribute('aria-label'))`),
    ["Show in File Explorer"],
  );
  await evaluate(`island.setState({settings:{...island.getState().settings,hiddenModules:[]}})`);
  await wait(100);

  // The folder menu names the one folder, opens it, and adds another.
  const band = () => evaluate(`document.querySelector('.dl-band-menu > span').textContent`);
  assert.equal(await band(), "Downloads");
  await emit([row("archive.zip", "completed")], ["C:\\Users\\me\\Pobrane"]);
  await wait(100);
  assert.equal(await band(), "Pobrane");
  await emit([row("archive.zip", "completed")], [...folders, "D:\\Games\\Incoming", "E:\\Downloads"]);
  await wait(100);
  assert.equal(await band(), "Downloads", "Several folders go by the section's name");
  await click(".dl-band-menu");
  const entries = await evaluate(`[...document.querySelectorAll('.u-menu [role="menuitem"]')].map(e=>e.textContent)`);
  assert.deepEqual(entries, ["C:\\Users\\me\\Downloads", "Incoming", "E:\\Downloads", "Add Folder…"]);
  const insideMenu =
    await evaluate(`(()=>{const m=document.querySelector('.u-menu').getBoundingClientRect(),f=document.querySelector('.island-frame').getBoundingClientRect();
    return m.left>=f.left&&m.right<=f.right&&m.bottom<=f.bottom})()`);
  assert(insideMenu, "The menu stays inside the island");
  await shoot("downloads-folder-menu");
  await evaluate(`[...document.querySelectorAll('.u-menu [role="menuitem"]')][1].click()`);
  await wait(100);
  assert.deepEqual(await calls("downloads_open_folder"), [{ folder: "D:\\Games\\Incoming" }]);

  // Add Folder: cancelled, already watched, then new; the island stays open throughout.
  const addFolder = async (chosen) => {
    await evaluate(`__chosen=${JSON.stringify(chosen)}`);
    await click(".dl-band-menu");
    await evaluate(`[...document.querySelectorAll('.u-menu [role="menuitem"]')].at(-1).click()`);
    await wait(150);
  };
  await addFolder(null);
  assert.deepEqual(await calls("prefs_set"), [], "Cancelling saves nothing");
  await addFolder("c:\\users\\ME\\downloads");
  assert.deepEqual(await calls("prefs_set"), [], "A folder already watched isn't added twice");
  await addFolder("F:\\New");
  assert.deepEqual(
    (await calls("prefs_set")).map((c) => c.patch),
    [{ island: { downloadFolders: ["F:\\New"] } }],
  );
  assert.deepEqual(await evaluate(`({e:island.getState().expanded,held:island.getState().held})`), {
    e: true,
    held: false,
  });
  console.log("Downloads OK: notice, actions, folder menu, Add Folder");
}
