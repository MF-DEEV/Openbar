// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";

// The companion in the island itself: where it stands, what it does to the
// island's size, and how the pointer and a limit coming back reach it.
export async function checkMascot({ send, evaluate, base }) {
  // The other cases run with reduced motion; this one needs it to move.
  const motion = (value) =>
    send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value }] });
  await motion("no-preference");
  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.n-activation')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async()=>{
    window.prefs=await import('/src/shared/prefs.ts');
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.mascot=await import('/src/mascot/service.ts');
    window.agents=await import('/src/agents/register.tsx');
    await prefs.setPrefs({island:{idleContent:'none',mascotVisits:false,mascotHidesWhenIdle:false,mascotEnabled:false}});
  })()`);
  const wait = (ms) => evaluate(`new Promise(r=>setTimeout(r,${ms}))`);
  const reading = () =>
    evaluate(`(()=>{
    const box=e=>{const b=e.getBoundingClientRect();return {left:b.left,right:b.right,top:b.top,bottom:b.bottom,width:b.width}};
    const root=document.querySelector('.mr-root');
    return {frame:box(document.querySelector('.island-frame')),rig:root?box(root):null,
      attention:root?getComputedStyle(root.querySelectorAll('.mr-layer')[3]).transform:null};
  })()`);

  await wait(700);
  const off = await reading();
  assert.equal(off.rig, null, "switched off, nothing is drawn");

  await evaluate(`prefs.setPrefs({island:{mascotEnabled:true}})`);
  await wait(1200);
  const notch = await reading();
  assert(notch.frame.width > off.frame.width, "its wings open beside the notch");
  assert(notch.rig.left >= notch.frame.left && notch.rig.right <= notch.frame.left + 44, "it rests in the left wing");
  assert(notch.rig.top >= 0 && notch.rig.bottom <= notch.frame.bottom, "it fits under the top edge");

  await evaluate(`prefs.setPrefs({island:{mascotSide:'right'}})`);
  await wait(1600);
  const right = await reading();
  assert(right.rig.left >= right.frame.right - 44, "it crosses to the right wing");

  // The pointer reaches it through what it stands on.
  const x = (right.rig.left + right.rig.right) / 2;
  const y = (right.rig.top + right.rig.bottom) / 2;
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: right.frame.left + 10, y });
  await wait(350);
  const looking = await reading();
  assert.notEqual(looking.attention, "none", "its eyes follow the pointer");
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x + 1, y });
  await evaluate(`(window.__pet=new Promise(done=>{const seen=setInterval(()=>{
    if(document.getAnimations().some(a=>a.effect.getTiming().duration===1400)){clearInterval(seen);done(true)}},30);
    setTimeout(()=>{clearInterval(seen);done(false)},2000)}),0)`);
  assert.equal(await evaluate(`window.__pet`), true, "a second on it pets it");
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 5, y: 150 });
  await wait(2000);

  // A limit coming back: the notice wears it in place of its symbol.
  await evaluate(
    `agents.present({kind:'limitReset',provider:'claude',window:{kind:'session',label:'Session',used:0,resetsAt:0}})`,
  );
  await wait(300);
  assert.equal(await evaluate(`island.getState().notice?.mascot`), "celebrate");
  assert.equal(await evaluate(`!!document.querySelector('.n-notice .mr-face')`), true, "the notice wears it");
  assert.equal(await evaluate(`!!document.querySelector('.n-notice-image')`), false, "in place of its picture");
  await evaluate(`island.getState().dismissNotice()`);
  await wait(800);

  await evaluate(`prefs.setPrefs({island:{silhouette:'capsule'}})`);
  await wait(900);
  const capsule = await reading();
  const middle = (capsule.frame.left + capsule.frame.right) / 2;
  assert(Math.abs((capsule.rig.left + capsule.rig.right) / 2 - middle) < 1, "a capsule holds it in the middle");

  // Over an activity, the strip steps aside and comes back.
  await evaluate(`island.getState().setActivity('music',{wing:60,capsuleContent:56,opens:'music'})`);
  await wait(600);
  await evaluate(`mascot.greetMascot()`);
  await wait(300);
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('.mr-activity-content')).opacity!=='1'`), true);
  await wait(3500);
  assert.equal(await evaluate(`mascot.useMascot.getState().stepsAside`), false);
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('.mr-activity-content')).opacity`), "1");
  await evaluate(`island.getState().setActivity('music',null)`);

  await evaluate(`prefs.setPrefs({island:{silhouette:'notch',mascotEnabled:false}})`);
  await wait(1100);
  assert.equal((await reading()).rig, null, "switched off, it says goodbye and is gone");
  // Without motion it appears in its place with no stroll, and doesn't follow the pointer.
  await motion("reduce");
  await evaluate(`prefs.setPrefs({island:{mascotEnabled:true}})`);
  await wait(200);
  assert.equal(await evaluate(`mascot.useMascot.getState().visit`), null, "no entrance without motion");
  const still = await reading();
  assert(still.rig, "it rests all the same");
  await evaluate(`prefs.setPrefs({island:{mascotEnabled:false}})`);
  console.log("mascot: ok");
}
