// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkMusicSources({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?music` });
  const wait = async (expression) => {
    for (let i = 0; i < 100; i++) {
      if (await evaluate(expression)) return;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    assert.fail(`Timed out: ${expression}`);
  };
  const frames = () => evaluate(`(async()=>{for(let i=0;i<8;i++)await new Promise(requestAnimationFrame)})()`);
  await wait(`!!document.querySelector('.m-strip')`);
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.music=(await import('/src/music/store.ts')).useMusic;
    window.bridge=(await import('/src/shared/bridge.ts')).bridge;
    window.preferences=await import('/src/shared/prefs.ts');
    window.fixture={...music.getState().playback};
    window.sessions=[{id:'music',name:'Music App',isPlaying:true,isMusicApp:true,hasTrack:true},{id:'custom',name:'Custom Player',isPlaying:true,isMusicApp:false,hasTrack:true}];
    window.selectionCalls=[]; window.controlCalls=[]; window.volumeCalls=[]; window.revision=0;
    bridge.music.volume=async(source,level)=>{if(level!==undefined)volumeCalls.push([source,level]);return level??0.5;};
    bridge.music.players=async()=>sessions;
    bridge.music.command=async(command,source)=>{controlCalls.push([command,source]);return true;};
    bridge.music.seek=async(seconds,source)=>{controlCalls.push(['seek',source]);return true;};
    bridge.music.shuffle=async(on,source)=>{controlCalls.push(['shuffle',source]);return true;};
    bridge.music.selectSource=async source=>{
      selectionCalls.push(source);revision++;
      const p=sessions.find(s=>s.id===(source??'music'));
      music.getState().receive({sources:sessions,selectedSource:source,revision,playback:{...fixture,source:p.id,sourceName:p.name,title:p.name+' Track',isPlaying:true}});
    };
    music.getState().receive({sources:sessions,selectedSource:null,revision:0,playback:{...fixture,source:'music',sourceName:'Music App'}});
    island.getState().open('music');
  })()`);
  await wait("!!document.querySelector('[aria-label=\"Music options\"]')");
  const ensurePanel = async () => {
    if (!(await evaluate("!!document.querySelector('.m-volume-popover')")))
      await evaluate("document.querySelector('[aria-label=\"Music options\"]').click()");
    await wait('!!document.querySelector(\'[role="radiogroup"][aria-label="Music source"]\')');
    await frames();
  };
  const choose = async (name) => {
    await ensurePanel();
    if (name === "Include other players")
      await evaluate("document.querySelector('.m-other-players [role=switch]').click()");
    else
      await evaluate(
        "Array.from(document.querySelectorAll('.m-source-option')).find(e=>e.children[1]?.textContent===" +
          JSON.stringify(name) +
          ").click()",
      );
    await frames();
  };
  await ensurePanel();
  assert(
    await evaluate(
      "(()=>{const p=document.querySelector('.m-volume-popover').getBoundingClientRect(),f=document.querySelector('.island-frame').getBoundingClientRect();return p.top>=f.top && p.left>=f.left && p.right<=f.right+1 && p.bottom<=f.bottom+1})()",
    ),
    "The combined panel must fit inside the island",
  );
  assert(
    await evaluate(
      "document.querySelector('.m-player-options').getBoundingClientRect().top > document.querySelector('.m-volume-popover input[type=range]').getBoundingClientRect().bottom",
    ),
    "Player options must appear below Volume",
  );
  await evaluate("document.querySelector('.m-preferred-trigger').click()");
  await wait("!!document.querySelector('.m-preferred-menu')");
  const clip = await evaluate(
    "(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return {x:Math.max(0,r.left-12),y:Math.max(0,r.top-12),width:r.width+24,height:r.height+24,scale:2};})()",
  );
  const capture = await send("Page.captureScreenshot", { format: "png", clip });
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(resolve("../build/music-options.png"), Buffer.from(capture.data, "base64"));
  await evaluate("document.querySelector('.m-preferred-trigger').click()");
  await choose("Custom Player");
  assert.equal(await evaluate(`selectionCalls.at(-1)`), "custom");
  assert.equal(await evaluate(`document.querySelector('.m-title').textContent`), "Custom Player Track");
  await evaluate(
    `document.querySelector('[aria-label="Next"]').click(); music.getState().seek(20); music.getState().shuffle(true)`,
  );
  await frames();
  assert.deepEqual(await evaluate(`controlCalls`), [
    ["next", "custom"],
    ["seek", "custom"],
    ["shuffle", "custom"],
  ]);
  await evaluate(`island.getState().collapse()`);
  await wait(`!!document.querySelector('.m-strip')`);
  assert.equal(await evaluate(`music.getState().playback.source`), "custom");
  await evaluate(`island.getState().open('music')`);
  await frames();
  await ensurePanel();
  await evaluate(
    `music.getState().receive({sources:[sessions[0]],selectedSource:'custom',revision,playback:{...fixture,source:'music',sourceName:'Music App'}})`,
  );
  await frames();
  assert(await evaluate(`document.querySelector('.m-source-status')?.textContent.includes('Automatic fallback')`));
  assert.equal(await evaluate(`music.getState().selectedSource`), "custom");
  await evaluate(
    `music.getState().receive({sources:sessions,selectedSource:'custom',revision,playback:{...fixture,source:'custom',sourceName:'Custom Player',title:'Restored player'}})`,
  );
  await frames();
  assert.equal(await evaluate(`document.querySelector('.m-title').textContent`), "Restored player");
  await choose("Automatic");
  assert.equal(await evaluate(`music.getState().selectedSource`), null);
  await choose("Include other players");
  assert.equal(await evaluate(`(async()=> (await preferences.loadPrefs()).island.includeOtherPlayers)()`), false);

  await ensurePanel();
  await evaluate("document.querySelector('.m-preferred-trigger').click()");
  await wait("!!document.querySelector('.m-preferred-menu')");
  await evaluate(
    "Array.from(document.querySelectorAll('.m-preferred-option')).find(option=>option.textContent==='Custom Player').click()",
  );
  await wait("preferences.loadPrefs().then(p=>p.island.preferredPlayer==='custom')");
  await evaluate("island.getState().collapse()");
  await frames();
  await evaluate("island.getState().open('music')");
  await frames();
  await ensurePanel();
  assert.equal(await evaluate("document.querySelector('.m-preferred-trigger').textContent"), "Custom Player");
  assert.equal(
    await evaluate("document.querySelector('.m-other-players [role=switch]').getAttribute('aria-checked')==='true'"),
    false,
  );
  await evaluate(
    "document.querySelector('.m-preferred-trigger').focus(); document.querySelector('.m-preferred-trigger').click()",
  );
  await wait("!!document.querySelector('.m-preferred-menu')");
  await send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "ArrowDown",
    code: "ArrowDown",
    windowsVirtualKeyCode: 40,
  });
  await send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "ArrowDown",
    code: "ArrowDown",
    windowsVirtualKeyCode: 40,
  });
  await frames();
  await send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Enter",
    code: "Enter",
    text: "\r",
    windowsVirtualKeyCode: 13,
  });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await wait("preferences.loadPrefs().then(p=>p.island.preferredPlayer==='music')");
  await evaluate("document.querySelector('.m-preferred-trigger').click()");
  await wait("!!document.querySelector('.m-preferred-menu')");
  assert.equal(await evaluate("getComputedStyle(document.querySelector('.m-preferred-menu')).borderRadius"), "12px");
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await wait("!document.querySelector('.m-preferred-menu')");
  assert(
    await evaluate("!!document.querySelector('.m-volume-popover')"),
    "Escape closes the picker before the options panel",
  );
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await wait("!document.querySelector('.m-volume-popover')");
  await evaluate(
    `bridge.music.selectSource=async()=>{throw Error('closed');}; bridge.music.now=async()=>({sources:sessions,playback:{...fixture,source:'music'},revision,selectedSource:null})`,
  );
  await choose("Custom Player");
  assert(await evaluate(`document.querySelector('.m-source-status')?.textContent.includes('Could not change')`));
  assert.equal(await evaluate(`music.getState().playback.source`), "music");
  await ensurePanel();
  await wait("document.querySelector('.m-volume-popover input[type=range]').disabled === false");
  await evaluate(
    "(()=>{const input=document.querySelector('.m-volume-popover input[type=range]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'35');input.dispatchEvent(new Event('input',{bubbles:true}));})()",
  );
  await wait("volumeCalls.some(([source,level])=>source==='music' && level===0.35)");
  await evaluate("music.getState().receive({sources:[],playback:null,selectedSource:null,revision:++revision})");
  await frames();
  assert(
    await evaluate(
      "!!document.querySelector('.m-player-options') && !document.querySelector('[aria-label=\"Music options\"]').disabled",
    ),
    "Options must remain available without playback",
  );
  console.log(
    "Music sources passed: dynamic players, manual/Automatic, command targets, strip, unavailable fallback, restored sessions, persisted preferences, menu Escape and errors.",
  );
}
