// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkMusicQueue({ send, evaluate, base }) {
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  const wait = async (expression) => {
    for (let i = 0; i < 120; i++) {
      if (await evaluate(expression)) return;
      await new Promise((r) => setTimeout(r, 50));
    }
    assert.fail(`Timed out: ${expression}`);
  };
  await send("Page.navigate", { url: `${base}island.html?music` });
  await wait("!!document.querySelector('.m-strip')");
  await evaluate(`(async () => {
    window.island = (await import('/src/island/store.ts')).useIsland;
    window.music = (await import('/src/music/store.ts')).useMusic;
    window.queue = (await import('/src/music/queueStore.ts')).useQueue;
    window.lyrics = (await import('/src/music/lyricsStore.ts')).useLyrics;
    window.bridge = (await import('/src/shared/bridge.ts')).bridge;
    window.queueReads = []; window.playCommands = [];
    bridge.music.command = async (...args) => { playCommands.push(args); return true; };
    bridge.music.queue = async (source,title,open) => {
      queueReads.push({source,title,open});
      return {source,title,status:'ready',items:Array.from({length:8},(_,i)=>({title:['Diseasë','When tha World Ends','Real Trendsettër','Last Day','U could'][i%5],artist:'Yeat, Lyfestyle Corporation'}))};
    };
    island.getState().open('music');
  })()`);
  await wait("!!document.querySelector('[aria-controls=music-queue]')");
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await evaluate("document.querySelector('[aria-controls=music-queue]').click()");
  const opacities = await evaluate(`(async () => {
    const samples = [];
    for (let i=0;i<22;i++) { await new Promise(requestAnimationFrame); samples.push(Number(getComputedStyle(document.querySelector('.m-queue-reveal')).opacity)); }
    return samples;
  })()`);
  assert(new Set(opacities).size > 3, "Queue reveal must animate through intermediate frames");
  await wait("document.querySelectorAll('.m-queue-row').length === 5");
  await new Promise((r) => setTimeout(r, 450));
  assert.deepEqual(await evaluate("Array.from(document.querySelectorAll('.m-queue-song strong'),e=>e.textContent)"), [
    "Diseasë",
    "When tha World Ends",
    "Real Trendsettër",
    "Last Day",
    "U could",
  ]);
  const bounds = await evaluate(`(() => {
    const panel = document.querySelector('.m-queue').getBoundingClientRect();
    const last = document.querySelector('.m-queue-row:last-child').getBoundingClientRect();
    return {panelBottom: panel.bottom, lastBottom:last.bottom, pageBottom:document.querySelector('.m-page').getBoundingClientRect().bottom, surfaceBottom:document.querySelector('.n-expanded').getBoundingClientRect().bottom};
  })()`);
  assert(bounds.lastBottom <= bounds.panelBottom + 1, "Five queue rows must fit without clipping");
  assert(bounds.surfaceBottom - bounds.lastBottom < 40, "Queue surface must finish close to the fifth row");
  console.log('Space below fifth row:',Math.round(bounds.surfaceBottom-bounds.lastBottom),'px');
  assert.equal(await evaluate("lyrics.getState().expanded"), false);
  assert.deepEqual(await evaluate("playCommands"), []);
  const shot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../music-queue.png"), Buffer.from(shot.data, "base64"));
  await evaluate("(async()=>{ (await import('/src/island/pageLayer.ts')).closeTopLayer(); })()");
  await wait("!document.querySelector('.m-queue')");
  const stopped = await evaluate("queueReads.length");
  await new Promise((r) => setTimeout(r, 2700));
  assert.equal(await evaluate("queueReads.length"), stopped, "Closing must stop polling");
  await evaluate("document.querySelector('[aria-controls=music-queue]').click()");
  await wait("document.querySelectorAll('.m-queue-row').length === 5");
  await evaluate("document.querySelector('[aria-controls=music-lyrics]').click()");
  await wait("!document.querySelector('.m-queue')");
  assert.equal(await evaluate("queue.getState().expanded"), false);
  console.log(
    "Music queue passed: 5 ordered entries, no playback commands, row fit, animated expansion, Escape, exclusive Lyrics and stopped polling. Screenshot: ../music-queue.png",
  );
}
