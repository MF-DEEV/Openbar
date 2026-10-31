// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkIdleContent({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.n-activation')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.geometryOf=(await import('/src/island/store.ts')).geometryOf;
    window.battery=(await import('/src/island/battery.ts')).useBattery;
    window.music=(await import('/src/music/store.ts')).useMusic;
    window.agents=(await import('/src/agents/store.ts')).useAgents;
    window.prefs=await import('/src/shared/prefs.ts');
    window.emptyAgents=(await import('/src/agents/store.ts')).emptySnapshot;
    window.setIdle=idleContent=>prefs.setPrefs({island:{idleContent}});
    await document.fonts.ready;
    await prefs.setPrefs({island:{opening:'click',trackChange:false,agentsLiveActivity:false}});
    battery.setState({battery:{percent:100,charging:true,pluggedIn:true,rateWatts:null,secondsLeft:null,healthPercent:null,cycleCount:null}});
    await setIdle('battery');
  })()`);
  const settle = () => evaluate(`new Promise(r=>setTimeout(r,650))`);
  const capture = async (name) => {
    const frame = await evaluate(
      `(()=>{const r=document.querySelector('.island-frame').getBoundingClientRect();return {left:r.left,width:r.width,height:r.height}})()`,
    );
    const screenshot = await send("Page.captureScreenshot", {
      format: "png",
      clip: { x: frame.left - 12, y: 0, width: frame.width + 24, height: Math.max(60, frame.height + 24), scale: 3 },
    });
    await mkdir(resolve("../build"), { recursive: true });
    await writeFile(resolve(`../build/${name}.png`), Buffer.from(screenshot.data, "base64"));
  };
  const reading = () =>
    evaluate(`(()=>{
    const el=document.querySelector('.n-activation');
    const r=e=>{const b=e.getBoundingClientRect();return {left:b.left,right:b.right,top:b.top,bottom:b.bottom,width:b.width,height:b.height}};
    return {text:el?.textContent,label:el?.getAttribute('aria-label'),idle:!!el?.classList.contains('n-idle'),stripHeight:geometryOf(island.getState()).stripHeight,
      frame:r(document.querySelector('.island-frame')),button:el?r(el):null,
      wings:[...document.querySelectorAll('.n-idle-wing')].map(e=>({box:r(e),clipped:e.scrollWidth>e.clientWidth+1,
        children:[...e.children].map(r)}))};
  })()`);

  for (const silhouette of ["notch", "capsule"]) {
    await evaluate(`prefs.setPrefs({island:{silhouette:'${silhouette}'}})`);
    await settle();
    const result = await reading();
    assert(result.idle && result.label.includes("100%, charging"), JSON.stringify(result));
    assert.equal(result.text, "100%");
    assert(Math.abs(result.frame.height - result.stripHeight) < 0.02, "Idle content keeps the original strip height");
    for (const wing of result.wings) {
      assert(!wing.clipped, `No clipped idle reading: ${JSON.stringify(wing)}`);
      for (const child of wing.children) {
        assert(child.left >= result.frame.left && child.right <= result.frame.right);
        assert(child.top >= result.frame.top && child.bottom <= result.frame.bottom);
      }
    }
    if (silhouette === "notch") {
      assert.equal(result.frame.width, 223, "Use the existing 44px wings");
      assert(result.wings[0].box.right <= result.frame.left + 44);
      assert(result.wings[1].box.left >= result.frame.right - 44);
    }
    const screenshot = await send("Page.captureScreenshot", {
      format: "png",
      clip: { x: result.frame.left - 12, y: 0, width: result.frame.width + 24, height: 60, scale: 3 },
    });
    await mkdir(resolve("../build"), { recursive: true });
    await writeFile(resolve(`../build/idle-battery-${silhouette}.png`), Buffer.from(screenshot.data, "base64"));
  }

  await evaluate(`battery.setState(s=>({battery:{...s.battery,percent:12,charging:false,pluggedIn:false}}))`);
  await settle();
  let result = await reading();
  assert.equal(result.text, "12%");
  assert(result.label.includes("on battery"));
  assert(await evaluate(`!!document.querySelector('.n-idle-wing .lucide-battery-low')`));
  const reading_color = () =>
    evaluate(`(()=>{const w=[...document.querySelectorAll('.n-idle-wing')];
      return {icon:getComputedStyle(w[0].querySelector('svg')).color+'|'+w[0].querySelector('svg').getAttribute('stroke'),
              text:getComputedStyle(w[1].querySelector('span')??w[1]).color}})()`);
  const WHITE_BEFORE = await reading_color();
  await evaluate(`prefs.setPrefs({island:{lowBatteryTint:true}})`);
  await settle();
  assert.equal((await reading_color()).text, WHITE_BEFORE.text, "12% stays plain above the red level");
  await evaluate(`battery.setState(s=>({battery:{...s.battery,percent:9}}))`);
  await settle();
  let warned = await reading_color();
  assert.equal(warned.text, "rgb(255, 69, 58)", "At the red level the charge turns red");
  assert(warned.icon.endsWith("#ff453a"), "and so does its icon");
  await evaluate(`battery.setState(s=>({battery:{...s.battery,pluggedIn:true}}))`);
  await settle();
  assert.equal((await reading_color()).text, WHITE_BEFORE.text, "Plugged in, no warning shows");
  await evaluate(`battery.setState(s=>({battery:{...s.battery,pluggedIn:false,percent:18}}))`);
  await evaluate(`prefs.setPrefs({island:{lowBatteryEarly:true}})`);
  await settle();
  assert.equal((await reading_color()).text, "rgb(255, 159, 10)", "The early level turns amber first");
  await evaluate(`prefs.setPrefs({island:{lowBatteryTint:false,lowBatteryEarly:false}})`);
  await evaluate(`battery.setState(s=>({battery:{...s.battery,percent:12}}))`);
  await settle();
  await evaluate(`battery.setState(s=>({battery:{...s.battery,percent:null}}))`);
  await settle();
  assert.equal((await reading()).text, "", "An unknown charge must not appear as 0%");
  await evaluate(`battery.setState({battery:null})`);
  await settle();
  assert(!(await reading()).idle, "A computer without a battery rests empty");

  await evaluate(`setIdle('music')`);
  await settle();
  assert(!(await reading()).idle, "No music session rests empty");
  await evaluate(
    `(async()=>{const demo=(await import('/src/music/demo.ts')).demoNowPlaying();music.getState().receive(demo)})()`,
  );
  await settle();
  assert(
    await evaluate(`!!document.querySelector('.m-strip .m-bars')`),
    "Playing media shows artwork and visualization",
  );
  await capture("idle-music");
  await evaluate(`music.setState(s=>({playback:{...s.playback,isPlaying:false}}))`);
  await settle();
  assert(!(await reading()).idle, "Paused music rests empty");
  await evaluate(`setIdle('none')`);
  await evaluate(`music.setState(s=>({playback:{...s.playback,isPlaying:true}}))`);
  await settle();
  assert(
    await evaluate(`!document.querySelector('.m-strip') && !!document.querySelector('.n-activation')`),
    "Nothing disables the playing-music strip, as on macOS",
  );
  await evaluate(`music.setState({playback:null});setIdle('agents')`);
  await settle();
  assert.equal((await reading()).text, "", "No agent data invents no usage");
  await evaluate(
    `(()=>{const s=emptyAgents();s.seen=['claude'];s.periods.today.total.cost=4.25;agents.getState().receive(s)})()`,
  );
  await settle();
  assert.equal((await reading()).text, "$4.25", "Agents rest with today's cost without an allowance");
  await evaluate(
    `prefs.setPrefs({island:{silhouette:'notch'}});agents.getState().receive({...agents.getState().snapshot,periods:{...agents.getState().snapshot.periods,today:{...agents.getState().snapshot.periods.today,total:{...agents.getState().snapshot.periods.today.total,cost:9999}}}})`,
  );
  await settle();
  const costReading = await reading();
  assert.equal(costReading.text, "$9,999");
  assert(
    costReading.wings.every((wing) => !wing.clipped),
    "Long cost readings scale down within the resting wings",
  );
  await evaluate(
    `(()=>{const s=agents.getState().snapshot;agents.getState().receive({...s,limits:{claude:{provider:'claude',observedAt:Date.now(),source:'account',windows:[{id:'s',kind:'session',minutes:300,scope:null,usedPercent:25,resetsAt:null,estimatedResetsAt:null}]}}})})()`,
  );
  await settle();
  assert.equal((await reading()).text, "75%");
  await capture("idle-agents");
  await evaluate(`prefs.setPrefs({island:{agentsLimitDisplay:'used'}})`);
  await settle();
  assert.equal((await reading()).text, "25%", "Existing allowance settings update the idle reading");
  await evaluate(`prefs.setPrefs({island:{sections:{agents:false}}})`);
  await settle();
  assert(!(await reading()).idle);
  assert.equal(await evaluate(`island.getState().settings.idleContent`), "agents", "Unavailable choices stay saved");
  await evaluate(`prefs.setPrefs({island:{sections:{agents:true}}})`);
  await settle();
  assert.equal((await reading()).text, "25%");

  await evaluate(`(async()=>{
    await prefs.setPrefs({island:{agentsLiveActivity:true}});
    const s=agents.getState().snapshot;
    const tokens=(await import('/src/agents/store.ts')).emptyTotals().tokens;
    agents.getState().receive({...s,live:[{id:'idle-test',provider:'claude',started:Date.now(),lastActivity:Date.now(),model:'Claude',project:'test',tokens,cost:0}]});
  })()`);
  await settle();
  assert(
    await evaluate(`!!document.querySelector('.ag-strip')`),
    "Working Claude agents retain their live status strip",
  );
  await capture("idle-claude-working");
  await evaluate(`(async()=>{
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    await prefs.setPrefs({sessions:{autoExpand:false}});
    sessions.getState().receive((await import('/src/sessions/demo.ts')).demoSnapshot());
  })()`);
  await settle();
  await evaluate(`island.getState().dismissNotice()`);
  await settle();
  assert(
    await evaluate(`island.getState().activities.sessions?.urgent && !!document.querySelector('.s-strip')`),
    "Waiting Claude sessions keep their higher-priority status",
  );
  await capture("idle-claude-waiting");
  await evaluate(
    `sessions.getState().receive(sessions.getInitialState().snapshot);agents.getState().receive({...agents.getState().snapshot,live:[]});island.getState().dismissNotice()`,
  );
  await settle();

  // Preserve the activation button, hover preview and hidden-until-hover behavior.
  await evaluate(`document.querySelector('.n-activation').click()`);
  await settle();
  assert(await evaluate(`island.getState().expanded && !!document.querySelector('.n-expanded')`));
  await evaluate(
    `island.getState().collapse();prefs.setPrefs({island:{opening:'hoverPreview',hoverDelay:0.1}});island.getState().pointerEnter()`,
  );
  await settle();
  assert(await evaluate(`!!document.querySelector('.n-peek')`));
  await evaluate(`island.getState().pointerExit()`);
  await settle();
  assert.equal((await reading()).text, "25%");
  await evaluate(`prefs.setPrefs({island:{opening:'hiddenUntilHover'}})`);
  await settle();
  assert(await evaluate(`document.querySelector('.island-root').classList.contains('is-hidden')`));
  // Exercise the new settings control and its saved value.
  await send("Page.navigate", { url: `${base}settings.html#content` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('[role="combobox"][aria-label="While inactive"]')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`document.querySelector('[role="combobox"][aria-label="While inactive"]').click()`);
  await settle();
  assert.deepEqual(
    await evaluate(`[...document.querySelectorAll('[role="listbox"] [role="option"]')].map(e=>e.textContent.trim())`),
    ["Nothing", "Battery", "Music", "AI Agents"],
  );
  await evaluate(`[...document.querySelectorAll('[role="option"]')].find(e=>e.textContent.trim()==='Battery').click()`);
  await settle();
  assert.equal(
    await evaluate(
      `(async()=>{const {useSettings}=await import('/src/settings/state.ts');return useSettings.getState().prefs.island.idleContent})()`,
    ),
    "battery",
  );
  console.log(
    "Idle content: battery, music, agent allowance/cost, missing data, settings, geometry and interactions passed.",
  );
}
