// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkIslandPosition({ send, evaluate, base }) {
  await send("Emulation.setDeviceMetricsOverride", { width: 1050, height: 760, deviceScaleFactor: 1.25, mobile: false });
  await send("Page.navigate", { url: `${base}settings.html#layout` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.st-island-drag')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  assert(await evaluate(`!!document.querySelector('.st-island-drag')`), "Position preview did not render");
  const bounds = await evaluate(`(() => { const r = document.querySelector('.st-island-drag').getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: bounds.x, y: bounds.y, button: "left", clickCount: 1 });
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: bounds.x + 180, y: bounds.y + 90, button: "left", buttons: 1 });
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: bounds.x + 180, y: bounds.y + 90, button: "left", clickCount: 1 });
  const saved = await evaluate(`(async () => { const {useSettings}=await import('/src/settings/state.ts'); return useSettings.getState().prefs.island.position; })()`);
  assert(saved.x > 0.5 && saved.y > 0.1, `Drag did not save its position: ${JSON.stringify(saved)}`);
  assert(saved.x === 0 || saved.x === 1 || saved.y === 0 || saved.y === 1,
    `Drag saved a point outside the screen edges: ${JSON.stringify(saved)}`);
  const legacy = await evaluate(`(async () => {
    const {resolve}=await import('/src/shared/prefs.ts');
    return resolve({island:{position:{x:0.4,y:0.35}}}).island.position;
  })()`);
  assert(legacy.x === 0 || legacy.x === 1 || legacy.y === 0 || legacy.y === 1,
    `Old centre position was not constrained: ${JSON.stringify(legacy)}`);
  await mkdir(resolve("../build"), { recursive: true });
  const shot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../build/island-position-settings.png"), Buffer.from(shot.data, "base64"));

  for (const [position, label, axis, direction] of [
    [{ x: 0.5, y: 0 }, "Distance from top", "y", 1],
    [{ x: 0.5, y: 1 }, "Distance from bottom", "y", -1],
    [{ x: 0, y: 0.5 }, "Distance from left", "x", 1],
    [{ x: 1, y: 0.5 }, "Distance from right", "x", -1],
  ]) {
    const preview = await evaluate(`(async()=>{
      const {useSettings}=await import('/src/settings/state.ts');
      useSettings.getState().island('silhouette','capsule');
      useSettings.getState().island('position',${JSON.stringify(position)});
      useSettings.getState().island('capsuleFit',{width:0,height:0,drop:0});
      await new Promise(requestAnimationFrame);
      const read=()=>{const root=document.querySelector('.st-notch-preview');const g=root.querySelector('svg g');
        const m=g.getAttribute('transform').match(/translate\\(([-\\d.]+) ([-\\d.]+)\\)/);
        const b=g.querySelector('path').getBBox();
        return {x:+m[1],y:+m[2],vertical:b.height>b.width};};
      const before=read();
      useSettings.getState().island('capsuleFit',{width:0,height:0,drop:20});
      await new Promise(requestAnimationFrame);
      const after=read();
      const titles=[...document.querySelectorAll('.st-subrow .st-card-title')].map(e=>e.textContent);
      return {before,after,titles};
    })()`);
    assert(preview.titles.includes(label), `Wrong capsule distance label at ${JSON.stringify(position)}: ${preview.titles}`);
    assert(Math.abs(preview.after[axis] - preview.before[axis] - direction * 20) < 0.01);
    assert.equal(preview.after.vertical, axis === "x");
    await evaluate(`document.querySelector('.st-notch-preview').scrollIntoView({block:'center'})`);
    const capsuleShot = await send("Page.captureScreenshot", { format: "png" });
    await writeFile(resolve(`../build/capsule-fit-${label.split(" ").at(-1)}.png`), Buffer.from(capsuleShot.data, "base64"));
  }
  await evaluate(`(async()=>{const {useSettings}=await import('/src/settings/state.ts');useSettings.getState().island('capsuleFit',{width:0,height:0,drop:0})})()`);

  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.island-frame')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  for (const [position, axis, direction] of [
    [{ x: 0.5, y: 0 }, "top", 1],
    [{ x: 0.5, y: 1 }, "top", -1],
    [{ x: 0, y: 0.5 }, "left", 1],
    [{ x: 1, y: 0.5 }, "left", -1],
  ]) {
    const delta = await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      const update=(drop)=>useIsland.setState(s=>({settings:{...s.settings,silhouette:'capsule',position:${JSON.stringify(position)},capsuleFit:{...s.settings.capsuleFit,drop}}}));
      update(0);await new Promise(requestAnimationFrame);
      const before=parseFloat(document.querySelector('.island-frame').style.${axis});
      update(20);await new Promise(requestAnimationFrame);
      return parseFloat(document.querySelector('.island-frame').style.${axis})-before;
    })()`);
    assert(Math.abs(delta - direction * 20) < 0.01, `Live island moved the wrong way at ${JSON.stringify(position)}: ${delta}`);
  }
  await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');useIsland.setState(s=>({settings:{...s.settings,capsuleFit:{...s.settings.capsuleFit,drop:0}}}))})()`);
  const cases = [
    { x: 0.5, y: 0, mode: "resting" },
    { x: 0.02, y: 0.5, mode: "resting" },
    { x: 0.5, y: 0, mode: "expanded", portrait: false },
    { x: 0.5, y: 0.96, mode: "expanded", portrait: false },
    { x: 0.02, y: 0.5, mode: "expanded", portrait: true },
    { x: 0.98, y: 0.5, mode: "expanded", portrait: true },
    { x: 0.02, y: 0.02, mode: "expanded", portrait: false },
    { x: 0.98, y: 0.02, mode: "expanded", portrait: false },
    { x: 0.02, y: 0.98, mode: "expanded", portrait: false },
    { x: 0.98, y: 0.98, mode: "expanded", portrait: false },
    { x: 0.01, y: 0.2, mode: "expanded", portrait: true },
    { x: 0.99, y: 0.8, mode: "expanded", portrait: true },
    { x: 0.02, y: 0.9, mode: "notice" },
    { x: 0.98, y: 0.5, mode: "notice" },
    { x: 0.98, y: 0.1, mode: "peek" },
    { x: 0.02, y: 0.5, mode: "peek" },
    { x: 0.5, y: 0.95, mode: "dropHint" },
    { x: 0.02, y: 0.5, mode: "dropHint" },
  ];
  for (const c of cases) {
    const result = await evaluate(`(async () => {
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState(s => ({expanded:${c.mode === "expanded"},selected:'controls',showingSections:false,
        notice:${c.mode === "notice" ? "{id:'position-test',kind:'timer',title:'Timer',detail:'00:30',priority:1,duration:5}" : "null"},
        peeking:${c.mode === "peek"},dropHint:${c.mode === "dropHint"},
        settings:{...s.settings,position:{x:${c.x},y:${c.y}},idleContent:'none'}}));
      for(let i=0;i<5;i++) await new Promise(requestAnimationFrame);
      await new Promise(r=>setTimeout(r,350));
      const r=document.querySelector('.island-frame').getBoundingClientRect();
      const q=[...document.querySelectorAll('.n-bubble')].filter(el=>Number(getComputedStyle(el.parentElement).opacity)>0.01).map(el=>el.getBoundingClientRect());
      const panel=document.querySelector('.n-expanded');
      const page=panel?.querySelector('.n-page')?.getBoundingClientRect();
      const header=panel?.querySelector('.n-portrait-leading')?.getBoundingClientRect();
      const footer=panel?.querySelector('.n-portrait-trailing')?.getBoundingClientRect();
      const compact=document.querySelector('.n-notice.is-vertical,.n-peek.is-vertical,.fl-drop-hint.is-vertical');
      const compactBox=compact?.getBoundingClientRect();
      const {usableDisplayArea}=await import('/src/island/placement.ts');
      const work=usableDisplayArea({width:innerWidth,height:innerHeight});
      return {frame:{left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height},work,
        portrait:!!panel?.classList.contains('n-expanded-vertical'),
        page:page&&{left:page.left,right:page.right,top:page.top,bottom:page.bottom},
        header:header&&{top:header.top,bottom:header.bottom},
        footer:footer&&{top:footer.top,bottom:footer.bottom},
        compact:compactBox&&{left:compactBox.left,right:compactBox.right,top:compactBox.top,bottom:compactBox.bottom},
        bubbles:q.map(b=>({left:b.left,right:b.right,top:b.top,bottom:b.bottom}))};
    })()`);
    const { frame } = result;
    assert(frame.left >= -1 && frame.right <= 1051 && frame.top >= -1 && frame.bottom <= 761,
      `Island escaped viewport at ${JSON.stringify(c)}: ${JSON.stringify(frame)}`);
    assert(frame.left >= result.work.left - 1 && frame.right <= result.work.right + 1 &&
      frame.top >= result.work.top - 1 && frame.bottom <= result.work.bottom + 1,
      `Island escaped work area at ${JSON.stringify(c)}: ${JSON.stringify({frame,work:result.work})}`);
    if (c.x < 0.18 && c.mode === "resting") assert(frame.height > frame.width, "Side island did not become vertical");
    if ((c.x < 0.18 || c.x > 0.82) && c.y === 0.5 && ["resting","notice","peek","dropHint"].includes(c.mode)) {
      assert(frame.height > frame.width, `Side ${c.mode} did not become portrait`);
      if (c.mode !== "resting") assert(result.compact && result.compact.left>=frame.left-1&&
        result.compact.right<=frame.right+1&&result.compact.top>=frame.top-1&&result.compact.bottom<=frame.bottom+1,
        `Side ${c.mode} content escaped its frame`);
    }
    if (c.mode === "expanded") {
      assert.equal(result.portrait, c.portrait, "Wrong orientation at " + JSON.stringify(c));
      if (c.portrait) assert(frame.height > frame.width, "Side panel did not become portrait");
      if (c.portrait) {
        assert((!result.header || result.header.bottom <= result.page.top) &&
          (!result.footer || result.page.bottom <= result.footer.top),
          "Header, page and footer overlap");
      }
      assert(result.page.left >= frame.left - 1 && result.page.right <= frame.right + 1 &&
        result.page.top >= frame.top - 1 && result.page.bottom <= frame.bottom + 1,
        "Page escaped its panel");
    }
    for (const bubble of result.bubbles) {
      assert(bubble.left >= -1 && bubble.right <= 1051 && bubble.top >= -1 && bubble.bottom <= 761,
        `Quick button escaped viewport at ${JSON.stringify(c)}: ${JSON.stringify(bubble)}`);
    }
    if (c.mode === "expanded" && c.x === 0.5 && c.y > 0.9) {
      assert(result.frame.bottom >= result.work.bottom - 12,
        `Bottom panel kept an unnecessary gap: ${JSON.stringify(result.frame)}`);
      assert(result.bubbles.some((b) => b.bottom < result.frame.top &&
        (b.left + b.right) / 2 > result.frame.left && (b.left + b.right) / 2 < result.frame.right),
        "Bottom panel has no centred button above it");
      assert(result.bubbles.filter((b) => (b.top + b.bottom) / 2 > result.frame.top + result.frame.height / 2 &&
        b.bottom < result.frame.bottom).length >= 4,
        "Bottom panel side buttons did not move toward its lower edge");
    }
    if (c.mode === "resting" && c.x < 0.18) {
      const sideShot = await send("Page.captureScreenshot", { format: "png" });
      await writeFile(resolve("../build/island-position-side.png"), Buffer.from(sideShot.data, "base64"));
    }
    if (c.mode === "expanded" && c.x > 0.82 && c.y === 0.5) {
      const sidePanelShot = await send("Page.captureScreenshot", { format: "png" });
      await writeFile(resolve("../build/island-position-side-panel.png"), Buffer.from(sidePanelShot.data, "base64"));
    }
    if (c.mode === "expanded" && c.y > 0.9) {
      const islandShot = await send("Page.captureScreenshot", { format: "png" });
      await writeFile(resolve("../build/island-position-bottom.png"), Buffer.from(islandShot.data, "base64"));
    }
  }
  const bottomModules=await evaluate(`(async()=>{
    const {useIsland,modulesOf}=await import('/src/island/store.ts');
    const ids=modulesOf(useIsland.getState());
    const results=[];
    for(const id of ids){
      useIsland.setState(s=>({expanded:true,selected:id,showingSections:false,
        sessionsEnlarged:false,capturePreview:null,notice:null,
        settings:{...s.settings,position:{x:0.5,y:1}}}));
      await new Promise(r=>setTimeout(r,550));
      const f=document.querySelector('.island-frame').getBoundingClientRect();
      const circles=[...document.querySelectorAll('.n-bubble')].filter(el=>
        Number(getComputedStyle(el.parentElement).opacity)>0.01).map(el=>el.getBoundingClientRect());
      results.push({id,frame:{left:f.left,right:f.right,top:f.top,bottom:f.bottom},
        bubbles:circles.map(b=>({left:b.left,right:b.right,top:b.top,bottom:b.bottom}))});
    }
    return results;
  })()`);
  for(const result of bottomModules){
    assert(result.frame.left>=-1&&result.frame.right<=1051&&result.frame.top>=-1&&result.frame.bottom<=761,
      `Bottom ${result.id} panel escaped viewport: ${JSON.stringify(result.frame)}`);
    assert(result.bubbles.some(b=>b.bottom<result.frame.top),
      `Bottom ${result.id} lost its button above the panel`);
    assert(result.bubbles.every(b=>b.left>=-1&&b.right<=1051&&b.top>=-1&&b.bottom<=761),
      `Bottom ${result.id} button escaped viewport`);
  }
  const idleSide = await evaluate(`(async () => {
    const {useIsland}=await import('/src/island/store.ts');
    const {useBattery}=await import('/src/island/battery.ts');
    useBattery.setState({battery:{percent:78,charging:false,pluggedIn:false,rateWatts:null,
      secondsLeft:null,healthPercent:null,cycleCount:null}});
    useIsland.setState(s=>({expanded:false,notice:null,peeking:false,dropHint:false,
      settings:{...s.settings,position:{x:1,y:0.5},idleContent:'battery'}}));
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const activation=document.querySelector('.n-activation.is-vertical');
    const wings=[...document.querySelectorAll('.n-idle-wing')].map(el=>el.getBoundingClientRect());
    return {vertical:!!activation,frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,
      width:frame.width,height:frame.height},wings:wings.map(w=>({left:w.left,right:w.right,top:w.top,bottom:w.bottom}))};
  })()`);
  assert(idleSide.vertical && idleSide.frame.height > idleSide.frame.width, "Side idle content did not turn vertical");
  assert(idleSide.wings.every(w=>w.left>=idleSide.frame.left-1&&w.right<=idleSide.frame.right+1&&
    w.top>=idleSide.frame.top-1&&w.bottom<=idleSide.frame.bottom+1), "Side idle content escaped its capsule");
  for(const fraction of [0.18,0.5,0.82]) {
    const hit=await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState(s=>({expanded:false,notice:null,peeking:false,dropHint:false,
        activities:{},pickedActivity:null,hoverSuppressed:true,
        settings:{...s.settings,position:{x:0,y:0.5},opening:'click',idleContent:'none'}}));
      await new Promise(r=>setTimeout(r,400));
      const b=document.querySelector('.island-backdrop path[id]').getBoundingClientRect();
      return {x:b.left+b.width/2,y:b.top+b.height*${fraction}};
    })()`);
    await send("Input.dispatchMouseEvent",{type:"mousePressed",x:hit.x,y:hit.y,button:"left",clickCount:1});
    await send("Input.dispatchMouseEvent",{type:"mouseReleased",x:hit.x,y:hit.y,button:"left",clickCount:1});
    assert(await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');return useIsland.getState().expanded})()`),
      `The resting side capsule has a dead click zone at ${fraction}`);
  }
  await send("Input.dispatchMouseEvent",{type:"mouseMoved",x:500,y:500});
  const musicSide = await evaluate(`(async () => {
    const {useIsland}=await import('/src/island/store.ts');
    const {useMusic}=await import('/src/music/store.ts');
    const {demoNowPlaying}=await import('/src/music/demo.ts');
    useIsland.setState(s=>({expanded:false,notice:null,peeking:false,dropHint:false,
      settings:{...s.settings,position:{x:1,y:0.5},idleContent:'battery',showPlayingMusic:true,trackChange:false}}));
    useMusic.getState().receive(demoNowPlaying());
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const portrait=document.querySelector('.n-activity-portrait .m-strip');
    const cover=portrait?.querySelector('.m-cover')?.getBoundingClientRect();
    const bars=portrait?.querySelector('.m-strip-wing.is-end')?.getBoundingClientRect();
    return {width:frame.width,height:frame.height,portrait:!!portrait,
      frame:{left:frame.left,right:frame.right},
      cover:cover&&{left:cover.left,right:cover.right,bottom:cover.bottom,width:cover.width},bars:bars&&{top:bars.top}};
  })()`);
  assert(musicSide.portrait && musicSide.width<=32 && musicSide.height>musicSide.width,
    `Playing music did not get a vertical capsule: ${JSON.stringify(musicSide)}`);
  assert(musicSide.cover.bottom < musicSide.bars.top, "Album artwork and visualizer did not stack vertically");
  assert(musicSide.cover.width >= 12 && musicSide.cover.left >= musicSide.frame.left + 2 && musicSide.cover.right <= musicSide.frame.right - 2,
    `Portrait artwork escaped the capsule: ${JSON.stringify(musicSide)}`);
  for (const x of [0, 1]) {
    const musicClick = await evaluate(`(async () => {
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState(s=>({expanded:false,activityPickerOpen:false,pickedActivity:'music',notice:null,
        hoverSuppressed:false,activities:{sessions:{wing:44,opens:'sessions'},music:{wing:56,capsuleContent:56,opens:'music'}},
        settings:{...s.settings,position:{x:${x},y:0.5},opening:'click'}}));
      await new Promise(r=>setTimeout(r,500));
      const r=document.querySelector('.n-activity-portrait .m-strip').getBoundingClientRect();
      return {x:r.left+r.width/2,y:r.top+r.height/2};
    })()`);
    await send("Input.dispatchMouseEvent", {type:"mousePressed",x:musicClick.x,y:musicClick.y,button:"left",clickCount:1});
    await send("Input.dispatchMouseEvent", {type:"mouseReleased",x:musicClick.x,y:musicClick.y,button:"left",clickCount:1});
    const afterClick=await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');
      return {picker:useIsland.getState().activityPickerOpen,expanded:useIsland.getState().expanded}})()`);
    assert(afterClick.picker && !afterClick.expanded,
      `One side music click fired twice at x=${x}: ${JSON.stringify(afterClick)}`);
  }
  const musicShot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../build/island-position-side-music.png"), Buffer.from(musicShot.data, "base64"));
  await evaluate(`(async()=>{const {useMusic}=await import('/src/music/store.ts');
    useMusic.setState(s=>({playback:{...s.playback,isPlaying:false}}));})()`);
  for (const activity of ["timer", "recording", "microphone", "downloads", "sessions", "agents", "calendar", "watch", "keepAwake"]) {
    const result = await evaluate(`(async () => {
      const {useIsland}=await import('/src/island/store.ts');
      const errors=[]; const originalError=console.error;
      console.error=(...parts)=>{errors.push(parts.join(' '));originalError(...parts)};
      useIsland.setState(s=>({expanded:false,notice:null,peeking:false,dropHint:false,activityPickerOpen:false,
        activities:{${activity}:{wing:56,capsuleContent:80,opens:'${["recording", "microphone", "keepAwake"].includes(activity) ? "controls" : activity}'}},
        pickedActivity:'${activity}'}));
      for(let i=0;i<8;i++) await new Promise(requestAnimationFrame);
      const frame=document.querySelector('.island-frame').getBoundingClientRect();
      const portrait=document.querySelector('.n-activity-portrait');
      const content=portrait?.firstElementChild?.getBoundingClientRect();
      const outside=portrait?[...portrait.querySelectorAll('*')].filter(el=>{
        const r=el.getBoundingClientRect();
        if(r.width===0||r.height===0||getComputedStyle(el).opacity==='0') return false;
        return r.left<frame.left-2||r.right>frame.right+2||r.top<frame.top-2||r.bottom>frame.bottom+2;
      }).map(el=>el.className?.baseVal??el.className).slice(0,5):[];
      console.error=originalError;
      return {frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,
        width:frame.width,height:frame.height},portrait:!!portrait,
        content:content&&{left:content.left,right:content.right,top:content.top,bottom:content.bottom},
        errors,outside};
    })()`);
    assert.equal(result.errors.length,0,`Activity ${activity} logged an error: ${result.errors.join('; ')}`);
    assert(result.portrait && result.frame.width<=32 && result.frame.height>result.frame.width,
      `Activity ${activity} did not turn vertical: ${JSON.stringify(result)}`);
    assert.equal(result.outside.length,0,`Activity ${activity} has clipped elements: ${JSON.stringify(result.outside)}`);
    if (result.content) assert(result.content.left>=result.frame.left-1&&result.content.right<=result.frame.right+1&&
      result.content.top>=result.frame.top-1&&result.content.bottom<=result.frame.bottom+1,
      `Activity ${activity} escaped its capsule: ${JSON.stringify(result)}`);
  }
  const activityHit = await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:false,notice:null,peeking:false,dropHint:false,
      activityPickerOpen:false,activities:{music:{wing:56,capsuleContent:80,opens:'music'}},
      pickedActivity:'music',settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,300));
    const r=document.querySelector('.n-activity-portrait .m-strip').getBoundingClientRect();
    return{x:r.left+r.width/2,y:r.top+r.height/2};
  })()`);
  await send("Input.dispatchMouseEvent",{type:"mousePressed",x:activityHit.x,y:activityHit.y,button:"left",clickCount:1});
  await send("Input.dispatchMouseEvent",{type:"mouseReleased",x:activityHit.x,y:activityHit.y,button:"left",clickCount:1});
  assert(await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');
    return useIsland.getState().expanded&&useIsland.getState().selected==='music'})()`),
    "The centre of the closed music activity did not open on the first click");
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  for (const x of [0, 1]) for (const fraction of [0.15, 0.5, 0.85]) {
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 525, y: 380 });
    const point = await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState(s=>({expanded:false,activityPickerOpen:false,notice:null,peeking:false,
        hoverEmphasized:false,hoverSuppressed:false,activities:{
          music:{wing:56,capsuleContent:80,opens:'music'},
          sessions:{wing:56,capsuleContent:80,opens:'sessions'}},pickedActivity:'music',
        settings:{...s.settings,position:{x:${x},y:0.5},opening:'click'}}));
      await new Promise(r=>setTimeout(r,350));
      const r=document.querySelector('.island-backdrop path[id]').getBoundingClientRect();
      const strip=document.querySelector('.n-activity-picker-strip').getBoundingClientRect();
      return {x:r.left+r.width/2,y:r.top+r.height*${fraction},stripY:strip.top+strip.height/2};
    })()`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y });
    await new Promise((r) => setTimeout(r, 250));
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x: point.x, y: point.y, button: "left", clickCount: 1 });
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: point.x, y: point.y, button: "left", clickCount: 1 });
    if (fraction === 0.5) {
      const centers = await evaluate(`(async()=>{
        const values=[];
        for(let i=0;i<18;i++){
          await new Promise(requestAnimationFrame);
          const r=document.querySelector('.n-activity-picker-strip')?.getBoundingClientRect();
          if(r)values.push(r.top+r.height/2);
        }
        return values;
      })()`);
      assert(centers.length > 8 && Math.max(...centers, point.stripY)-Math.min(...centers, point.stripY) < 4,
        `Side strip jumped during picker animation at ${x}: ${JSON.stringify(centers)}`);
    }
    await new Promise((r) => setTimeout(r, 450));
    const opened = await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');
      return {picker:useIsland.getState().activityPickerOpen,expanded:useIsland.getState().expanded}})()`);
    assert(opened.picker && !opened.expanded,
      `Side activity did not stay open on the first click at ${x}, ${fraction}: ${JSON.stringify(opened)}`);
  }
  const pickerMotion = await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const original=document.querySelector('.n-activity-picker');
    const samples=[];
    for(const open of [false,true,false,true]){
      useIsland.setState({activityPickerOpen:open});
      for(let i=0;i<15;i++){
        await new Promise(requestAnimationFrame);
        const menu=document.querySelector('.n-activity-picker');
        const button=menu?.querySelector('.n-activity-choice');
        const selected=menu?.querySelector('.n-activity-choice.is-selected');
        const highlight=selected?.querySelector('.n-activity-choice-highlight');
        const box=button?.getBoundingClientRect();
        const active=selected?.getBoundingClientRect();
        const white=highlight?.getBoundingClientRect();
        const strip=menu?.parentElement?.querySelector('.n-activity-picker-strip')?.getBoundingClientRect();
        const surface=document.querySelector('.island-backdrop path[id]')?.getBoundingClientRect();
        samples.push({same:menu===original,width:box?.width,height:box?.height,left:box?.left,
          stripLeft:strip?.left,stripRight:strip?.right,surfaceRight:surface?.right,
          opacity:menu?Number(getComputedStyle(menu).opacity):0,
          highlightInside:!!white&&!!active&&white.left>=active.left-0.5&&white.right<=active.right+0.5&&
            white.top>=active.top-0.5&&white.bottom<=active.bottom+0.5});
      }
    }
    return samples;
  })()`);
  assert(pickerMotion.every(s=>s.surfaceRight-s.stripRight>=-5&&s.surfaceRight-s.stripRight<32),
    `Right-edge activity strip escaped the animated surface: ${JSON.stringify(pickerMotion.filter(s=>s.surfaceRight-s.stripRight < -5).slice(0,3))}`);
  assert(pickerMotion.every((s,i)=>!i||Math.abs(s.stripLeft-pickerMotion[i-1].stripLeft)<8),
    "Right-edge activity icons jumped between animation frames");
  assert(pickerMotion.every(s=>s.same&&Math.abs(s.width-104)<0.1&&s.height===36&&s.highlightInside),
    `Side picker buttons or selection moved during rapid toggles: ${JSON.stringify(pickerMotion.filter(s=>!s.same||Math.abs(s.width-104)>=0.1||s.height!==36||!s.highlightInside).slice(0,3))}`);
  assert(pickerMotion.some(s=>s.opacity>0&&s.opacity<1),
    "Side picker content did not fade with the surface animation");
  for(let start=0;start<pickerMotion.length;start+=15){
    const shown=pickerMotion.slice(start,start+15).filter(s=>s.opacity>0.05);
    if(shown.length>1) assert(Math.max(...shown.map(s=>s.left))-Math.min(...shown.map(s=>s.left))<4,
      `Side picker choices drifted while visible: ${JSON.stringify(shown.map(s=>s.left))}`);
  }
  const leftMotion=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({activityPickerOpen:false,pickedActivity:'sessions',
      settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,500));
    const read=()=>{const strip=document.querySelector('.n-activity-picker-strip')?.getBoundingClientRect();
      const surface=document.querySelector('.island-backdrop path[id]')?.getBoundingClientRect();
      return {left:strip?.left,gap:strip?.left-surface?.left};};
    const start=read();const samples=[];
    for(const open of [true,false,true,false]){
      useIsland.setState({activityPickerOpen:open});
      for(let i=0;i<15;i++){await new Promise(requestAnimationFrame);samples.push(read());}
    }
    await new Promise(r=>setTimeout(r,500));
    return {start,finish:read(),samples};
  })()`);
  assert(leftMotion.samples.every(s=>s.gap>=-5&&s.gap<32),
    `Left-edge activity strip escaped the animated surface: ${JSON.stringify(leftMotion.samples.filter(s=>s.gap < -5).slice(0,3))}`);
  assert(leftMotion.samples.every((s,i)=>!i||Math.abs(s.left-leftMotion.samples[i-1].left)<8),
    "Left-edge activity icons jumped between animation frames");
  assert(Math.abs(leftMotion.finish.left-leftMotion.start.left)<1,
    `Left-edge activity icons did not return to their closed position: ${JSON.stringify(leftMotion)}`);
  const fourChoices=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState({activityPickerOpen:true,pickedActivity:'sessions',activities:{
      recording:{wing:56,capsuleContent:80,opens:'tools'},
      sessions:{wing:56,capsuleContent:80,opens:'sessions'},
      agents:{wing:56,capsuleContent:80,opens:'agents'},
      music:{wing:56,capsuleContent:80,opens:'music'}}});
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    return [...document.querySelectorAll('.n-activity-choice')].map(el=>{
      const box=el.getBoundingClientRect();
      return {label:el.textContent,width:box.width,height:box.height,
        inside:box.left>=frame.left-1&&box.right<=frame.right+1&&
          box.top>=frame.top-1&&box.bottom<=frame.bottom+1};
    });
  })()`);
  assert(fourChoices.length===4&&fourChoices.every(b=>b.inside&&Math.abs(b.width-104)<0.1&&b.height===36),
    `Four side activity buttons do not fit: ${JSON.stringify(fourChoices)}`);
  await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState({activityPickerOpen:false})})()`);
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  for (const selected of ["controls", "mixer", "music", "clipboard", "captures", "files", "system", "tools",
    "calendar", "notifications", "timer", "camera", "downloads", "scratchpad", "agents", "sessions", "watch"]) {
    if (selected === "mixer") await evaluate(`(async()=>{
      const {fillDemo}=await import('/src/controls/demo.ts'); fillDemo();
    })()`);
    const result = await evaluate(`(async () => {
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState(s => ({expanded:true,selected:'${selected}',showingSections:false,
        notice:null,peeking:false,dropHint:false,settings:{...s.settings,position:{x:0.98,y:0.5}}}));
      await new Promise(r=>setTimeout(r,500));
      const frame=document.querySelector('.island-frame').getBoundingClientRect();
      const panel=document.querySelector('.n-expanded-vertical');
      const page=panel?.querySelector('.n-page')?.getBoundingClientRect();
      const controls=panel?.querySelector('.n-portrait-leading')?.getBoundingClientRect();
      const footer=panel?.querySelector('.n-portrait-trailing')?.getBoundingClientRect();
      return {portrait:!!panel,frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom},
        page:page&&{left:page.left,right:page.right,top:page.top,bottom:page.bottom},
        controls:controls&&{bottom:controls.bottom},footer:footer&&{top:footer.top}};
    })()`);
    assert(result.portrait && result.page, `No portrait page for ${selected}`);
    assert(result.page.left >= result.frame.left - 1 && result.page.right <= result.frame.right + 1 &&
      result.page.top >= result.frame.top - 1 && result.page.bottom <= result.frame.bottom + 1,
      `Portrait page escaped the panel: ${selected}`);
    assert((!result.controls || result.controls.bottom <= result.page.top) &&
      (!result.footer || result.page.bottom <= result.footer.top),
      `Portrait page overlaps controls: ${selected}`);
    if (selected === "mixer") {
      const tracks = await evaluate(`[...document.querySelectorAll('.mx-desk .mx-track .u-slider-track')]
        .map(el=>el.getBoundingClientRect().height)`);
      assert(tracks.length >= 2 && tracks.every(height => height >= 32),
        `Portrait mixer faders have no visible tracks: ${JSON.stringify(tracks)}`);
      const mixerShot = await send("Page.captureScreenshot", { format: "png" });
      await writeFile(resolve("../build/island-position-side-mixer-page.png"), Buffer.from(mixerShot.data, "base64"));
    }
    if(selected==='music') {
      const musicPageShot=await send("Page.captureScreenshot",{format:"png"});
      await writeFile(resolve("../build/island-position-side-music-page.png"),Buffer.from(musicPageShot.data,"base64"));
    }
  }
  const musicStable = await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const {useMusic}=await import('/src/music/store.ts');
    useIsland.setState(s=>({expanded:true,selected:'music',showingSections:false,
      settings:{...s.settings,position:{x:0,y:0.5}}}));
    const start=useMusic.getState().playback;
    useMusic.setState(s=>({playback:{...s.playback,isPlaying:true,duration:720,
      title:'A very long track title to test scrolling without changing the panel geometry'}}));
    await new Promise(r=>setTimeout(r,500));
    const sizes=[];
    for(let i=0;i<12;i++){
      useMusic.setState(s=>({playback:{...s.playback,elapsed:i*37,sampledAt:Date.now()/1000}}));
      await new Promise(r=>setTimeout(r,90));
      const r=document.querySelector('.island-frame').getBoundingClientRect();
      sizes.push([r.left,r.top,r.width,r.height,useIsland.getState().pageContent.music]);
    }
    useMusic.setState({playback:start});
    return sizes;
  })()`);
  for (const sample of musicStable) for(let i=0;i<sample.length;i++)
    assert(Math.abs(sample[i]-musicStable[0][i])<=1,
      `Music playback moved the panel: ${JSON.stringify(musicStable)}`);
  const systemScroll = await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const {useSystem}=await import('/src/system/store.ts');
    const snapshot={seq:1,supported:{cpu:true,gpu:true,memory:true,network:true,disk:true,devices:false},
      cpu:0.42,gpu:0.24,memoryUsed:8e9,memoryTotal:16e9,memoryPressure:0.5,
      memoryCompressed:0,memoryCached:0,memorySwapUsed:0,battery:null,netDown:1024,netUp:512,
      netTotalDown:10000,netTotalUp:5000,disk:{name:'C:',free:200e9,total:500e9},
      diskRead:2048,diskWrite:1024,devices:null,processes:[],processStatus:'ready',
      processesFor:null,uptimeSeconds:3600};
    useSystem.getState().receive(snapshot);
    useIsland.setState(s=>({expanded:true,selected:'system',showingSections:false,
      settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,400));
    const grid=document.querySelector('.sy-grid.is-portrait');
    if(!grid) return null;
    const before={scrollHeight:grid.scrollHeight,clientHeight:grid.clientHeight,
      scrollTop:grid.scrollTop,frame:document.querySelector('.island-frame').getBoundingClientRect().height};
    grid.scrollTop=grid.scrollHeight;
    useSystem.getState().receive({...snapshot,seq:2,cpu:0.58,netDown:4096});
    await new Promise(r=>setTimeout(r,100));
    const last=[...grid.querySelectorAll('.sy-card')].at(-1);
    const visible=last?.getBoundingClientRect().bottom<=grid.getBoundingClientRect().bottom+1;
    return {before,after:grid.scrollTop,visible,frame:document.querySelector('.island-frame').getBoundingClientRect().height};
  })()`);
  assert(systemScroll && systemScroll.before.scrollHeight>systemScroll.before.clientHeight &&
    systemScroll.after>0 && systemScroll.visible && Math.abs(systemScroll.frame-systemScroll.before.frame)<=1,
    `System grid did not scroll stably: ${JSON.stringify(systemScroll)}`);
  const wheelPoint = await evaluate(`(()=>{const grid=document.querySelector('.sy-grid.is-portrait');
    grid.scrollTop=0;const r=grid.getBoundingClientRect();return{x:r.left+r.width/2,y:r.top+r.height/2}})()`);
  await send("Input.dispatchMouseEvent",{type:"mouseWheel",x:wheelPoint.x,y:wheelPoint.y,deltaX:0,deltaY:240});
  await new Promise(r=>setTimeout(r,180));
  assert(await evaluate(`document.querySelector('.sy-grid.is-portrait').scrollTop>0`),
    "Mouse wheel did not scroll the portrait System grid");
  await send("Input.dispatchMouseEvent",{type:"mouseWheel",x:wheelPoint.x,y:wheelPoint.y,deltaX:0,deltaY:-240});
  await new Promise(r=>setTimeout(r,180));
  assert(await evaluate(`document.querySelector('.sy-grid.is-portrait').scrollTop===0`),
    "Mouse wheel did not scroll the portrait System grid back to its start");
  const gallerySide = await evaluate(`(async () => {
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:true,showingSections:true,capturePreview:null,
      settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,450));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const gallery=document.querySelector('.n-gallery').getBoundingClientRect();
    const tiles=[...document.querySelectorAll('.n-tile')].map(el=>el.getBoundingClientRect());
    const header=document.querySelector('.n-portrait-leading').getBoundingClientRect();
    return {frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,
      width:frame.width,height:frame.height},gallery:{left:gallery.left,right:gallery.right,
      top:gallery.top,bottom:gallery.bottom},header:{bottom:header.bottom},
      tiles:tiles.map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height}))};
  })()`);
  assert(gallerySide.frame.height>gallerySide.frame.width && gallerySide.tiles.length>=8,
    `The side module gallery is not portrait: ${JSON.stringify(gallerySide)}`);
  assert(gallerySide.header.bottom<=gallerySide.gallery.top &&
    gallerySide.gallery.bottom<=gallerySide.frame.bottom+1,
    "The side gallery overlaps its header or frame");
  for(const tile of gallerySide.tiles.slice(0,8)) assert(tile.left>=gallerySide.frame.left-1&&
    tile.right<=gallerySide.frame.right+1&&tile.top>=gallerySide.frame.top-1&&
    tile.bottom<=gallerySide.frame.bottom+1&&tile.width>80&&tile.height>80,
    `A side gallery tile is clipped or undersized: ${JSON.stringify(tile)}`);
  const galleryShot = await send("Page.captureScreenshot", {format:"png"});
  await writeFile(resolve("../build/island-position-side-gallery.png"),Buffer.from(galleryShot.data,"base64"));
  const noticePreviewSide = await evaluate(`(async () => {
    const {useIsland}=await import('/src/island/store.ts');
    const notification={id:'side-notification',app:'Discord',aumid:'Discord',time:1793450000,
      title:'Example sender',sender:'Example sender',body:'A message on a side display'};
    useIsland.setState(s=>({expanded:false,noticeExpanded:true,peeking:false,dropHint:false,
      notice:{id:'side-notice',kind:'notification',title:'Example sender',detail:'A message',
        priority:1,duration:20,notification},
      settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const preview=document.querySelector('.nt-preview.is-vertical')?.getBoundingClientRect();
    const actions=[...document.querySelectorAll('.nt-preview-actions button')].map(el=>el.getBoundingClientRect());
    return {frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,
      width:frame.width,height:frame.height},
      preview:preview&&{left:preview.left,right:preview.right,top:preview.top,bottom:preview.bottom},
      actions:actions.map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom}))};
  })()`);
  assert(noticePreviewSide.frame.height>noticePreviewSide.frame.width && noticePreviewSide.preview,
    "Side notification preview did not become portrait");
  for (const box of [noticePreviewSide.preview,...noticePreviewSide.actions]) assert(
    box.left>=noticePreviewSide.frame.left-1&&box.right<=noticePreviewSide.frame.right+1&&
    box.top>=noticePreviewSide.frame.top-1&&box.bottom<=noticePreviewSide.frame.bottom+1,
    "Side notification preview escaped its frame");
  const previewShot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../build/island-position-side-notification.png"), Buffer.from(previewShot.data, "base64"));
  await send("Input.dispatchMouseEvent",{type:"mouseMoved",x:500,y:500});
  const pickerSide = await evaluate(`(async () => {
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:false,notice:null,noticeExpanded:false,peeking:false,
      dropHint:false,settings:{...s.settings,position:{x:1,y:0.5}}}));
    useIsland.setState({activities:{music:{wing:56,capsuleContent:80,opens:'music'},
      sessions:{wing:56,capsuleContent:80,opens:'sessions'},
      agents:{wing:56,capsuleContent:80,opens:'agents'}},
      pickedActivity:'music',activityPickerOpen:true,hoverSuppressed:false,attentionFullscreen:false});
    for(let i=0;i<8;i++) await new Promise(requestAnimationFrame);
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const picker=document.querySelector('.n-compact-activity.is-vertical.is-choosing');
    const strip=picker?.querySelector('.n-activity-picker-strip')?.getBoundingClientRect();
    const choices=[...document.querySelectorAll('.n-activity-choice')].map(el=>el.getBoundingClientRect());
    return {frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,
      width:frame.width,height:frame.height},picker:!!picker,
      strip:strip&&{left:strip.left,right:strip.right},
      choices:choices.map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height}))};
  })()`);
  assert(pickerSide.picker && pickerSide.frame.height>pickerSide.frame.width && pickerSide.choices.length===3,
    `Side activity chooser did not become portrait: ${JSON.stringify(pickerSide)}`);
  const pickerBodyInset=Math.min(14,pickerSide.frame.width/8);
  assert(pickerSide.strip.left-pickerSide.choices[0].right>=8,
    "Right-side activity menu does not separate the strip from its buttons");
  for (const [index,box] of pickerSide.choices.entries()) {
    assert(box.left>=pickerSide.frame.left+pickerBodyInset+4&&
      box.right<=pickerSide.frame.right-pickerBodyInset-4&&
      box.top>=pickerSide.frame.top+8&&box.bottom<=pickerSide.frame.bottom-8,
      `Side activity choice touched the capsule edge: ${JSON.stringify(pickerSide)}`);
    assert(box.width===pickerSide.choices[0].width&&box.height===pickerSide.choices[0].height,
      "Side activity choices have unequal sizes");
    if(index) assert(box.top-pickerSide.choices[index-1].bottom>=8,
      "Side activity choices are too close together");
  }
  const pickerShot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../build/island-position-side-picker.png"), Buffer.from(pickerShot.data, "base64"));
  const leftPicker=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:false,activityPickerOpen:true,pickedActivity:'music',
      notice:null,peeking:false,settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,300));
    const strip=document.querySelector('.n-activity-picker-strip').getBoundingClientRect();
    const first=document.querySelector('.n-activity-choice').getBoundingClientRect();
    return{stripRight:strip.right,buttonLeft:first.left};
  })()`);
  assert(leftPicker.buttonLeft-leftPicker.stripRight>=8,
    `Left-side activity menu does not open inward: ${JSON.stringify(leftPicker)}`);
  for (const id of ["sessions","music","agents"]) {
    await send("Input.dispatchMouseEvent",{type:"mouseMoved",x:500,y:500});
    await new Promise(r=>setTimeout(r,100));
    const button=await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState({expanded:false,activityPickerOpen:true,pickedActivity:'music',
        activities:{music:{wing:56,capsuleContent:80,opens:'music'},
          sessions:{wing:56,capsuleContent:80,opens:'sessions'},
          agents:{wing:56,capsuleContent:80,opens:'agents'}},
        notice:null,peeking:false,hoverSuppressed:false,attentionFullscreen:false});
      await new Promise(r=>setTimeout(r,240));
      const el=[...document.querySelectorAll('.n-activity-choice')]
        .find(el=>el.textContent.toLowerCase().includes('${id==='agents'?'agent':id==='sessions'?'session':'music'}'));
      if(!el) return {error:[...document.querySelectorAll('.n-activity-choice')].map(el=>el.textContent),
        picker:useIsland.getState().activityPickerOpen,hoverSuppressed:useIsland.getState().hoverSuppressed};
      const r=el.getBoundingClientRect();return{x:r.left+r.width/2,y:r.top+r.height/2};
    })()`);
    assert(button&&!button.error,`Side activity picker did not show ${id}: ${JSON.stringify(button)}`);
    await send("Input.dispatchMouseEvent",{type:"mousePressed",x:button.x,y:button.y,button:"left",clickCount:1});
    await send("Input.dispatchMouseEvent",{type:"mouseReleased",x:button.x,y:button.y,button:"left",clickCount:1});
    const opened=await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');
      await new Promise(r=>setTimeout(r,240));
      return {expanded:useIsland.getState().expanded,selected:useIsland.getState().selected,
        picked:useIsland.getState().pickedActivity,vertical:!!document.querySelector('.n-expanded-vertical')};
    })()`);
    assert(opened.expanded&&opened.selected===id&&opened.picked===id&&opened.vertical,
      `Side activity button did not open ${id}: ${JSON.stringify(opened)}`);
  }
  const liveSessionsSide = await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const {useSessions}=await import('/src/sessions/store.ts');
    const {demoSnapshot}=await import('/src/sessions/demo.ts');
    window.__previousSessionSnapshot=useSessions.getState().snapshot;
    useIsland.setState({autoPresentAllowed:false,expanded:false,notice:null,capturePreview:null});
    const sample=demoSnapshot();
    useSessions.setState({snapshot:{...sample,sessions:sample.sessions.filter(s=>!s.phase.startsWith('waiting'))}});
    useIsland.setState(s=>({expanded:false,activityPickerOpen:false,notice:null,capturePreview:null,
      activities:{sessions:{wing:56,capsuleContent:80,opens:'sessions'}},pickedActivity:'sessions',
      settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const values=[...document.querySelectorAll('.s-strip-count')].map(el=>el.getBoundingClientRect());
    return {state:{expanded:useIsland.getState().expanded,selected:useIsland.getState().selected,
      picker:useIsland.getState().activityPickerOpen,notice:!!useIsland.getState().notice},
      frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,width:frame.width},
      values:values.map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom}))};
  })()`);
  assert(liveSessionsSide.frame.width<=32 && liveSessionsSide.values.length>=2,
    `Real Sessions activity did not remain slim: ${JSON.stringify(liveSessionsSide)}`);
  for(const [i,value] of liveSessionsSide.values.entries()) assert(value.left>=liveSessionsSide.frame.left-1&&
    value.right<=liveSessionsSide.frame.right+1&&value.top>=liveSessionsSide.frame.top-1&&
    value.bottom<=liveSessionsSide.frame.bottom+1&&
    (!i||value.top>=liveSessionsSide.values[i-1].bottom-1),
    `Sessions readings are not vertically stacked: ${JSON.stringify(liveSessionsSide)}`);
  const sessionsShot=await send("Page.captureScreenshot",{format:"png"});
  await writeFile(resolve("../build/island-position-side-sessions.png"),Buffer.from(sessionsShot.data,"base64"));
  const focusedSession=await evaluate(`(async()=>{
    const {useSessions}=await import('/src/sessions/store.ts');
    const session=useSessions.getState().snapshot.sessions[0];
    window.__previousFocusedSession=useSessions.getState().focused;
    window.__previousSessionNames=useSessions.getState().names;
    useSessions.setState({focused:session.id,focusedRequest:null,
      names:{...useSessions.getState().names,[session.id]:'Dodaj swobodne pozycjonowanie Dynamic Island na wszystkich krawędziach ekranu'}});
    useSessions.getState().setDraft(session.id,'Retained reply draft');
    return session.id;
  })()`);
  for (const position of [{x:0,y:0.5},{x:1,y:0.5},{x:0,y:0.08},{x:1,y:0.92},
    {x:0.5,y:0},{x:0.5,y:1}]) {
    await send("Input.dispatchMouseEvent",{type:"mouseMoved",x:500,y:500});
    const expandButton=await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState(s=>({expanded:true,selected:'sessions',sessionsEnlarged:false,
        notice:null,peeking:false,showingSections:false,
        settings:{...s.settings,position:{x:${position.x},y:${position.y}}}}));
      await new Promise(r=>setTimeout(r,400));
      window.__compactSessionPage=document.querySelector('.s-detail-page');
      const row=document.querySelector('.n-portrait-trailing,.n-row-trailing');
      const r=row?.querySelectorAll('button')[1]?.getBoundingClientRect();
      return r&&{x:r.left+r.width/2,y:r.top+r.height/2};
    })()`);
    assert(expandButton,`Missing Sessions expand button at ${JSON.stringify(position)}`);
    await send("Input.dispatchMouseEvent",{type:"mousePressed",x:expandButton.x,y:expandButton.y,button:"left",clickCount:1});
    await send("Input.dispatchMouseEvent",{type:"mouseReleased",x:expandButton.x,y:expandButton.y,button:"left",clickCount:1});
    const full=await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      const {useSessions}=await import('/src/sessions/store.ts');
      await new Promise(r=>setTimeout(r,650));
      const r=document.querySelector('.island-frame').getBoundingClientRect();
      const title=document.querySelector('.s-back span');
      const leading=document.querySelector('.n-portrait-leading');
      const trailing=document.querySelector('.n-portrait-trailing');
      const heading=document.querySelector('.s-back');
      return {large:useIsland.getState().sessionsEnlarged,
        frame:{left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height},
        detail:!!document.querySelector('.s-detail-page.is-large'),
        conversation:!!document.querySelector('.s-conversation.is-large'),
        sidebar:!!document.querySelector('.s-details-sidebar'),
        composer:!!document.querySelector('.s-composer'),
        samePage:window.__compactSessionPage===document.querySelector('.s-detail-page'),
        focused:useSessions.getState().focused,draft:useSessions.getState().drafts['${focusedSession}'],
        title: title?.textContent,
        titleFits: title ? title.scrollWidth<=title.clientWidth+1 && title.scrollHeight<=title.clientHeight+1 : false,
        titleEllipsis: title ? getComputedStyle(title).textOverflow==='ellipsis' && getComputedStyle(title).whiteSpace==='nowrap' : false,
        headingFits: !!heading && !!leading && heading.getBoundingClientRect().width<=leading.getBoundingClientRect().width+1,
        headerRowsFit: !leading || !trailing || leading.getBoundingClientRect().bottom<=trailing.getBoundingClientRect().top+1,
        shrink:!!document.querySelector('.n-portrait-trailing,.n-row-trailing')?.querySelectorAll('button')[1]};
    })()`);
    assert(full.large&&full.detail&&full.conversation&&full.sidebar&&full.composer&&full.shrink&&
      full.title==='Dodaj swobodne pozycjonowanie Dynamic Island na wszystkich krawędziach ekranu'&&
      ((position.x===0||position.x===1)?full.titleEllipsis&&full.headingFits&&full.headerRowsFit:true)&&
      full.focused===focusedSession&&full.draft==='Retained reply draft'&&
      full.frame.width>400&&full.frame.height>400&&
      full.frame.left>=-1&&full.frame.right<=1051&&full.frame.top>=-1&&full.frame.bottom<=761&&
      ((position.x===0||position.x===1)?full.samePage:true),
      `Sessions did not enlarge at ${JSON.stringify(position)}: ${JSON.stringify(full)}`);
    if(position.x===0&&position.y===0.5){
      const shortHeader=await evaluate(`(()=>{const button=document.querySelector('.s-back');
        const label=button.querySelector('span');const original=label.textContent;label.textContent='Short session';
        const width=button.getBoundingClientRect().width;const row=button.parentElement.getBoundingClientRect().width;
        label.textContent=original;return {width,row}})()`);
      assert(shortHeader.width<shortHeader.row/2,
        `Expanded Sessions header did not fit its short title: ${JSON.stringify(shortHeader)}`);
      const fullShot=await send("Page.captureScreenshot",{format:"png"});
      await writeFile(resolve("../build/island-position-side-sessions-full.png"),Buffer.from(fullShot.data,"base64"));
    }
    const shrinkButton=await evaluate(`(()=>{const r=document.querySelector('.n-portrait-trailing,.n-row-trailing')
      .querySelectorAll('button')[1].getBoundingClientRect();return{x:r.left+r.width/2,y:r.top+r.height/2}})()`);
    await send("Input.dispatchMouseEvent",{type:"mousePressed",x:shrinkButton.x,y:shrinkButton.y,button:"left",clickCount:1});
    await send("Input.dispatchMouseEvent",{type:"mouseReleased",x:shrinkButton.x,y:shrinkButton.y,button:"left",clickCount:1});
    const compact=await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      const {useSessions}=await import('/src/sessions/store.ts');
      await new Promise(r=>setTimeout(r,550));
      return {large:useIsland.getState().sessionsEnlarged,
        width:document.querySelector('.island-frame').getBoundingClientRect().width,
        focused:useSessions.getState().focused,draft:useSessions.getState().drafts['${focusedSession}'],
        samePage:window.__compactSessionPage===document.querySelector('.s-detail-page')};
    })()`);
    assert(!compact.large&&((position.x===0||position.x===1)?compact.width<full.frame.width:true)&&compact.focused===focusedSession&&
      compact.draft==='Retained reply draft'&&((position.x===0||position.x===1)?compact.samePage:true),
      `Sessions did not shrink at ${JSON.stringify(position)}: ${JSON.stringify(compact)}`);
    if(position.x===0&&position.y===0.5){
      const shortHeader=await evaluate(`(()=>{const button=document.querySelector('.s-back');
        const label=button.querySelector('span');const original=label.textContent;label.textContent='Short session';
        const width=button.getBoundingClientRect().width;const row=button.parentElement.getBoundingClientRect().width;
        label.textContent=original;return {width,row}})()`);
      assert(shortHeader.width<shortHeader.row/2,
        `Compact Sessions header did not fit its short title: ${JSON.stringify(shortHeader)}`);
    }
  }
  await evaluate(`(async()=>{const {useSessions}=await import('/src/sessions/store.ts');
    useSessions.setState({snapshot:window.__previousSessionSnapshot,focused:window.__previousFocusedSession,
      names:window.__previousSessionNames})})()`);
  const liveTimerSide=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const {useTimer}=await import('/src/timer/store.ts');
    const model=await import('/src/timer/model.ts');
    window.__previousTimerState=useTimer.getState();
    useTimer.setState({session:model.start(model.IDLE,'stopwatch',0,0),now:123});
    useIsland.setState(s=>({expanded:false,notice:null,activityPickerOpen:false,
      activities:{timer:{wing:56,capsuleContent:80,opens:'timer'}},pickedActivity:'timer',
      settings:{...s.settings,position:{x:1,y:0.5}}}));
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const reading=document.querySelector('.tm-strip-reading.is-vertical');
    const lines=[...reading.children].map(el=>el.getBoundingClientRect());
    return {width:frame.width,reading:reading.textContent,
      lines:lines.map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom})),
      frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom}};
  })()`);
  assert(liveTimerSide.width<=32&&liveTimerSide.lines.length===2&&
    liveTimerSide.lines.every((r,i)=>r.left>=liveTimerSide.frame.left-1&&r.right<=liveTimerSide.frame.right+1&&
      r.top>=liveTimerSide.frame.top-1&&r.bottom<=liveTimerSide.frame.bottom+1&&
      (!i||r.top>=liveTimerSide.lines[i-1].bottom-1)),
    `Stopwatch reading did not stack in its slim capsule: ${JSON.stringify(liveTimerSide)}`);
  const timerShot=await send("Page.captureScreenshot",{format:"png"});
  await writeFile(resolve("../build/island-position-side-timer.png"),Buffer.from(timerShot.data,"base64"));
  await evaluate(`(async()=>{const {useTimer}=await import('/src/timer/store.ts');
    useTimer.setState(window.__previousTimerState)})()`);
  const liveAgentsSide=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const {useAgents}=await import('/src/agents/store.ts');
    const {demoSnapshot}=await import('/src/agents/demo.ts');
    window.__previousAgentsSnapshot=useAgents.getState().snapshot;
    useAgents.setState({snapshot:demoSnapshot()});
    useIsland.setState(s=>({expanded:false,notice:null,activityPickerOpen:false,
      activities:{agents:{wing:56,capsuleContent:80,opens:'agents'}},pickedActivity:'agents',
      settings:{...s.settings,position:{x:0,y:0.5},agentsReadout:'limit'}}));
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const mark=document.querySelector('.ag-strip-wing:first-child').getBoundingClientRect();
    const glyph=document.querySelector('.ag-glyph')?.getBoundingClientRect();
    const readingElement=document.querySelector('.ag-strip-reading');
    const reading=readingElement.getBoundingClientRect();
    return {frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,width:frame.width},
      mark:{left:mark.left,right:mark.right,bottom:mark.bottom},
      glyph:glyph&&{left:glyph.left,right:glyph.right},
      reading:{left:reading.left,right:reading.right,top:reading.top,bottom:reading.bottom,
        scrollWidth:readingElement.scrollWidth,clientWidth:readingElement.clientWidth}};
  })()`);
  assert(liveAgentsSide.frame.width<=32&&liveAgentsSide.mark.bottom<liveAgentsSide.reading.top&&
    (!liveAgentsSide.glyph || liveAgentsSide.glyph.left>=liveAgentsSide.frame.left-1&&
      liveAgentsSide.glyph.right<=liveAgentsSide.frame.right+1)&&
    liveAgentsSide.reading.left>=liveAgentsSide.frame.left-1&&
    liveAgentsSide.reading.right<=liveAgentsSide.frame.right+1&&
    liveAgentsSide.reading.scrollWidth<=liveAgentsSide.reading.clientWidth+1,
    `Agent mark and limit did not stack in the slim capsule: ${JSON.stringify(liveAgentsSide)}`);
  const agentsShot=await send("Page.captureScreenshot",{format:"png"});
  await writeFile(resolve("../build/island-position-side-agents.png"),Buffer.from(agentsShot.data,"base64"));
  await evaluate(`(async()=>{const {useAgents}=await import('/src/agents/store.ts');
    useAgents.setState({snapshot:window.__previousAgentsSnapshot})})()`);
  const liveCalendarSide=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const {useCalendar}=await import('/src/calendar/store.ts');
    const {demoCalendar}=await import('/src/calendar/demo.ts');
    window.__previousCalendarCountdown=useCalendar.getState().countdown;
    useIsland.setState(s=>({expanded:false,notice:null,activityPickerOpen:false,
      activities:{calendar:{wing:56,capsuleContent:80,opens:'calendar'}},pickedActivity:'calendar',
      settings:{...s.settings,position:{x:1,y:0.5},calendarTimeLeft:true}}));
    useCalendar.setState({countdown:{event:demoCalendar('allowed').events[0],ongoing:true}});
    useIsland.setState({activities:{calendar:{wing:56,capsuleContent:80,opens:'calendar'}},pickedActivity:'calendar'});
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const title=document.querySelector('.cal-strip-title');
    const clock=document.querySelector('.cal-strip-clock.is-vertical');
    const parts=[title,...(clock?.children??[])].filter(Boolean).map(el=>el.getBoundingClientRect());
    return {state:{expanded:useIsland.getState().expanded,notice:!!useIsland.getState().notice,
      picked:useIsland.getState().pickedActivity},width:frame.width,title:title?.textContent,
      lines:clock?.children.length??0,
      frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom},
      parts:parts.map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom}))};
  })()`);
  assert(liveCalendarSide.width<=32&&liveCalendarSide.title==='DR'&&liveCalendarSide.lines===2&&
    liveCalendarSide.parts.every(r=>r.left>=liveCalendarSide.frame.left-1&&
      r.right<=liveCalendarSide.frame.right+1&&r.top>=liveCalendarSide.frame.top-1&&
      r.bottom<=liveCalendarSide.frame.bottom+1),
    `Calendar title or countdown did not fit the slim capsule: ${JSON.stringify(liveCalendarSide)}`);
  const calendarShot=await send("Page.captureScreenshot",{format:"png"});
  await writeFile(resolve("../build/island-position-side-calendar.png"),Buffer.from(calendarShot.data,"base64"));
  await evaluate(`(async()=>{const {useCalendar}=await import('/src/calendar/store.ts');
    useCalendar.setState({countdown:window.__previousCalendarCountdown})})()`);
  const liveDownloadsSide=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    const {useDownloads}=await import('/src/downloads/Downloads.tsx');
    window.__previousDownloads=useDownloads.getState();
    useDownloads.setState({items:[{id:'portrait-test',name:'design-review.zip',path:'',source:'Browser',
      received:500,total:1000,speed:100,status:'downloading'}]});
    useIsland.setState(s=>({expanded:false,notice:null,activityPickerOpen:false,
      activities:{downloads:{wing:56,capsuleContent:80,opens:'downloads'}},pickedActivity:'downloads',
      settings:{...s.settings,position:{x:0,y:0.5}}}));
    await new Promise(r=>setTimeout(r,350));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const name=document.querySelector('.dl-strip-name').getBoundingClientRect();
    return {width:frame.width,label:document.querySelector('.dl-strip-name').textContent,
      frame:{left:frame.left,right:frame.right},name:{left:name.left,right:name.right}};
  })()`);
  assert(liveDownloadsSide.width<=32&&liveDownloadsSide.label==='ZIP'&&
    liveDownloadsSide.name.left>=liveDownloadsSide.frame.left-1&&
    liveDownloadsSide.name.right<=liveDownloadsSide.frame.right+1,
    `Download information did not fit the slim capsule: ${JSON.stringify(liveDownloadsSide)}`);
  const downloadsShot=await send("Page.captureScreenshot",{format:"png"});
  await writeFile(resolve("../build/island-position-side-downloads.png"),Buffer.from(downloadsShot.data,"base64"));
  await evaluate(`(async()=>{const {useDownloads}=await import('/src/downloads/Downloads.tsx');
    useDownloads.setState(window.__previousDownloads)})()`);
  const captureSide = await evaluate(`(async () => {
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:true,capturePreview:'demo',notice:null,activityPickerOpen:false,
      settings:{...s.settings,position:{x:1,y:0.5}}}));
    await new Promise(r=>setTimeout(r,450));
    const frame=document.querySelector('.island-frame').getBoundingClientRect();
    const layout=document.querySelector('.n-capture-preview .n-expanded-layout')?.getBoundingClientRect();
    return {frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,
      width:frame.width,height:frame.height},
      layout:layout&&{left:layout.left,right:layout.right,top:layout.top,bottom:layout.bottom}};
  })()`);
  assert(captureSide.layout && captureSide.frame.height>captureSide.frame.width &&
    captureSide.layout.left>=captureSide.frame.left-1&&captureSide.layout.right<=captureSide.frame.right+1&&
    captureSide.layout.top>=captureSide.frame.top-1&&captureSide.layout.bottom<=captureSide.frame.bottom+1,
    "Side screenshot preview escaped its portrait panel");
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  const motion = await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:false,capturePreview:null,notice:null,peeking:false,dropHint:false,
      settings:{...s.settings,position:{x:0.95,y:0.95}}}));
    await new Promise(r=>setTimeout(r,300));
    useIsland.setState({expanded:true});
    const frames=[];
    for(let i=0;i<30;i++){
      await new Promise(requestAnimationFrame);
      const b=document.querySelector('.island-frame').getBoundingClientRect();
      frames.push({left:b.left,right:b.right,top:b.top,bottom:b.bottom});
    }
    return frames;
  })()`);
  assert(motion.every(b=>b.left>=-1&&b.right<=1051&&b.top>=-1&&b.bottom<=761),
    `Animated side expansion escaped the display: ${JSON.stringify(motion.filter(b=>b.left<-1||b.right>1051||b.top<-1||b.bottom>761).slice(0,3))}`);
  const bottomMotion=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:false,selected:'controls',notice:null,activities:{},
      pickedActivity:null,peeking:false,dropHint:false,
      settings:{...s.settings,position:{x:0.5,y:1}}}));
    await new Promise(r=>setTimeout(r,550));
    const measure=()=>{const b=document.querySelector('.island-backdrop path[id]').getBoundingClientRect();
      return {top:b.top,bottom:b.bottom}};
    const frames=[measure()];
    useIsland.setState({expanded:true});
    for(let i=0;i<32;i++){await new Promise(requestAnimationFrame);frames.push(measure())}
    return frames;
  })()`);
  assert(bottomMotion.at(-1).top<bottomMotion[0].top-80,
    "Bottom island did not expand upward");
  assert(Math.max(...bottomMotion.map(b=>Math.abs(b.bottom-bottomMotion[0].bottom)))<20,
    "Bottom island lost its lower anchor during expansion");
  const cornerMotion=await evaluate(`(async()=>{
    const {useIsland}=await import('/src/island/store.ts');
    useIsland.setState(s=>({expanded:false,notice:null,activities:{},pickedActivity:null,
      peeking:false,dropHint:false,hoverSuppressed:true,inside:false,
      settings:{...s.settings,opening:'click',idleContent:'none',position:{x:0.001,y:0}}}));
    await new Promise(r=>setTimeout(r,550));
    const measure=()=>{
      const b=document.querySelector('.island-backdrop path[id]').getBoundingClientRect();
      return {left:b.left,right:b.right,top:b.top,bottom:b.bottom,cx:b.left+b.width/2,cy:b.top+b.height/2};
    };
    const samples=[measure()];
    useIsland.setState(s=>({settings:{...s.settings,position:{x:0,y:0.001}}}));
    for(let i=0;i<30;i++){await new Promise(requestAnimationFrame);samples.push(measure())}
    return {samples,width:innerWidth,height:innerHeight};
  })()`);
  assert(cornerMotion.samples.every(b=>b.left>=-1&&b.right<=cornerMotion.width+1&&
    b.top>=-1&&b.bottom<=cornerMotion.height+1), "Corner transition painted outside the display");
  const cornerShift=Math.max(...cornerMotion.samples.slice(1).map((b,i)=>
    Math.hypot(b.cx-cornerMotion.samples[i].cx,b.cy-cornerMotion.samples[i].cy)));
  assert(cornerShift<40,`Corner transition jumped by ${cornerShift.toFixed(1)} px`);
  for (const screen of [{width:640,height:480,scale:1.5},{width:320,height:240,scale:1}]) {
    await send("Emulation.setDeviceMetricsOverride", {
      width:screen.width,height:screen.height,deviceScaleFactor:screen.scale,mobile:false,
    });
    const result=await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState(s=>({expanded:true,selected:'controls',capturePreview:null,notice:null,
        settings:{...s.settings,position:{x:0,y:0.5}}}));
      await new Promise(r=>setTimeout(r,350));
      const frame=document.querySelector('.island-frame').getBoundingClientRect();
      return {left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom,
        width:frame.width,height:frame.height,viewport:{width:innerWidth,height:innerHeight}};
    })()`);
    assert(result.height>result.width&&result.left>=-1&&result.right<=result.viewport.width+1&&
      result.top>=-1&&result.bottom<=result.viewport.height+1,
      `Side panel escaped a scaled or small screen: ${JSON.stringify(result)}`);
    const smallGallery=await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      useIsland.setState({showingSections:true});
      await new Promise(r=>setTimeout(r,350));
      const frame=document.querySelector('.island-frame').getBoundingClientRect();
      const tile=document.querySelector('.n-tile').getBoundingClientRect();
      return {frame:{left:frame.left,right:frame.right,top:frame.top,bottom:frame.bottom},
        tile:{left:tile.left,right:tile.right,top:tile.top,bottom:tile.bottom,width:tile.width,height:tile.height}};
    })()`);
    assert(smallGallery.tile.left>=smallGallery.frame.left-1&&smallGallery.tile.right<=smallGallery.frame.right+1&&
      smallGallery.tile.top>=smallGallery.frame.top-1&&smallGallery.tile.bottom<=smallGallery.frame.bottom+1&&
      smallGallery.tile.width>=80&&smallGallery.tile.height>=80,
      `Small-screen gallery tile was clipped or scaled down: ${JSON.stringify(smallGallery)}`);
    await evaluate(`(async()=>{const {useIsland}=await import('/src/island/store.ts');useIsland.setState({showingSections:false})})()`);
  }
  console.log(`${cases.length} position cases, 10 portrait activities, portrait previews and picker, 17 portrait pages, and settings drag passed.`);
}
