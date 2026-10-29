// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkMusicLyrics({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?music` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.m-strip')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  const frames = () => evaluate(`(async()=>{for(let i=0;i<8;i++)await new Promise(requestAnimationFrame)})()`);
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.music=(await import('/src/music/store.ts')).useMusic;
    window.lyrics=(await import('/src/music/lyricsStore.ts')).useLyrics;
    const original=window.fetch;
    window.fetch=(url,...args)=>String(url).startsWith('https://lrclib.net/')
      ? Promise.resolve(new Response(null,{status:404})) : original(url,...args);
    window.reading={...music.getState().playback,title:'Lyrics browser fixture',artist:'Test artist',album:'Test album',isPlaying:false,elapsed:20,duration:120,sampledAt:Date.now()/1000};
    music.getState().receive({playback:reading,sources:[]});
    island.setState({expanded:true,selected:'music',showingSections:false,inside:true});
  })()`);
  await frames();
  await evaluate(`document.querySelector('.m-lyrics-toggle').click()`);
  await frames();
  assert(await evaluate(`!!document.querySelector('.m-lyrics-empty')`));
  // Exercise the real file input handler with a local synthetic LRC (test code only).
  await evaluate(`(()=>{
    document.querySelector('[aria-label="Import .lrc lyrics"]').click();
    const source=Array.from({length:30},(_,i)=>'[00:'+String(i*2).padStart(2,'0')+']Test verse '+i).join('\\n');
    const transfer=new DataTransfer();transfer.items.add(new File([source],'fixture.lrc',{type:'text/plain'}));
    const input=document.querySelector('.m-lyrics-tools input');input.files=transfer.files;
    input.dispatchEvent(new Event('change',{bubbles:true}));
  })()`);
  await frames();
  assert.equal(await evaluate(`document.querySelector('.m-lyric.is-active')?.textContent`), "Test verse 10");
  assert(
    await evaluate(`document.querySelector('.m-lyrics-scroll').scrollTop > 0`),
    "Active verse should scroll into view",
  );
  await evaluate(`document.querySelector('[aria-label="Lyrics later by 0.5 seconds"]').click()`);
  await frames();
  assert.equal(await evaluate(`document.querySelector('.m-lyric.is-active')?.textContent`), "Test verse 9");
  assert.equal(await evaluate(`lyrics.getState().offset`), 0.5);
  await evaluate(`music.getState().receive({playback:{...reading,elapsed:4},sources:[]})`);
  await frames();
  assert.equal(await evaluate(`document.querySelector('.m-lyric.is-active')?.textContent`), "Test verse 1");
  const paused = await evaluate(`document.querySelector('.m-lyric.is-active')?.textContent`);
  await new Promise((r) => setTimeout(r, 600));
  assert.equal(await evaluate(`document.querySelector('.m-lyric.is-active')?.textContent`), paused);
  await evaluate(
    `music.getState().receive({playback:{...reading,elapsed:4.4,isPlaying:true,sampledAt:Date.now()/1000},sources:[]})`,
  );
  await new Promise((r) => setTimeout(r, 250));
  assert.equal(await evaluate(`document.querySelector('.m-lyric.is-active')?.textContent`), "Test verse 2");
  await evaluate(`music.getState().receive({playback:{...reading,elapsed:20},sources:[]})`);
  await frames();
  const layout = await evaluate(`(()=>{
    const panel=document.querySelector('.m-lyrics').getBoundingClientRect();
    const layer=document.querySelector('.island-layer').getBoundingClientRect();
    return {panelBottom:panel.bottom,layerBottom:layer.bottom,width:panel.width};
  })()`);
  assert(layout.panelBottom <= layout.layerBottom && layout.width > 0, JSON.stringify(layout));
  await mkdir(resolve("../build"), { recursive: true });
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../build/music-lyrics.png"), Buffer.from(screenshot.data, "base64"));
  // Real wheel input must leave the user in control across verse changes.
  const viewport = await evaluate(`(()=>{
    const r=document.querySelector('.m-lyrics-scroll').getBoundingClientRect();
    return {x:r.x+r.width/2,y:r.y+r.height/2};
  })()`);
  await send("Input.dispatchMouseEvent", { type: "mouseWheel", ...viewport, deltaX: 0, deltaY: 600 });
  await frames();
  assert(
    await evaluate(`!!document.querySelector('.m-lyrics-sync')`),
    "Browsing far from the active verse should offer Sync",
  );
  const browsedPosition = await evaluate(`document.querySelector('.m-lyrics-scroll').scrollTop`);
  await evaluate(`music.getState().receive({playback:{...reading,elapsed:24},sources:[]})`);
  await frames();
  assert.equal(
    await evaluate(`document.querySelector('.m-lyrics-scroll').scrollTop`),
    browsedPosition,
    "Playback updates must not snap manual scrolling back to the active verse",
  );
  const browsingScreenshot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../build/music-lyrics-browsing.png"), Buffer.from(browsingScreenshot.data, "base64"));
  await evaluate(`document.querySelector('.m-lyrics-sync').click()`);
  await frames();
  assert.equal(await evaluate(`!!document.querySelector('.m-lyrics-sync')`), false);
  assert(
    await evaluate(`(()=>{
    const list=document.querySelector('.m-lyrics-scroll').getBoundingClientRect();
    const line=document.querySelector('.m-lyric.is-active').getBoundingClientRect();
    return line.top>=list.top && line.bottom<=list.bottom;
  })()`),
    "Sync must return to the active verse",
  );
  await evaluate(`music.getState().receive({playback:{...reading,elapsed:28},sources:[]})`);
  await frames();
  assert.notEqual(await evaluate(`document.querySelector('.m-lyrics-scroll').scrollTop`), browsedPosition);
  await evaluate(`document.querySelector('.m-lyrics-scroll').focus()`);
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "End", code: "End", windowsVirtualKeyCode: 35 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "End", code: "End", windowsVirtualKeyCode: 35 });
  await new Promise((r) => setTimeout(r, 350));
  await frames();
  assert(
    await evaluate(`!!document.querySelector('.m-lyrics-sync')`),
    "Keyboard browsing should also offer synchronization: " +
      JSON.stringify(
        await evaluate(
          `(()=>{const e=document.querySelector('.m-lyrics-scroll');return {top:e.scrollTop,height:e.clientHeight,total:e.scrollHeight,focus:document.activeElement?.className}})()`,
        ),
      ),
  );
  await evaluate(`document.querySelector('.m-lyrics-sync').click()`);
  await frames();
  // Drag the scrollbar thumb, rather than changing scrollTop through JavaScript.
  const thumb = await evaluate(`(()=>{
    const e=document.querySelector('.m-lyrics-scroll'),r=e.getBoundingClientRect();
    const scale=r.height/e.clientHeight, height=Math.max(18,e.clientHeight*e.clientHeight/e.scrollHeight);
    return {x:r.right-4,y:r.top+(e.scrollTop/(e.scrollHeight-e.clientHeight)*(e.clientHeight-height)+height/2)*scale,bottom:r.bottom-12};
  })()`);
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: thumb.x, y: thumb.y });
  await send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: thumb.x,
    y: thumb.y,
    button: "left",
    clickCount: 1,
  });
  await send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: thumb.x,
    y: thumb.bottom,
    button: "left",
    buttons: 1,
  });
  await send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: thumb.x,
    y: thumb.bottom,
    button: "left",
    clickCount: 1,
  });
  await frames();
  assert(
    await evaluate(`!!document.querySelector('.m-lyrics-sync')`),
    "Dragging the scrollbar should allow manual browsing",
  );
  await evaluate(`(async()=>{
    const p=music.getState().playback;
    await lyrics.getState().select({...p,title:'Different fixture'});
    await lyrics.getState().select(p);
  })()`);
  assert.equal(await evaluate(`lyrics.getState().offset`), 0.5);
  assert.equal(await evaluate(`lyrics.getState().lyrics.lines.length`), 30);
  // Route clicks through the real music store and bridge, replacing only SMTC.
  await evaluate(`(async()=>{
    const {bridge}=await import('/src/shared/bridge.ts');
    window.testBridge=bridge;window.seeks=[];window.commands=[];
    bridge.music.seek=async(seconds)=>{
      seeks.push(seconds);
      music.getState().receive({playback:{...music.getState().playback,elapsed:seconds,sampledAt:Date.now()/1000},sources:[]});
      return true;
    };
    bridge.music.command=async(command)=>{
      commands.push(command);
      if(command==='toggle')music.getState().receive({playback:{...music.getState().playback,isPlaying:!music.getState().playback.isPlaying,sampledAt:Date.now()/1000},sources:[]});
      return true;
    };
    music.getState().receive({playback:{...reading,isPlaying:false},sources:[]});
  })()`);
  await frames();
  await evaluate(`document.querySelector('[data-line="22"]').click()`);
  await frames();
  assert.deepEqual(await evaluate(`seeks`), [44.5], "Verse clicks should seek with the positive timing adjustment");
  assert.deepEqual(await evaluate(`commands`), ["toggle"], "Clicking a verse while paused should resume playback");
  assert.equal(await evaluate(`document.querySelector('.m-lyric.is-active')?.dataset.line`), "22");
  assert.equal(await evaluate(`!!document.querySelector('.m-lyrics-sync')`), false);
  await evaluate(`document.querySelector('[data-line="5"]').focus()`);
  await send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Enter",
    code: "Enter",
    windowsVirtualKeyCode: 13,
    text: "\r",
  });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await frames();
  assert.deepEqual(await evaluate(`seeks`), [44.5, 10.5], "Keyboard activation should seek to the selected verse");
  assert.deepEqual(await evaluate(`commands`), ["toggle"], "Seeking while playing must keep playback running");
  await evaluate(`lyrics.getState().adjustOffset(-1);document.querySelector('[data-line="0"]').click()`);
  await frames();
  assert.equal(
    await evaluate(`seeks.at(-1)`),
    0,
    "Negative offsets must clamp the first verse to the start of playback",
  );
  await evaluate(
    `testBridge.music.seek=async(seconds)=>{seeks.push(seconds);return false};document.querySelector('[data-line="8"]').click()`,
  );
  await frames();
  assert.equal(await evaluate(`music.getState().failed`), true);
  assert.deepEqual(await evaluate(`commands`), ["toggle"], "A failed seek must not toggle playback");
  await evaluate(`music.getState().receive({playback:{...music.getState().playback,canSeek:false},sources:[]})`);
  await frames();
  const beforeDisabledClick = await evaluate(`seeks.length`);
  assert(await evaluate(`document.querySelector('[data-line="8"]').disabled`));
  await evaluate(`document.querySelector('[data-line="8"]').click()`);
  assert.equal(await evaluate(`seeks.length`), beforeDisabledClick, "Unsupported players must not receive verse seeks");
  console.log(
    "Music lyrics browser checks passed: file import, highlighting, manual scrolling, synchronization, verse clicks, keyboard seeking, offset, pause/resume, unsupported players, persistence and panel layout.",
  );
}
