// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";

export async function checkMusicOpenPlayer({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?music` });
  const wait = async (expression) => {
    for (let i = 0; i < 100; i++) {
      if (await evaluate(expression)) return;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    assert.fail(`Timed out: ${expression}`);
  };
  await wait(`!!document.querySelector('.m-strip')`);
  await evaluate(`(async () => {
    window.island = (await import('/src/island/store.ts')).useIsland;
    window.music = (await import('/src/music/store.ts')).useMusic;
    window.playerBridge = (await import('/src/shared/bridge.ts')).bridge.music;
    window.originalPlayback = {...music.getState().playback};
    window.openedPlayers = []; window.playbackCommands = []; window.unhandled = [];
    window.addEventListener('unhandledrejection', e => window.unhandled.push(String(e.reason)));
    playerBridge.openPlayer = async source => { openedPlayers.push(source); return true; };
    playerBridge.command = async command => { playbackCommands.push(command); return true; };
    playerBridge.shuffle = async () => true;
    island.getState().open('music');
  })()`);
  await wait(`!!document.querySelector('.m-cover-button')`);
  assert.equal(await evaluate(`document.querySelector('.m-cover-button').tagName`), "BUTTON");
  assert.equal(await evaluate(`document.querySelector('.m-cover-button').getAttribute('aria-label')`), "Open Spotify");
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('.m-cover-button')).cursor`), "pointer");
  const frames = () => evaluate(`(async()=>{for(let i=0;i<8;i++)await new Promise(requestAnimationFrame)})()`);
  await frames();
  const originalBounds = await evaluate(
    `(() => { const r=document.querySelector('.m-cover-button').getBoundingClientRect();return {width:r.width,height:r.height}; })()`,
  );
  assert(Math.abs(originalBounds.width - originalBounds.height) < 1, "The cover must remain square");
  await evaluate(`document.querySelector('.m-cover-button').click()`);
  await wait(`openedPlayers.length === 1 && !document.querySelector('.m-cover-button').disabled`);
  assert.equal(await evaluate(`openedPlayers[0]`), "Spotify.exe");

  // Native button activation must work with both Enter and Space, without toggling playback.
  await evaluate(`document.querySelector('.m-cover-button').focus()`);
  for (const [key, code, virtual] of [
    ["Enter", "Enter", 13],
    [" ", "Space", 32],
  ]) {
    await send("Input.dispatchKeyEvent", {
      type: "keyDown",
      key,
      code,
      text: key === "Enter" ? "\r" : key,
      windowsVirtualKeyCode: virtual,
    });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: virtual });
    await frames();
    assert.equal(
      await evaluate(`document.activeElement?.className`),
      "m-cover-button",
      `The cover must keep keyboard focus after ${code}`,
    );
  }
  await wait(`openedPlayers.length === 3`);
  assert.deepEqual(await evaluate(`playbackCommands`), []);
  for (const label of ["Previous", "Pause", "Next", "Shuffle"]) {
    await evaluate(`document.querySelector('[aria-label="${label}"]').click()`);
    await frames();
  }
  assert.deepEqual(await evaluate(`playbackCommands`), ["previous", "toggle", "next"]);
  assert.equal(await evaluate(`openedPlayers.length`), 3, "Transport controls must not open the player");

  await evaluate(`playerBridge.openPlayer = async () => false; document.querySelector('.m-cover-button').click()`);
  await wait(`document.querySelector('.m-artist').textContent.includes('Could not open')`);
  await evaluate(
    `playerBridge.openPlayer = async () => { throw Error('unavailable'); }; document.querySelector('.m-cover-button').click()`,
  );
  await wait(`!document.querySelector('.m-cover-button').disabled`);
  assert(await evaluate(`document.querySelector('.m-artist').classList.contains('is-failed')`));
  await evaluate(
    `playerBridge.openPlayer = async source => { openedPlayers.push(source); return true; }; document.querySelector('.m-cover-button').click()`,
  );
  await wait(`!document.querySelector('.m-artist').textContent.includes('Could not open')`);
  assert.deepEqual(await evaluate(`unhandled`), []);

  const beforePending = await evaluate(`openedPlayers.length`);
  await evaluate(
    `playerBridge.openPlayer = source => { openedPlayers.push(source); return new Promise(resolve => window.finishOpen = resolve); }; document.querySelector('.m-cover-button').click(); document.querySelector('.m-cover-button').click()`,
  );
  assert.equal(
    await evaluate(`openedPlayers.length`),
    beforePending + 1,
    "An in-flight open must ignore repeated activation",
  );
  assert.equal(await evaluate(`document.querySelector('.m-cover-button').getAttribute('aria-busy')`), "true");
  await evaluate(
    `window.finishOpen(true); playerBridge.openPlayer = async source => { openedPlayers.push(source); return true; }`,
  );
  await wait(`document.querySelector('.m-cover-button').getAttribute('aria-busy') === 'false'`);

  // The displayed playback takes precedence over an unrelated session in the sources list.
  await evaluate(`music.setState({playback:{...originalPlayback,source:'other-player',sourceName:'Other Player'}})`);
  await frames();
  await evaluate(`document.querySelector('.m-cover-button').click()`);
  await wait(`openedPlayers.at(-1) === 'other-player'`);

  // The empty icon opens a known session; prefer a playing session to an idle one.
  await evaluate(
    `music.setState({playback:null,optimistic:null,sources:[{id:'idle',name:'Idle Player',isPlaying:false},{id:'available',name:'Available Player',isPlaying:true}]})`,
  );
  await frames();
  assert.equal(await evaluate(`document.querySelector('.m-title').textContent`), "Nothing playing");
  await evaluate(`document.querySelector('.m-cover-button').click()`);
  await wait(`openedPlayers.at(-1) === 'available'`);
  await evaluate(`music.setState({sources:[{id:'idle',name:'Idle Player',isPlaying:false}]})`);
  await frames();
  await evaluate(`document.querySelector('.m-cover-button').click()`);
  await wait(`openedPlayers.at(-1) === 'idle'`);

  await evaluate(`music.setState({sources:[]})`);
  await frames();
  const count = await evaluate(`openedPlayers.length`);
  assert(await evaluate(`document.querySelector('.m-cover-button').disabled`));
  await evaluate(`document.querySelector('.m-cover-button').click()`);
  assert.equal(
    await evaluate(`openedPlayers.length`),
    count,
    "An unsupported empty state must not launch an arbitrary app",
  );
  console.log(
    "Music open-player passed: cover, Enter/Space, independent controls, errors/retry, correct source, supported and unsupported empty states.",
  );
}
