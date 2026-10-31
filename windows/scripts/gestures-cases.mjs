// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";

// Real wheel events over the real island, read from the DOM: a wheel opens and
// closes, fine-grained sideways travel skips tracks, and lists keep the wheel.
export async function checkGestures({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.n-activation')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.music=(await import('/src/music/store.ts')).useMusic;
    window.prefs=await import('/src/shared/prefs.ts');
    await prefs.setPrefs({island:{opening:'click',idleContent:'music',showPlayingMusic:true,gestures:true}});
    window.sent=[];
    const real=music.getState().command;
    music.setState({command:async(c)=>{sent.push(c)}});
    window.wheel=(x,y,deltaX,deltaY,wheelDeltaX,wheelDeltaY)=>{
      const target=document.elementFromPoint(x,y);
      const event=new WheelEvent('wheel',{bubbles:true,clientX:x,clientY:y,deltaX,deltaY});
      Object.defineProperty(event,'wheelDeltaX',{value:wheelDeltaX});
      Object.defineProperty(event,'wheelDeltaY',{value:wheelDeltaY});
      target.dispatchEvent(event);
      return target.className||target.tagName;
    };
  })()`);
  const pause = () => evaluate(`new Promise(r=>setTimeout(r,650))`);
  const middle = await evaluate(`innerWidth/2`);
  const expanded = () => evaluate(`island.getState().expanded`);

  // A wheel notch turned down opens; the next notch of the same turn does nothing more.
  assert.equal(await expanded(), false);
  await evaluate(`wheel(${middle},10,0,100,0,-120)`);
  assert.equal(await expanded(), true, "A wheel notch down opens the island");
  await evaluate(`wheel(${middle},10,0,-100,0,120)`);
  assert.equal(await expanded(), true, "The same sequence can't close what it opened");
  await pause();

  // Over the top row it closes; over another page's body it does not.
  await evaluate(`island.getState().open('timer')`);
  await pause();
  const body = await evaluate(
    `(()=>{const r=document.querySelector('.n-page').getBoundingClientRect();return r.top+r.height/2})()`,
  );
  await evaluate(`wheel(${middle},${body},0,-100,0,120)`);
  assert.equal(await expanded(), true, "A page body keeps the wheel");
  await pause();
  // A control in the row keeps the wheel too, so aim between them.
  const gap = await evaluate(`(()=>{const r=document.querySelector('.n-row').getBoundingClientRect();
    for(let x=r.left+2;x<r.right;x+=4){const e=document.elementFromPoint(x,r.top+r.height/2);
      if(e?.closest('.island-frame')&&!e.closest('button,input,a,[role=slider]'))return {x,y:r.top+r.height/2}}})()`);
  await evaluate(`wheel(${gap.x},${gap.y},0,-100,0,120)`);
  assert.equal(await expanded(), false, "Scrolling up over the top row closes");
  await pause();

  // Off, nothing answers.
  await evaluate(`prefs.setPrefs({island:{gestures:false}})`);
  await evaluate(`wheel(${middle},10,0,100,0,-120)`);
  assert.equal(await expanded(), false, "The setting turns gestures off");
  await evaluate(`prefs.setPrefs({island:{gestures:true}})`);
  await pause();

  // No music: a sideways swipe does nothing.
  await evaluate(`wheel(${middle},10,50,0,-60,0)`);
  assert.deepEqual(await evaluate(`sent`), [], "Nothing to skip without music");
  await pause();

  // With music, the fingers' direction picks the track, once per swipe.
  await evaluate(
    `(async()=>{const c=music.getState().command;music.getState().receive((await import('/src/music/demo.ts')).demoNowPlaying());music.setState({command:c})})()`,
  );
  await pause();
  for (let i = 0; i < 6; i++) await evaluate(`wheel(${middle},10,30,0,-36,0)`);
  assert.deepEqual(await evaluate(`sent`), ["next"], "A left swipe skips forward once");
  await pause();
  for (let i = 0; i < 3; i++) await evaluate(`wheel(${middle},10,-30,0,36,0)`);
  assert.deepEqual(await evaluate(`sent`), ["next", "previous"], "A right swipe skips back");
  await pause();
  await evaluate(`wheel(${middle},10,100,0,-120,0)`);
  assert.deepEqual(await evaluate(`sent`), ["next", "previous"], "A tilt wheel doesn't skip");
  assert.equal(await expanded(), false);
}
