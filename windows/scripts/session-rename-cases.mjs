// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkSessionRename({ send, evaluate, base }) {
  await send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-reduced-motion", value: "no-preference" }],
  });
  await send("Page.navigate", { url: `${base}island.html?sessions` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.s-strip, .n-activation')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    const demo=(await import('/src/sessions/demo.ts')).demoSnapshot();
    sessions.setState({snapshot:demo,focused:demo.sessions[0].id,renaming:false,names:{}});
    island.setState(s=>({expanded:true,pinned:true,selected:'sessions',notice:null,showingSections:false,
      settings:{...s.settings,layout:'custom',customWidth:600,customHeight:420}}));
    await document.fonts.ready;
  })()`);
  await new Promise((r) => setTimeout(r, 600));
  const opening = await evaluate(`(async()=>{
    sessions.setState({renaming:true});
    const heights=[];
    const start=performance.now();
    while(performance.now()-start<340){
      await new Promise(requestAnimationFrame);
      heights.push(document.querySelector('.s-rename-slot')?.getBoundingClientRect().height??0);
    }
    const field=document.querySelector('.s-rename');
    const rect=e=>{const r=e.getBoundingClientRect();return {top:r.top,bottom:r.bottom,left:r.left,right:r.right}};
    return {heights,field:rect(field),editor:rect(field.closest('.s-rename-editor')),
      page:rect(document.querySelector('.n-page')),content:rect(document.querySelector('.s-scroll')),
      focused:document.activeElement===field};
  })()`);
  const finalHeight = opening.heights.at(-1);
  assert(finalHeight > 30, "The rename editor must take its own space");
  assert(
    opening.heights.some((h) => h > 1 && h < finalHeight - 1),
    "Opening must have intermediate frames",
  );
  assert(opening.focused, "Opening must focus the input");
  assert(opening.editor.top >= opening.page.top - 1, "The editor must stay inside the page");
  assert(opening.editor.bottom <= opening.content.top, "The editor must not overlap session content");
  assert(
    opening.field.left >= opening.editor.left && opening.field.right <= opening.editor.right,
    "The input must fit inside the editor",
  );
  await send("Input.insertText", { text: "Dodaj synchronizację z main" });
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await mkdir(resolve("../build"), { recursive: true });
  await writeFile(resolve("../build/session-rename.png"), Buffer.from(screenshot.data, "base64"));
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape" });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape" });
  const closing = await evaluate(`(async()=>{
    const heights=[];
    const start=performance.now();
    while(performance.now()-start<340){
      await new Promise(requestAnimationFrame);
      heights.push(document.querySelector('.s-rename-slot')?.getBoundingClientRect().height??0);
    }
    return {heights,renaming:sessions.getState().renaming,names:sessions.getState().names,
      expanded:island.getState().expanded,field:!!document.querySelector('.s-rename')};
  })()`);
  assert(
    closing.heights.some((h) => h > 1 && h < finalHeight - 1),
    "Closing must have intermediate frames",
  );
  assert(!closing.field && !closing.renaming && closing.expanded, "Escape closes only the editor");
  assert.deepEqual(closing.names, {}, "Escape must not save on blur during exit");
  await evaluate(`sessions.setState({renaming:true})`);
  await new Promise((r) => setTimeout(r, 300));
  await send("Input.insertText", { text: "Saved session" });
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter" });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter" });
  await new Promise((r) => setTimeout(r, 300));
  assert.equal(await evaluate(`sessions.getState().names[sessions.getState().focused]`), "Saved session");
  await send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-reduced-motion", value: "reduce" }],
  });
  // Motion reads this preference when the component mounts.
  await evaluate(`sessions.setState({focused:null})`);
  await new Promise((r) => setTimeout(r, 60));
  await evaluate(`sessions.setState(s=>({focused:s.snapshot.sessions[0].id}))`);
  await new Promise((r) => setTimeout(r, 60));
  await evaluate(`sessions.setState({renaming:true})`);
  await new Promise((r) => setTimeout(r, 60));
  assert(
    await evaluate(`document.querySelector('.s-rename-slot').getBoundingClientRect().height>30`),
    "Reduced motion must reveal the field immediately",
  );
  console.log(
    "Session rename: layout, opening/closing frames, Escape, Enter and reduced motion passed. Screenshot: build/session-rename.png",
  );
}
