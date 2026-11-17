// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkIslandContextMenu({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.island-frame')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async () => {
    window.island = (await import('/src/island/store.ts')).useIsland;
    window.bridge = (await import('/src/shared/bridge.ts')).bridge;
    window.menuCalls = {settings: 0, capture: [], focus: []};
    bridge.openSettings = async () => { menuCalls.settings++; };
    bridge.setCapture = async value => { menuCalls.capture.push(value); };
    bridge.setFocusable = async (value, take) => { menuCalls.focus.push([value,take]); };
    island.getState().collapse();
    island.setState(s => ({settings: {...s.settings, opening:'click', hiddenModules:['camera'], returnHome:false}}));
    await document.fonts.ready;
  })()`);
  const settle = () => evaluate(`new Promise(r => setTimeout(r, 180))`);
  await settle();
  const state = () =>
    evaluate(`({expanded:island.getState().expanded,pinned:island.getState().pinned,held:island.getState().held,
    sections:island.getState().showingSections,selected:island.getState().selected})`);
  const surfaceColour = () => evaluate(`getComputedStyle(document.querySelector('.island-backdrop path')).fill`);
  const key = async (key, code = key) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key, code });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key, code });
    await settle();
  };
  const click = async (x, y, button = "left") => {
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button, clickCount: 1 });
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button, clickCount: 1 });
    await settle();
  };
  const open = async () => {
    const point = await evaluate(`(() => {
      const r=document.querySelector('.island-frame').getBoundingClientRect();
      return {x:r.left+r.width/2,y:r.top+Math.min(16,r.height/2)};
    })()`);
    await click(point.x, point.y, "right");
    assert.equal(await evaluate(`document.querySelectorAll('.u-menu').length`), 1, "Right-click opens one menu");
  };
  const choose = async (label) => {
    const point = await evaluate(`(() => {
      const e=[...document.querySelectorAll('.u-menu-item')].find(e=>e.textContent.trim()===${JSON.stringify(label)});
      if(!e) throw new Error('Missing menu item: '+${JSON.stringify(label)});
      const r=e.getBoundingClientRect(); return {x:r.left+r.width/2,y:r.top+r.height/2};
    })()`);
    await click(point.x, point.y);
    assert.equal(await evaluate(`!!document.querySelector('.u-menu')`), false, "Selecting an action closes the menu");
  };
  const labels = () => evaluate(`[...document.querySelectorAll('.u-menu-item')].map(e=>e.textContent.trim())`);

  // Settle the ordinary pointer-hover pulse before measuring the context-menu interaction.
  const hover = await evaluate(
    `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+r.height/2}})()`,
  );
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", ...hover });
  await settle();
  // Closed island keeps its compact dimensions and receives keyboard focus without expanding.
  assert.equal(await surfaceColour(), "rgb(0, 0, 0)", "The Mac's standard compact island is opaque black");
  const before = await evaluate(
    `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return [r.width,r.height]})()`,
  );
  await open();
  assert.equal(await surfaceColour(), "rgb(0, 0, 0)", "Right-clicking does not recolour the island");
  const style = await evaluate(`(() => {
    const menu = document.querySelector('.n-context-menu');
    const row = menu.querySelector('.u-menu-item');
    return {font:getComputedStyle(menu).fontSize, outline:getComputedStyle(menu).outlineStyle, row:row.getBoundingClientRect().height,
      check:getComputedStyle(menu.querySelector('.u-menu-check')).display,
      backdrop:getComputedStyle(document.querySelector('.u-menu-backdrop')).backgroundColor};
  })()`);
  assert.deepEqual(style, { font: "13px", outline: "none", row: 22, check: "none", backdrop: "rgba(0, 0, 0, 0)" });
  assert.deepEqual(await labels(), ["Sections", "Pin", "Settings", "Collapse"]);
  assert.equal((await state()).expanded, false);
  assert.deepEqual(
    await evaluate(
      `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return [r.width,r.height]})()`,
    ),
    before,
  );
  assert.equal(await evaluate(`menuCalls.capture.at(-1)`), true);
  assert.deepEqual(await evaluate(`menuCalls.focus.at(-1)`), [true, true]);
  await key("Escape");
  assert.equal(await evaluate(`!!document.querySelector('.u-menu')`), false);
  assert.equal((await state()).held, false);
  assert.equal(await evaluate(`menuCalls.capture.at(-1)`), false);
  assert.deepEqual(await evaluate(`menuCalls.focus.at(-1)`), [false, false]);

  await open();
  await choose("Pin");
  assert.equal((await state()).pinned, true);
  await open();
  assert((await labels()).includes("Unpin"));
  await choose("Unpin");
  assert.equal((await state()).pinned, false);
  await open();
  await evaluate(`island.getState().setPinned(true)`);
  await settle();
  assert((await labels()).includes("Unpin"), "Pin label follows external state changes while open");
  await key("ArrowDown");
  await key("ArrowDown");
  await key(" ", "Space");
  assert.equal((await state()).pinned, false, "Space selects the active menu item");

  await open();
  await choose("Sections");
  assert.equal(await surfaceColour(), "rgb(0, 0, 0)", "The expanded island has the same black as the compact island");
  assert.equal((await state()).sections, true);
  assert.equal(
    await evaluate(`[...document.querySelectorAll('.n-tile-title')].some(e=>e.textContent==='Camera')`),
    false,
  );
  await open();
  assert.equal((await labels())[0], "Back");
  assert.equal(await surfaceColour(), "rgb(0, 0, 0)", "The expanded island stays black beneath its context menu");
  await key("Escape");
  assert.equal((await state()).sections, true, "Escape dismisses menu before the gallery");
  await evaluate(
    `[...document.querySelectorAll('.n-tile')].find(e=>e.querySelector('.n-tile-title').textContent==='Now Playing').click()`,
  );
  await settle();
  assert.equal((await state()).selected, "music");
  assert.equal((await state()).sections, false);

  // A music transport click remains the real music action after the context menu closes.
  await evaluate(`(async () => {
    window.music = (await import('/src/music/store.ts')).useMusic;
    music.getState().receive((await import('/src/music/demo.ts')).demoNowPlaying());
    menuCalls.music=[];
    bridge.music.command=async (...args)=>{ menuCalls.music.push(args); return true; };
  })()`);
  await settle();
  await open();
  await key("Escape");
  await evaluate(`document.querySelector('.m-transport button[aria-label="Pause"]').click()`);
  await settle();
  assert.equal(await evaluate(`menuCalls.music[0][0]`), "toggle");
  await evaluate(`music.getState().receive({playback:null,sources:[]})`);
  await settle();
  await open();
  const expanded = await state();
  await click(10, 900);
  assert.equal(await evaluate(`!!document.querySelector('.u-menu')`), false);
  assert.equal((await state()).expanded, expanded.expanded, "Outside click dismisses only the menu");

  await open();
  await choose("Settings");
  assert.equal(await evaluate(`menuCalls.settings`), 1, "Invokes the settings bridge");
  assert.equal((await state()).expanded, false);

  // Arrow keys and Enter operate the menu on both expanded and compact surfaces.
  await evaluate(`island.getState().open('music')`);
  await settle();
  await open();
  await key("ArrowUp");
  await key("Enter");
  assert.equal((await state()).expanded, false, "Keyboard Collapse uses the existing state machine");

  for (const [x, y] of [
    [0, 0],
    [1439, 0],
    [1439, 999],
    [0, 999],
  ]) {
    const prevented = await evaluate(`(() => {
      const e=new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:${x},clientY:${y}});
      document.querySelector('.n-activation, .island-frame').dispatchEvent(e);
      return e.defaultPrevented;
    })()`);
    assert(prevented, "Suppresses browser context menu");
    await settle();
    const fits = await evaluate(`(() => {const r=document.querySelector('.u-menu').getBoundingClientRect();
      return r.left>=0 && r.top>=0 && r.right<=innerWidth && r.bottom<=innerHeight;})()`);
    assert(fits, `Menu stays visible at ${x},${y}`);
    await key("Escape");
  }

  // Preserve page-specific right-click handlers.
  await evaluate(
    `document.querySelector('.n-activation').addEventListener('contextmenu',e=>e.preventDefault(),{once:true})`,
  );
  const prevented = await evaluate(
    `!document.querySelector('.n-activation').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true}))`,
  );
  assert(prevented);
  assert.equal(await evaluate(`!!document.querySelector('.u-menu')`), false);

  // Left-click remains the original island action, and hover-open does not dismiss the menu.
  await evaluate(`document.querySelector('.n-activation').click()`);
  await settle();
  assert.equal((await state()).expanded, true);
  await evaluate(`island.setState({openedByHover:true,clickedSinceOpening:false})`);
  await open();
  await evaluate(`island.getState().pointerExit()`);
  await evaluate(`new Promise(r=>setTimeout(r,800))`);
  assert.equal((await state()).expanded, true);
  await choose("Collapse");
  assert.equal((await state()).expanded, false);
  assert.equal((await state()).held, false);

  await evaluate(`island.setState(s=>({settings:{...s.settings,silhouette:'capsule'}}))`);
  await settle();
  await open();
  await key("Escape");
  assert.equal(await evaluate(`!!document.querySelector('.u-menu')`), false, "Capsule supports the same menu");
  await open();
  await evaluate(`window.dispatchEvent(new Event('blur'))`);
  await settle();
  assert.equal(await evaluate(`!!document.querySelector('.u-menu')`), false, "Losing focus releases the menu");
  assert.equal(await evaluate(`menuCalls.capture.at(-1)`), false);
  await open();
  await evaluate(`window.dispatchEvent(new Event('resize'))`);
  await settle();
  assert.equal(await evaluate(`!!document.querySelector('.u-menu')`), false, "Resize closes the menu");
  await evaluate(`island.setState(s=>({settings:{...s.settings,silhouette:'notch'}}))`);
  await settle();

  await open();
  await mkdir(resolve("../build"), { recursive: true });
  const shot = await send("Page.captureScreenshot", {
    format: "png",
    clip: { x: 360, y: 0, width: 720, height: 400, scale: 1.5 },
  });
  await writeFile(resolve("../build/island-context-menu.png"), Buffer.from(shot.data, "base64"));
  await key("Escape");
  await evaluate(`island.getState().open('controls')`);
  await settle();
  await open();
  const expandedShot = await send("Page.captureScreenshot", {
    format: "png",
    clip: { x: 360, y: 0, width: 720, height: 400, scale: 1.5 },
  });
  await writeFile(resolve("../build/island-context-menu-expanded.png"), Buffer.from(expandedShot.data, "base64"));
  await key("Escape");
  await evaluate(`island.getState().collapse()`);
  console.log(
    "Island context menu: actions, gallery navigation, current pin state, Escape, outside click, keyboard, screen edges, hover and page-menu precedence passed.",
  );
}
