// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

// A session notice wears its session's Clawd, and a tap opens that session's
// page rather than the list (NotchService.swift:2523).
export async function checkSessionNotice({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.n-activation')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  const frames = () => evaluate(`(async()=>{for(let i=0;i<8;i++)await new Promise(requestAnimationFrame)})()`);
  // Notices rather than the island opening: the demo's two waiting sessions
  // arrive through the same path a real snapshot takes.
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    const demo=(await import('/src/sessions/demo.ts')).demoSnapshot;
    await document.fonts.ready;
    sessions.setState(s=>({prefs:{...s.prefs,autoExpand:false}}));
    sessions.getState().receive(demo());
  })()`);
  for (let i = 0; i < 50; i++) {
    if (await evaluate(`!!island.getState().notice?.session`)) break;
    await new Promise((r) => setTimeout(r, 50));
  }
  await frames();
  const notice = await evaluate(`(()=>{
    const n=island.getState().notice;
    const view=document.querySelector('.n-notice');
    const canvas=view?.querySelector('.n-notice-title canvas');
    const title=view?.querySelector('.n-notice-title > span:last-child');
    const r=e=>{const b=e.getBoundingClientRect();return{left:b.left,right:b.right,width:b.width,height:b.height}};
    return {kind:n?.kind,session:n?.session,canvas:canvas?r(canvas.parentElement):null,title:title?r(title):null,
      clipped:title?title.scrollWidth>title.clientWidth:null,frame:r(document.querySelector('.island-frame'))};
  })()`);
  assert.equal(notice.kind, "sessions");
  const about = await evaluate(`(()=>{const id=island.getState().notice.session.id;
    const s=sessions.getState().snapshot.sessions.find(x=>x.id===id);
    return {id,provider:s.provider,event:s.phase==='waitingInput'?'needsInput':'needsPermission'}})()`);
  assert.deepEqual(notice.session, about, "The notice must carry its session and what happened");
  assert(notice.canvas, "The notice must show the session's Clawd");
  assert(notice.canvas.right <= notice.title.left, "The Clawd stands before the title");
  const screenshot = await send("Page.captureScreenshot", {
    format: "png",
    clip: { x: notice.frame.left - 20, y: 0, width: notice.frame.width + 40, height: 60, scale: 3 },
  });
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(resolve("../build/session-notice.png"), Buffer.from(screenshot.data, "base64"));

  // The Clawd's slot is measured in: with a short detail the title side sets
  // the width, and the title must still end before the camera.
  const fit = await evaluate(`(async()=>{
    const n=island.getState().notice;
    island.getState().showNotice({...n,id:'fit',title:'a-rather-long-project-name',detail:'Done'});
    for(let i=0;i<8;i++)await new Promise(requestAnimationFrame);
    const view=document.querySelector('.n-notice');
    const title=view.querySelector('.n-notice-title > span:last-child').getBoundingClientRect();
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const gap=parseFloat(getComputedStyle(view).columnGap)||0;
    return {right:title.right,limit:frame.left+frame.width/2-gap/2,clipped:view.querySelector('.n-notice-title > span:last-child').scrollWidth>title.width+0.5};
  })()`);
  assert(fit.right <= fit.limit + 0.5 && !fit.clipped, `The title must end before the camera: ${JSON.stringify(fit)}`);

  // The strip's own tap opens the first waiting session; the notice's opens its own.
  const first = await evaluate(
    `sessions.getState().snapshot.sessions.find(s=>s.permission||s.phase.startsWith('waiting')).id`,
  );
  assert.notEqual(notice.session.id, first, "Pick a notice whose session the strip would not open");
  await evaluate(`document.querySelector('.n-notice').click()`);
  await frames();
  const opened = await evaluate(`({expanded:island.getState().expanded,selected:island.getState().selected,
    focused:sessions.getState().focused,notice:island.getState().notice,
    back:!!document.querySelector('.s-back')})`);
  assert.deepEqual(opened, {
    expanded: true,
    selected: "sessions",
    focused: notice.session.id,
    notice: null,
    back: true,
  });

  // With buddies off the notice keeps the phase's symbol instead.
  await evaluate(`(async()=>{
    island.getState().collapse();
    sessions.setState(s=>({focused:null,prefs:{...s.prefs,buddies:false}}));
    const n=sessions.getState().snapshot.sessions.find(s=>s.id===${JSON.stringify(notice.session.id)});
    island.getState().showNotice({...${JSON.stringify({ kind: "sessions", priority: 2, duration: 6, opens: "sessions" })},
      id:'off',title:n.name??'x',session:${JSON.stringify(notice.session)}});
  })()`);
  await frames();
  const off = await evaluate(`(()=>{const t=document.querySelector('.n-notice-title');
    return {canvas:!!t.querySelector('canvas'),svg:!!t.querySelector('svg')}})()`);
  assert.deepEqual(off, { canvas: false, svg: true });
  console.log(
    `Session notice passed (${notice.session.event} for ${notice.session.id}). Screenshot: build/session-notice.png`,
  );
}
