// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkCalendar({ send, evaluate, base }) {
  const wait = async (expression) => {
    for (let i = 0; i < 150; i++) {
      if (await evaluate(expression)) return;
      await new Promise((r) => setTimeout(r, 60));
    }
    assert.fail("Timed out: " + expression);
  };
  await send("Page.navigate", { url: `${base}island.html` });
  await wait("!!document.querySelector('.n-activation')");
  await evaluate(`(async () => {
    window.cal = await import('/src/calendar/store.ts');
    window.island = (await import('/src/island/store.ts')).useIsland;
    window.bridge = (await import('/src/shared/bridge.ts')).bridge;
    window.rows = []; window.saveCalls = 0; window.failSave = false;
    window.snapshot = () => ({access:'allowed',systemAccess:'unavailable',localError:null,events:rows,current:rows,calendars:[{id:'openbar-local',title:'Openbar',sourceId:'openbar-local',source:'Openbar',color:'#ff5c63'}],firstWeekday:1,minimumDaysInFirstWeek:4});
    bridge.calendar.showRange = async () => snapshot();
    bridge.calendar.saveEvent = async draft => {
      saveCalls++; if(failSave) throw 'Test write failure';
      const id=draft.id || 'local-test';
      const row={...draft,id,calendar:'Openbar',calendarId:'openbar-local',localId:id,instanceStart:null,countdownKey:id};
      rows=rows.filter(r=>r.id!==id).concat(row); return snapshot();
    };
    bridge.calendar.deleteEvent = async id => {rows=rows.filter(r=>r.id!==id);return snapshot();};
    cal.receive(snapshot());
    cal.useCalendar.setState({showingMonth:false,selectedDay:null,focus:Date.now()});
    island.getState().open('calendar'); island.getState().setPinned(true);
    window.change = (selector,value) => {
      const input=document.querySelector(selector); const proto=input.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto,'value').set.call(input,value);
      input.dispatchEvent(new Event('input',{bubbles:true})); input.dispatchEvent(new Event('change',{bubbles:true}));
    };
  })()`);
  await wait("!!document.querySelector('.cal-empty')");
  assert.equal(
    await evaluate("!!document.querySelector('.cal-permission')"),
    false,
    "Local calendar works without Windows identity",
  );
  await evaluate("document.querySelector('.cal-empty .n-pill-button').click()");
  await wait("!!document.querySelector('.cal-editor')");
  await evaluate(
    `change('.cal-editor-title input','Design review');change('.cal-editor textarea','Review the new calendar and plan next steps.');change('.cal-editor input[maxlength="500"]','Studio');`,
  );
  await evaluate(`document.querySelector('.cal-editor-dates input').value`);
  await evaluate(
    `change('.cal-editor-dates label:last-child input', '2020-01-01T10:00');document.querySelector('.cal-editor').requestSubmit()`,
  );
  await wait("!!document.querySelector('[role=alert]')");
  assert.equal(await evaluate("saveCalls"), 0, "Invalid dates must not be sent to disk");
  await evaluate(
    `change('.cal-editor-dates label:last-child input', (new Date(Date.now()+86400000)).toLocaleString('sv-SE').slice(0,16).replace(' ','T'))`,
  );
  const capture = async (name) => {
    await new Promise((r) => setTimeout(r, 350));
    const rect = await evaluate(
      `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return {x:Math.max(0,r.x-4),y:Math.max(0,r.y),width:r.width+8,height:r.height+8,scale:2}})()`,
    );
    const shot = await send("Page.captureScreenshot", { format: "png", clip: rect });
    await mkdir(resolve("../build"), { recursive: true });
    await writeFile(resolve(`../build/${name}.png`), Buffer.from(shot.data, "base64"));
  };
  await capture("calendar-editor");
  await evaluate("failSave=true;document.querySelector('.cal-editor').requestSubmit()");
  await wait("document.querySelector('[role=alert]')?.textContent==='Test write failure'");
  assert.equal(
    await evaluate("document.querySelector('.cal-editor-title input').value"),
    "Design review",
    "Failed save preserves the draft",
  );
  await evaluate("failSave=false;document.querySelector('.cal-editor').requestSubmit()");
  await wait("!!document.querySelector('.cal-card')");
  assert.equal(
    await evaluate("document.querySelector('.cal-card-description').textContent"),
    "Review the new calendar and plan next steps.",
  );
  await capture("calendar-agenda");
  await evaluate("document.querySelector('.cal-card').click()");
  await wait("!!document.querySelector('.cal-editor')");
  await evaluate(
    "change('.cal-editor-title input','Updated review');document.querySelector('.cal-editor').requestSubmit()",
  );
  await wait("document.querySelector('.cal-card-title')?.textContent==='Updated review'");
  await evaluate("document.querySelector('.cal-card').click()");
  await wait("!!document.querySelector('.cal-delete')");
  await evaluate("document.querySelector('.cal-delete').click()");
  assert.equal(await evaluate("rows.length"), 1, "Deletion needs confirmation");
  await evaluate("document.querySelector('.cal-delete').click()");
  await wait("!!document.querySelector('.cal-empty')");
  assert.equal(await evaluate("rows.length"), 0);
  // All-day editing and Escape work in the compact layout too.
  await evaluate(`document.querySelector('[aria-label="New event"]').click()`);
  await wait("!!document.querySelector('.cal-editor')");
  await evaluate("document.querySelector('.cal-all-day input').click()");
  assert.equal(await evaluate("document.querySelector('.cal-editor-dates input').type"), "date");
  await evaluate("(async()=>{(await import('/src/island/pageLayer.ts')).closeTopLayer()})()");
  await wait("!document.querySelector('.cal-editor')");
  console.log(
    "Calendar UI: creation, validation, save failure, description, editing, confirmed deletion, all-day and Escape passed.",
  );
  await evaluate("cal.useCalendar.setState({showingMonth:true})");
  await wait("!!document.querySelector('.cal-month-scroll')");
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await evaluate(
    "window.monthBefore=cal.useCalendar.getState().focus;window.scrollSamples=[];window.sampleScroll=true;window.sample=()=>{const e=document.querySelector('.cal-month-scroll');if(e)scrollSamples.push(e.scrollTop);if(sampleScroll)requestAnimationFrame(sample)};requestAnimationFrame(sample)",
  );
  const rect = await evaluate(
    "(()=>{const r=document.querySelector('.cal-month-scroll').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()",
  );
  await send("Input.dispatchMouseEvent", { type: "mouseWheel", ...rect, deltaX: 0, deltaY: 220 });
  await wait("cal.useCalendar.getState().focus>monthBefore");
  await evaluate("sampleScroll=false");
  assert((await evaluate("new Set(scrollSamples.map(Math.round)).size")) > 4, "Wheel movement has intermediate frames");
  assert.equal(await evaluate("document.querySelectorAll('.cal-month-page').length"), 5, "Recycle a fixed five months");
  await capture("calendar-month-scroll");
  await evaluate("window.monthBefore=cal.useCalendar.getState().focus");
  await send("Input.dispatchMouseEvent", { type: "mouseWheel", ...rect, deltaX: 0, deltaY: -220 });
  await wait("cal.useCalendar.getState().focus<monthBefore");
  await evaluate(
    "document.querySelector('[aria-label=\"Next month\"]').click();window.expectedMonth=cal.useCalendar.getState().focus",
  );
  await wait(
    "Number(document.querySelectorAll('.cal-month-page')[2]?.dataset.month)===expectedMonth && Math.abs(document.querySelector('.cal-month-scroll').scrollTop-2*document.querySelector('.cal-month-scroll').clientHeight)<1",
  );
  const savedFocus = await evaluate("expectedMonth");
  await send("Page.reload");
  await wait("!!document.querySelector('.n-activation')");
  const restored = await evaluate(
    "(async()=>{const s=(await import('/src/calendar/store.ts')).useCalendar.getState();return {focus:s.focus,showingMonth:s.showingMonth}})()",
  );
  assert.equal(restored.focus, savedFocus);
  assert.equal(restored.showingMonth, true);
  console.log(
    "Month browsing: native smooth wheel down/up, bounded pages, arrows and place restored after reload passed.",
  );
}
