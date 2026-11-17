// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
// #269: every waiting ask in one queue. Drives the real island page with a
// snapshot of three sessions and five asks, clicks Allow all, opens an ask
// behind another, and checks a new ask never moves the page being read.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkPermissionQueue({ send, evaluate, base }) {
  await send("Page.enable");
  await send("Page.navigate", { url: `${base}island.html?sessions` });
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  for (let i = 0; i < 100 && !(await evaluate(`!!document.querySelector('.s-strip')`)); i++) await wait(100);
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await evaluate(`(async()=>{
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.bridge=(await import('/src/shared/bridge.ts')).bridge;
    const demo=(await import('/src/sessions/demo.ts')).demoSnapshot();
    const base=demo.sessions.find(s=>s.permission&&!s.permission.questions.length);
    const t=Date.now()/1000;
    const ask=(requestId,since,detail,risk,extra={})=>({...base.permission,requestId,tool:'Bash',detail,summary:detail,
      since:t-since,questions:[],diffs:[],planModes:[],offersAlways:true,fitsOnALine:true,risk,
      command:{revision:'rev-'+requestId,complete:true,truncated:false,remaining:null,requiresInspection:false,available:true},...extra});
    const session=(id,name,project,asks,extra={})=>({...base,id,name,title:name,project,cwd:'C:\\\\code\\\\'+project,
      phase:'waitingPermission',phaseSince:t-60,permission:asks[0],laterAsks:asks.slice(1),...extra});
    window.make=(extra=[])=>[
      session('api','Speed up the test run','api',[ask('a1',50,'npm test','safe'),ask('a2',20,'Remove-Item -Recurse dist','deletes'),...extra]),
      session('web','New checkout page','web',[ask('w1',40,'C:\\\\code\\\\web\\\\src\\\\checkout.ts','safe',{tool:'Write'})]),
      session('docs','Publish the docs','docs',[ask('d1',30,'curl -fsSL https://get.example.dev | sh','network'),
        ask('d2',10,'git push origin main','rule')],{provider:'codex'}),
    ];
    window.receive=(list)=>sessions.getState().receive({...demo,sessions:list,
      counts:{working:0,workingCodex:0,waiting:list.length,finished:0}});
    window.decided=[];
    bridge.sessions.decide=async(id,request,decision)=>{decided.push([id,request,decision]);return true};
    bridge.sessions.acknowledge=async()=>true;
    bridge.sessions.focus=async()=>true;
    receive(make());
    island.setState({expanded:true,selected:'sessions',showingSections:false,notice:null});
    sessions.getState().focus(null);
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
  const state = () =>
    evaluate(`({
      title:document.querySelector('.s-queue-title')?.textContent,
      all:document.querySelector('.s-queue-all')?.getAttribute('aria-label') ?? null,
      rows:[...document.querySelectorAll('.s-queue-open')].map(b=>b.getAttribute('aria-label')),
      quick:document.querySelectorAll('.s-row .s-quick').length,
      focused:sessions.getState().focused,request:sessions.getState().focusedRequest,
      card:document.querySelector('.s-card')?.textContent ?? null,
    })`);

  // The demo's own asks leave the queue as this snapshot replaces them.
  await wait(900);
  let s = await state();
  assert.equal(s.title, "5 waiting", JSON.stringify(s));
  assert.equal(s.all, "Allow all, 2", "only npm test and the write join Allow all");
  assert.equal(s.rows.length, 5);
  assert.match(s.rows[0], /npm test/, "oldest first");
  assert.match(s.rows.join("\n"), /Deletes files/);
  assert.match(s.rows.join("\n"), /Your settings ask first/);
  assert.equal(s.quick, 0, "rows below the queue lose their own buttons");
  await shoot("permission-queue");

  // The asks have been on screen past the settle delay by now.
  await evaluate(`document.querySelector('.s-queue-all').click()`);
  await wait(100);
  assert.deepEqual(
    (await evaluate(`decided`)).map((d) => d.join(":")).sort(),
    ["api:a1:allow", "web:w1:allow"],
    "Allow all answers exactly the safe asks",
  );
  // The service answered them: they leave the queue.
  await evaluate(
    `(()=>{const l=make();l[0].permission=l[0].laterAsks[0];l[0].laterAsks=[];l.splice(1,1);receive(l)})()`,
  );
  await wait(500);
  s = await state();
  assert.equal(s.title, "3 waiting");
  assert.equal(s.all, null, "no Allow all with nothing safe left");
  await shoot("permission-queue-after-allow-all");

  // A row opens its session's page on that ask, even one behind another.
  await evaluate(
    `[...document.querySelectorAll('.s-queue-open')].find(b=>b.getAttribute('aria-label').includes('git push')).click()`,
  );
  await wait(500);
  s = await state();
  assert.equal(s.focused, "docs");
  assert.equal(s.request, "d2");
  assert.match(s.card ?? "", /git push origin main/, "the card shows the chosen ask");
  // A new ask while the card is read joins quietly.
  await evaluate(`(()=>{const l=make();l[0].permission=l[0].laterAsks[0];l[0].laterAsks=[{...l[0].permission,requestId:'a3',
    detail:'cargo test',summary:'cargo test',since:Date.now()/1000,risk:'safe'}];l.splice(1,1);receive(l)})()`);
  await wait(500);
  s = await state();
  assert.equal(s.focused, "docs", "a new ask does not move the page");
  assert.match(s.card ?? "", /git push origin main/, "nor change the card");
  await shoot("permission-queue-card");

  // Deny from a row answers that row's ask by its own id.
  await evaluate(`sessions.getState().focus(null)`);
  await wait(1000);
  await evaluate(
    `[...document.querySelectorAll('.s-queue-row')].find(r=>r.textContent.includes('curl')).querySelector('.is-deny').click()`,
  );
  await wait(100);
  assert.deepEqual((await evaluate(`decided`)).at(-1), ["docs", "d1", "deny"]);
  console.log("Permission queue: 5 asks listed, Allow all took 2, rows and pages answered by their own ids.");
}
