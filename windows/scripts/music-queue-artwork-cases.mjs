// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkMusicQueueArtwork({ send, evaluate, base }) {
  // The plain browser has no Tauri HTTPS bridge. Supply actual public catalog
  // responses through its bridge fixture, including the album verification step.
  const songs = [
    { title: "Out of My League", artist: "Fitz and The Tantrums" },
    { title: "The Blonde", artist: "TV Girl" },
    { title: "Let's Get It On", artist: "Marvin Gaye" },
    { title: "Out of Touch", artist: "Daryl Hall & John Oates" },
    { title: "Swing Lynn", artist: "Harmless" },
    { title: "Take On Me", artist: "a-ha" },
  ];
  const reportedSong = { title: "Swimming Pools (Drank)", artist: "Kendrick Lamar" };
  const catalog = await Promise.all(
    [...songs, reportedSong].map(async (song) => {
      const response = await fetch(
        "https://api.deezer.com/search?" + new URLSearchParams({ q: song.title + " " + song.artist, limit: "12" }),
      );
      assert(response.ok, "Public catalog request failed");
      const data = await response.json();
      const normalized = (value) =>
        value
          .toLowerCase()
          .replace(/&/g, "and")
          .replace(/[^a-z0-9]+/g, "");
      const row = data.data?.find(
        (row) =>
          normalized(row.title) === normalized(song.title) && normalized(row.artist.name) === normalized(song.artist),
      );
      assert(row, "Missing live catalog fixture: " + song.title);
      const album = await (await fetch("https://api.deezer.com/album/" + row.album.id)).json();
      assert(["single", "album", "ep"].includes(album.record_type), "Fixture must be an artist release");
      return { song, search: { data: [row] }, album };
    }),
  );
  const wait = async (expression, count = 200) => {
    for (let i = 0; i < count; i++) {
      if (await evaluate(expression)) return;
      await new Promise((r) => setTimeout(r, 100));
    }
    assert.fail("Timed out: " + expression);
  };
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await send("Page.navigate", { url: `${base}island.html?music` });
  await wait("!!document.querySelector('.m-strip')");
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.music=(await import('/src/music/store.ts')).useMusic;
    window.bridge=(await import('/src/shared/bridge.ts')).bridge;
    window.artwork=(await import('/src/music/queueArtwork.ts')).useQueueArtwork;
    window.songs=${JSON.stringify(songs)};
    window.catalog=${JSON.stringify(catalog)};
    bridge.music.queueArtwork=async(title,artist)=>catalog.find(entry=>entry.song.title===title&&entry.song.artist===artist)?.search;
    bridge.music.queueAlbum=async(id)=>catalog.find(entry=>entry.album.id===id)?.album;
    window.originalPlayback={...music.getState().playback,title:'Now'};
    music.setState({playback:originalPlayback});
    bridge.music.queue=async(source,title)=>{
      if(title!=='Now')await new Promise(r=>setTimeout(r,500));
      return {source,title,status:'ready',items:title==='Now'?songs.slice(0,5):songs.slice(1,6),prefetch:title==='Now'?songs.slice(5):[]};
    };
    island.getState().open('music');
  })()`);
  await wait("!!document.querySelector('[aria-controls=music-queue]')");
  await evaluate("document.querySelector('[aria-controls=music-queue]').click()");
  await wait("document.querySelectorAll('.m-queue-art img.is-loaded').length===5");
  await wait("Object.keys(artwork.getState().covers).length===6");
  await evaluate(`window.refreshButton=document.querySelector('.m-queue-heading button');window.refreshIcon=refreshButton.firstElementChild;window.refreshMutations=0;
    window.refreshObserver=new MutationObserver(()=>refreshMutations++);refreshObserver.observe(refreshButton,{attributes:true,childList:true,subtree:true});`);
  await new Promise((resolve) => setTimeout(resolve, 2900));
  assert.equal(
    await evaluate("refreshMutations"),
    0,
    "Background polling must not mutate or animate the refresh button",
  );
  assert.equal(
    await evaluate(
      "refreshButton.firstElementChild===refreshIcon && !refreshButton.disabled && refreshButton.querySelectorAll('.m-queue-spinner').length===0",
    ),
    true,
  );
  await evaluate("refreshObserver.disconnect()");
  assert.equal(
    await evaluate("document.querySelector('.m-queue-artwork-toggle, .m-queue-attribution') === null"),
    true,
  );
  assert.equal(await evaluate("document.querySelector('.m-queue').textContent.includes('Apple Music')"), false);
  assert.equal(await evaluate("document.querySelector('.m-queue-art a') === null"), true);
  const metrics = await evaluate(
    `(()=>{const row=document.querySelector('.m-queue-row');return {height:row.getBoundingClientRect().height,font:parseFloat(getComputedStyle(row.querySelector('strong')).fontSize),cover:parseFloat(getComputedStyle(row.querySelector('.m-queue-art')).width)};})()`,
  );
  assert(metrics.cover >= 40 && metrics.font >= 13.5);
  await evaluate(`window.keptRow=document.querySelectorAll('.m-queue-row')[1];window.keptImage=keptRow.querySelector('img');window.keptPanel=document.querySelector('.m-queue');window.resets=0;
    window.queueObserver=new MutationObserver(()=>{if(document.querySelector('.m-queue-empty') || !document.querySelector('.m-queue-list'))resets++;});
    queueObserver.observe(keptPanel,{childList:true,subtree:true});
    window.motionSamples=[];
    const sample=()=>{
      const list=document.querySelector('.m-queue-list');
      motionSamples.push({overflow:getComputedStyle(list).overflowY,height:list.clientHeight,scroll:list.scrollHeight,time:performance.now()});
      if(motionSamples.length<70)requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
    music.setState({playback:{...originalPlayback,title:songs[0].title,artist:songs[0].artist}});`);
  const immediate =
    await evaluate(`(async()=>{await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);
    const rows=Array.from(document.querySelectorAll('.m-queue-row'));const row=rows.find(r=>r.querySelector('strong').textContent==='Take On Me');
    const image=row?.querySelector('img');return {present:!!row,decoded:!!image?.complete && image.naturalWidth>0,opacity:image?getComputedStyle(image).opacity:null};})()`);
  assert.equal(immediate.present, true, "The fifth row must appear from lookahead before the native read returns");
  assert.equal(immediate.decoded, true, "The new row's cover must already be downloaded and decoded");
  assert.equal(immediate.opacity, "1", "A prefetched cover must not start a second fade/loading cycle");
  await wait(
    "document.querySelectorAll('.m-queue-row').length===5 && document.querySelector('.m-queue-row strong')?.textContent==='The Blonde'",
  );
  await wait("document.querySelectorAll('.m-queue-art img.is-loaded').length===5");
  await new Promise((r) => setTimeout(r, 500));
  assert.equal(await evaluate("document.querySelector('.m-queue')===keptPanel"), true, "Panel must stay mounted");
  assert.equal(
    await evaluate("document.querySelector('.m-queue-row')===keptRow"),
    true,
    "Retained song must keep its DOM node",
  );
  assert.equal(
    await evaluate("document.querySelector('.m-queue-row img')===keptImage"),
    true,
    "Retained cover must not reload",
  );
  assert.equal(await evaluate("resets"), 0, "No empty/loading panel during skip");
  assert.equal(
    await evaluate("motionSamples.every(s=>s.overflow==='clip' && s.height===240)"),
    true,
    "No scrollbar or list resizing during a skip",
  );
  await evaluate("queueObserver.disconnect()");
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../music-queue-artwork.png"), Buffer.from(screenshot.data, "base64"));
  await evaluate(`window.reportedRows=songs.slice(1,6);reportedRows[3]=${JSON.stringify(reportedSong)};
    bridge.music.queue=async(source,title)=>({source,title,status:'ready',items:reportedRows,prefetch:[]});
    document.querySelector('.m-queue-heading button').click();`);
  await wait(
    `Array.from(document.querySelectorAll('.m-queue-row')).some(row=>row.querySelector('strong')?.textContent==='Swimming Pools (Drank)' && row.querySelector('img')?.src.includes('4620d1f18a1e403b560efbf38a837c83'))`,
  );
  const clip = await evaluate(
    `(()=>{const r=document.querySelector('.n-expanded').getBoundingClientRect();return {x:Math.max(0,r.left-75),y:Math.max(0,r.top),width:r.width+150,height:r.height+75,scale:1}})()`,
  );
  const editionScreenshot = await send("Page.captureScreenshot", { format: "png", clip });
  await writeFile(resolve("../music-queue-cover-edition.png"), Buffer.from(editionScreenshot.data, "base64"));
  console.log(
    "Reported-song regression passed: original single cover, not Grammy compilation. Screenshot: ../music-queue-cover-edition.png",
  );
  console.log(
    "Animation samples:",
    await evaluate(
      `JSON.stringify({frames:motionSamples.length,maxFrameMs:Math.round(Math.max(...motionSamples.slice(1).map((s,i)=>s.time-motionSamples[i].time)))})`,
    ),
  );
  console.log(
    "Queue artwork passed: 5 real covers, larger rows, smooth skip with retained panel/row/image DOM and no loading reset. Screenshot: ../music-queue-artwork.png",
  );
}
