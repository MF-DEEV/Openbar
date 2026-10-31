// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";

// The selection overlay in a browser, on its test picture: real pointer and key
// events through CDP, what would be committed read from console.info.
export async function checkCaptureControls({ send, evaluate, base }) {
  const load = async (query) => {
    await send("Page.navigate", { url: `${base}capture.html${query}` });
    for (let i = 0; i < 100; i++) {
      if (await evaluate(`!!document.querySelector('.cp-surface')`)) break;
      await new Promise((r) => setTimeout(r, 100));
    }
    await evaluate(
      `(()=>{window.done=[];const info=console.info;console.info=(tag,value)=>{if(tag==='capture')done.push(JSON.parse(value));else info(tag,value)}})()`,
    );
  };
  const wait = (ms = 250) => evaluate(`new Promise(r=>setTimeout(r,${ms}))`);
  const key = async (k) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: k, text: k.length === 1 ? k : undefined });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: k });
    await wait(100);
  };
  const move = (x, y) => send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  const press = async (x, y, type) =>
    send("Input.dispatchMouseEvent", {
      type,
      x,
      y,
      button: "left",
      buttons: type === "mousePressed" ? 1 : 0,
      clickCount: 1,
    });
  const middle = await evaluate(`innerWidth/2`);

  await load("?last");
  await move(100, 400);
  await wait();
  assert(await evaluate(`!!document.querySelector('.cc-island')`), "The controls sit at the top");
  assert(!(await evaluate(`document.querySelector('.cc-island').classList.contains('is-open')`)), "and start compact");

  // The hint names the keys; keys switch without opening the controls.
  assert((await evaluate(`document.querySelector('.cp-hint').innerText`)).includes("R repeats"));
  await key("4");
  assert(await evaluate(`!!document.querySelector('.cp-surface.is-color')`), "4 is the color picker");
  assert(await evaluate(`!document.querySelector('.cc-key')`), "Color takes no area, so no R");
  await key("r");
  assert.deepEqual(await evaluate(`done`), [], "R does nothing on color");
  await key("3");
  assert(await evaluate(`!!document.querySelector('.cp-surface.is-area')`), "3 is text, an area tool");

  // Pointer opens them; a tile switches; pressing them starts no area.
  await move(middle, 10);
  await wait(350);
  assert(await evaluate(`document.querySelector('.cc-island').classList.contains('is-open')`), "Hover opens");
  const tile = await evaluate(
    `(()=>{const b=[...document.querySelectorAll('.cc-tile')].find(b=>b.innerText.includes('Screenshot')).getBoundingClientRect();return {x:b.left+b.width/2,y:b.top+b.height/2}})()`,
  );
  await move(tile.x, tile.y);
  await press(tile.x, tile.y, "mousePressed");
  await press(tile.x, tile.y, "mouseReleased");
  await wait();
  assert.equal(await evaluate(`document.querySelector('.cc-tile.is-active').innerText`), "Screenshot");
  assert.deepEqual(await evaluate(`done`), [], "Clicking a control selects nothing underneath");

  // Dragging commits with the tool chosen, and the controls step aside.
  await key("3");
  await move(300, 400);
  await wait(3200);
  assert(
    !(await evaluate(`document.querySelector('.cc-island').classList.contains('is-open')`)),
    "Leaving collapses after 3 s",
  );
  await press(300, 400, "mousePressed");
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 500, y: 600, button: "left", buttons: 1 });
  await wait(100);
  assert(
    await evaluate(`document.querySelector('.cc-island').classList.contains('is-passive')`),
    "A drag makes them passive",
  );
  await press(500, 600, "mouseReleased");
  await wait(100);
  let picks = await evaluate(`done`);
  assert.equal(picks.length, 1);
  assert.equal(picks[0].tool, "text", "The area goes to the tool chosen");
  assert.equal(picks[0].pick.kind, "area");

  // R repeats the last area.
  await load("?last");
  await key("r");
  picks = await evaluate(`done`);
  assert.equal(picks[0].tool, "screenshot");
  assert(picks[0].pick.width > 0, "R takes the last area");

  // No last area, no R.
  await load("");
  await key("r");
  assert.deepEqual(await evaluate(`done`), [], "Nothing to repeat yet");
  assert(!(await evaluate(`document.querySelector('.cp-hint').innerText`)).includes("R repeats"));

  // The X cancels.
  await move(100, 400);
  await wait(50);
  await move(middle, 10);
  await wait(300);
  const close = await evaluate(
    `(()=>{const b=document.querySelector('.cc-icon[aria-label=Close]').getBoundingClientRect();return {x:b.left+b.width/2,y:b.top+b.height/2}})()`,
  );
  await move(close.x, close.y);
  await press(close.x, close.y, "mousePressed");
  await press(close.x, close.y, "mouseReleased");
  await wait();
  assert.deepEqual((await evaluate(`done`))[0].pick, null, "Close cancels");

  // The recording tool's words follow the language.
  await evaluate(`import('/src/shared/prefs.ts').then((prefs)=>prefs.setPrefs({island:{language:'de'}}))`);
  await load("");
  await key("2");
  await move(100, 400);
  await wait(50);
  await move(middle, 10);
  await wait(400);
  const german = await evaluate(`document.querySelector('.cc-audio')?.innerText ?? ''`);
  assert(german.includes("Systemaudio"), `The switches are translated: ${german}`);
  assert((await evaluate(`document.querySelector('.cp-hint').innerText`)).includes("1 bis 4"), "and so is the hint");
  await evaluate(`import('/src/shared/prefs.ts').then((prefs)=>prefs.setPrefs({island:{language:'en-US'}}))`);

  // Watch: no controls, no tools.
  await load("?watch&last");
  assert(await evaluate(`!document.querySelector('.cc-island')`), "Watch shows no controls");
  await key("4");
  assert(await evaluate(`!!document.querySelector('.cp-surface.is-area')`), "and its keys don't switch");
}
