// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";

export async function checkMusicStability({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html?music` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate("!!document.querySelector('.island-frame')")) break;
    await new Promise((r) => setTimeout(r, 50));
  }
  assert(await evaluate("!!document.querySelector('.island-frame')"));
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });

  for (const position of [
    { x: 0, y: 0.5 },
    { x: 1, y: 0.5 },
    { x: 0.5, y: 0 },
  ]) {
    await evaluate(`(async()=>{
      const {useIsland}=await import('/src/island/store.ts');
      const {useMusic}=await import('/src/music/store.ts');
      const {demoNowPlaying}=await import('/src/music/demo.ts');
      useMusic.getState().receive(demoNowPlaying());
      useMusic.setState(s=>({playback:{...s.playback,title:'Jukebox Joints (feat. Joe Fox & Kanye West)'}}));
      useIsland.setState(s=>({expanded:true,selected:'music',showingSections:false,
        settings:{...s.settings,position:{x:${position.x},y:${position.y}}}}));
      await document.fonts.ready;
      await new Promise(r=>setTimeout(r,700));
      window.__musicSamples=[];
      let elapsed=74;
      window.__musicUpdates=setInterval(()=>{
        elapsed++;
        useMusic.setState(s=>({playback:{...s.playback,elapsed,sampledAt:Date.now()/1000}}));
      },200);
      let height=2;
      window.__musicLevels=setInterval(()=>{
        height=height===2?15:2;
        document.querySelectorAll('.m-page .m-bars > span').forEach(el=>el.style.height=height+'px');
      },60);
      window.__musicSampler=setInterval(()=>{
        const frame=document.querySelector('.island-frame')?.getBoundingClientRect();
        const page=document.querySelector('.m-page');
        const bubble=document.querySelector('.n-quick')?.getBoundingClientRect();
        if(frame&&page)window.__musicSamples.push({
          frame:[frame.x,frame.y,frame.width,frame.height],page:page.offsetHeight,
          content:useIsland.getState().pageContent.music,
          bubble:bubble&&[bubble.x,bubble.y,bubble.width,bubble.height]});
      },30);
    })()`);
    await new Promise((r) => setTimeout(r, 2000));
    const result = await evaluate(`(()=>{
      clearInterval(window.__musicSampler);
      clearInterval(window.__musicUpdates);
      clearInterval(window.__musicLevels);
      const values=window.__musicSamples;
      return {count:values.length,frames:[...new Set(values.map(v=>v.frame.join(',')))],
        heights:[...new Set(values.map(v=>v.page))],contents:[...new Set(values.map(v=>v.content))],
        bubbles:[...new Set(values.map(v=>v.bubble?.join(',')))]};
    })()`);
    assert(
      result.count > 40 &&
        result.frames.length === 1 &&
        result.heights.length === 1 &&
        result.contents.length === 1 &&
        result.bubbles.length === 1,
      `Music moved at ${JSON.stringify(position)}: ${JSON.stringify(result)}`,
    );
  }
  console.log(
    "Music panel, content and external buttons stayed still at left, right and top during playback and visualizer updates.",
  );
}
