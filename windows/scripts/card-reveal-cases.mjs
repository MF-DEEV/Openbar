// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
// A waiting ask's buttons stay in sight on the session page. Drives the real
// island page with a background session in a worktree (the tallest page top),
// previews off, scrolled to its end as the page follows the session, then has
// it ask: the card's buttons must sit inside the page, not above or below it.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkCardReveal({ send, evaluate, base }) {
  await send("Page.enable");
  await send("Page.navigate", { url: `${base}island.html?sessions` });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  for (let i = 0; i < 100 && !(await evaluate(`!!document.querySelector('.s-strip')`)); i++) await wait(100);
  await evaluate(`(async()=>{
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.bridge=(await import('/src/shared/bridge.ts')).bridge;
    const demo=(await import('/src/sessions/demo.ts')).demoSnapshot();
    const base=demo.sessions.find(s=>s.permission&&!s.permission.questions.length);
    const t=Date.now()/1000;
    const ask=(requestId)=>({...base.permission,requestId,tool:'Bash',detail:'touch hello.txt',summary:'touch hello.txt',
      since:t-2,questions:[],diffs:[],planModes:[],offersAlways:true,fitsOnALine:true,risk:'safe',
      command:{revision:'rev-'+requestId,complete:true,truncated:false,remaining:null,requiresInspection:false,available:true}});
    const session=(phase,permission)=>({...base,id:'bg',name:'please touch a file',title:'please touch a file',
      project:'demo',cwd:'/tmp/demo/.claude/worktrees/please-touch-a-file',phase,phaseSince:t-5,
      permission,laterAsks:[],subagents:[],tasks:undefined,currentTool:undefined,
      timeline:[{id:'x1',name:'Bash',summary:'touch hello.txt',started:t-30,finished:t-29,failed:false},
        {id:'x2',name:'Bash',summary:'touch hello.txt',started:t-3,failed:false}],background:{job:'55edb1b0',state:phase==='waitingPermission'?'blocked':'working',ended:false}});
    window.receive=(phase,permission)=>sessions.getState().receive({...demo,previews:false,sessions:[session(phase,permission)],
      counts:{working:phase==='generating'?1:0,workingCodex:0,waiting:phase==='waitingPermission'?1:0,finished:0}});
    window.ask=ask;
    bridge.sessions.decide=async()=>true;
    bridge.sessions.acknowledge=async()=>true;
    bridge.sessions.focus=async()=>true;
    bridge.sessions.worktreeBranch=async()=>'please-touch-a-file';
    receive('generating');
    island.setState({expanded:true,selected:'sessions',showingSections:false,notice:null});
    sessions.getState().focus('bg');
  })()`);
  const shoot = async (name) => {
    const frame = await evaluate(`(()=>{const b=document.querySelector('.island-frame').getBoundingClientRect();
      return {left:b.left,width:b.width,bottom:b.bottom}})()`);
    const shot = await send("Page.captureScreenshot", {
      format: "png",
      clip: { x: frame.left - 20, y: 0, width: frame.width + 40, height: frame.bottom + 20, scale: 2 },
    });
    await mkdir(resolve("../build"), { recursive: true });
    await writeFile(resolve(`../build/${name}.png`), Buffer.from(shot.data, "base64"));
  };
  const place = () =>
    evaluate(`(()=>{
      const page=document.querySelector('.s-scroll');
      const rows=document.querySelectorAll('.s-card-buttons');
      const buttons=rows.length?rows[rows.length-1].getBoundingClientRect():null;
      const box=page.getBoundingClientRect();
      return {top:box.top,bottom:box.bottom,scrollTop:page.scrollTop,end:page.scrollHeight-page.clientHeight,
        buttonsTop:buttons?.top??null,buttonsBottom:buttons?.bottom??null};
    })()`);

  await wait(900);
  let p = await place();
  assert.equal(p.buttonsTop, null, "no card while the session works");
  assert(p.end > 0, `the page has to scroll for this case: ${JSON.stringify(p)}`);
  // Following the session leaves the page at its end, as on a long run.
  await evaluate(`(()=>{const page=document.querySelector('.s-scroll');page.scrollTop=page.scrollHeight})()`);

  await evaluate(`receive('waitingPermission',ask('r1'))`);
  await wait(900);
  p = await place();
  await shoot("card-reveal");
  assert.notEqual(p.buttonsTop, null, "the card is on the page");
  assert(
    p.buttonsTop >= p.top - 0.5 && p.buttonsBottom <= p.bottom + 0.5,
    `Allow and Deny sit inside the page: ${JSON.stringify(p)}`,
  );

  // A second ask after the person scrolled away brings the buttons back too.
  await evaluate(`(()=>{const page=document.querySelector('.s-scroll');
    page.dispatchEvent(new WheelEvent('wheel',{deltaY:400}));page.scrollTop=page.scrollHeight})()`);
  await evaluate(`receive('waitingPermission',ask('r2'))`);
  await wait(900);
  p = await place();
  assert(
    p.buttonsTop >= p.top - 0.5 && p.buttonsBottom <= p.bottom + 0.5,
    `a new ask shows its buttons again: ${JSON.stringify(p)}`,
  );

  // Answered: the page follows the session down again.
  await evaluate(`receive('generating')`);
  await wait(900);
  p = await place();
  assert.equal(p.buttonsTop, null);
  assert(p.end - p.scrollTop < 2, `following resumes after the answer: ${JSON.stringify(p)}`);
  console.log("card reveal: ok");
}
