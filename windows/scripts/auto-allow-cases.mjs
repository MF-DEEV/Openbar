// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkAutoAllow({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?sessions` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.s-strip')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async()=>{
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.bridge=(await import('/src/shared/bridge.ts')).bridge;
    const demo=(await import('/src/sessions/demo.ts')).demoSnapshot();
    window.raw={...demo.sessions[0],provider:'codex',turnStarted:Date.now()/1000-20};
    window.receive=(phase,permission=undefined)=>sessions.getState().receive({...demo,sessions:[{...raw,phase,permission}],counts:{working:phase==='generating'?1:0,workingCodex:phase==='generating'?1:0,waiting:phase.startsWith('waiting')?1:0,finished:phase==='finished'?1:0}});
    window.approved=[];
    bridge.sessions.decide=async(id,request,decision)=>{approved.push(request);return true};
    island.setState({expanded:false,inside:false,notice:null,activityPickerOpen:false});
    receive('waitingPermission',raw.permission);
    sessions.getState().toggleAutoAllow(raw.id);
  })()`);
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  for (const expanded of [false, true]) {
    const result = await evaluate(`(async()=>{
      island.setState({expanded:${expanded},selected:'sessions',showingSections:false});
      sessions.setState({focused:raw.id});
      const frames=[];
      for(let request=0;request<3;request++) {
        for(const [phase,permission,count] of [
          ['waitingPermission',undefined,3],
          ['waitingPermission',{...raw.permission,requestId:'test-${expanded}-'+request},65],
          ['waitingPermission',undefined,3],
          ['generating',undefined,5]
        ]) {
          receive(phase,permission);
          const began=performance.now();
          for(let i=0;i<count || (permission && performance.now()-began<1000);i++) {
            await new Promise(requestAnimationFrame);
            const strip=document.querySelector('.s-strip');
            frames.push({working:!!strip?.querySelector('.agent-mark.is-working'),
              blue:[...document.querySelectorAll('.s-strip-count')].every(e=>getComputedStyle(e).color==='rgb(125, 153, 255)'),
              card:!!document.querySelector('.s-card'),
              detail:document.querySelector('.s-phase')?.textContent,
              expanded:island.getState().expanded});
          }
        }
      }
      return {frames,approved:approved.length};
    })()`);
    assert(
      result.frames.every((f) => (expanded ? !f.card && f.detail === "Working" : f.working && f.blue && !f.expanded)),
      `Automatic approval flashed a waiting state (${expanded ? "detail" : "compact"})`,
    );
    assert(
      result.approved >= (expanded ? 6 : 3),
      `Requests must still be approved in the background (received ${result.approved})`,
    );
  }
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(resolve("../build/auto-allow-working.png"), Buffer.from(screenshot.data, "base64"));
  const cancelled = await evaluate(`(async()=>{
    document.querySelector('button[aria-label="Cancel autoaccept"]').click();
    for(let i=0;i<3;i++)await new Promise(requestAnimationFrame);
    const hidden=!document.querySelector('button[aria-label="Cancel autoaccept"]') && !sessions.getState().autoAllow[raw.id];
    receive('waitingPermission',{...raw.permission,requestId:'manual-again'});
    await new Promise(r=>setTimeout(r,900));
    document.querySelector('.s-card button[aria-keyshortcuts="Control+t"]').click();
    for(let i=0;i<3;i++)await new Promise(requestAnimationFrame);
    return hidden && !!document.querySelector('button[aria-label="Cancel autoaccept"]') && sessions.getState().autoAllow[raw.id];
  })()`);
  assert(cancelled, "The status bar must only cancel autoaccept; enabling it belongs to the approval card");
  const question = await evaluate(`(async()=>{
    receive('waitingInput',{...raw.permission,requestId:'question',questions:[{question:'Choose one',options:[],multiSelect:false}]});
    for(let i=0;i<6;i++)await new Promise(requestAnimationFrame);
    return !!document.querySelector('.s-questions') && sessions.getState().autoAllow[raw.id];
  })()`);
  assert(question, "Real questions must stay visible without ending Always");
  const failed = await evaluate(`(async()=>{
    bridge.sessions.decide=async()=>false;
    receive('waitingPermission',{...raw.permission,requestId:'failed'});
    await new Promise(r=>setTimeout(r,1000));
    for(let i=0;i<3;i++)await new Promise(requestAnimationFrame);
    return !sessions.getState().autoAllow[raw.id] && !!document.querySelector('.s-card .is-allow');
  })()`);
  assert(failed, "Failed auto approval must return to a visible manual request");
  console.log(
    "Auto Allow passed: no waiting frame across six approvals in compact/detail views, blue working state, real questions and failure fallback. Screenshot: build/auto-allow-working.png",
  );
}
