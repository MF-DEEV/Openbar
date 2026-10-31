// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

// Hovering a mirrored banner opens its preview in place, and Clear All asks
// first (NotchService.holdNotification, confirmClearAboveIsland).
export async function checkNotificationPreview({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('.n-activation')`)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const mouse = (x, y) => send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  const click = async ({ x, y }) => {
    await mouse(x, y);
    for (const type of ["mousePressed", "mouseReleased"])
      await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
  };
  const center = (selector) =>
    evaluate(`(()=>{const b=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();
      return {x:b.left+b.width/2,y:b.top+b.height/2}})()`);
  const shoot = async (name) => {
    const frame = await evaluate(`(()=>{const b=document.querySelector('.island-frame').getBoundingClientRect();
      return {left:b.left,width:b.width,bottom:b.bottom}})()`);
    const shot = await send("Page.captureScreenshot", {
      format: "png",
      clip: { x: frame.left - 20, y: 0, width: frame.width + 40, height: frame.bottom + 20, scale: 2 },
    });
    await mkdir(resolve("../build"), { recursive: true });
    await writeFile(resolve(`../build/${name}.png`), Buffer.from(shot.data, "base64"));
  };
  const item = (id, title) => ({
    id,
    app: "Discord",
    aumid: "com.squirrel.Discord.Discord",
    time: 1793450000 + id,
    title,
    body: "SECRET BODY",
  });
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.inbox=(await import('/src/notifications/store.ts')).useNotifications;
    window.present=(await import('/src/notifications/presentation.ts')).notificationNotice;
    window.calls={open:[],dismiss:[],clear:0};
    const {bridge}=await import('/src/shared/bridge.ts');
    bridge.notifications.open=async a=>{calls.open.push(a);return true};
    bridge.notifications.dismiss=async id=>{calls.dismiss.push(id);
      return {access:'allowed',items:inbox.getState().items.filter(i=>i.id!==id)}};
    bridge.notifications.clear=async()=>{calls.clear++;return {access:'allowed',items:[]}};
    await document.fonts.ready;
    island.setState(s=>({settings:{...s.settings,opening:'hoverPreview',hoverDelay:0.1,closeDelay:0.1}}));
  })()`);
  const items = [item(1, "Alicja (#general, Friends)"), item(2, "Bartek")];
  const show = (i) =>
    evaluate(`(()=>{const it=${JSON.stringify(i)};
      inbox.getState().receive({access:'allowed',items:${JSON.stringify(items)}});
      island.getState().showNotice(present(it))})()`);
  const state = () =>
    evaluate(
      `(()=>{const s=island.getState();return {notice:s.notice?.id??null,expanded:s.noticeExpanded,open:s.expanded}})()`,
    );
  const away = async () => {
    await mouse(5, 300);
    await wait(500);
  };
  const hoverOpen = async (i) => {
    await show(i);
    await wait(50);
    const strip = await center(".n-notice");
    await mouse(strip.x, strip.y);
    await wait(800);
  };

  // Hover opens the preview after the delay, showing what is collected and no body.
  await mouse(5, 300);
  await show(items[0]);
  await wait(50);
  const strip = await center(".n-notice");
  await mouse(strip.x, strip.y);
  await wait(30);
  assert.deepEqual(await state(), { notice: "notification:1", expanded: false, open: false }, "Not before the delay");
  await wait(800);
  assert.deepEqual(await state(), { notice: "notification:1", expanded: true, open: false });
  const shown = await evaluate(`(()=>{const p=document.querySelector('.nt-preview');
    const t=document.querySelector('.nt-preview-title');const f=document.querySelector('.island-frame').getBoundingClientRect();
    const last=document.querySelector('.nt-preview-actions').getBoundingClientRect();
    return {text:p.innerText,html:p.outerHTML,fits:last.bottom<=f.bottom,overflow:t.scrollHeight>t.clientHeight+1,width:f.width}})()`);
  assert.match(shown.text, /Discord/);
  assert.match(shown.text, /Alicja/);
  assert.doesNotMatch(shown.html, /SECRET BODY/, "Message bodies must not reach the preview");
  assert(shown.fits && !shown.overflow, `The preview must fit its content: ${JSON.stringify(shown)}`);
  await shoot("notification-preview");

  // A long sender wraps to two lines and the island grows to fit, in both silhouettes.
  const long = item(1, "Bardzo długa nazwa nadawcy, która nie mieści się w jednej linii podglądu (#general, Friends)");
  for (const silhouette of ["notch", "capsule"]) {
    await evaluate(`island.setState(s=>({settings:{...s.settings,silhouette:${JSON.stringify(silhouette)}}}))`);
    await show(long);
    await wait(700);
    const fit = await evaluate(`(()=>{const t=document.querySelector('.nt-preview-title');
      const a=document.querySelector('.nt-preview-actions').getBoundingClientRect();
      const f=document.querySelector('.island-frame').getBoundingClientRect();
      return {lines:Math.round(t.getBoundingClientRect().height/18),below:a.bottom<=f.bottom,inside:a.left>=f.left&&a.right<=f.right}})()`);
    assert.deepEqual(fit, { lines: 2, below: true, inside: true }, silhouette);
    await shoot(`notification-preview-${silhouette}`);
  }
  await evaluate(`island.setState(s=>({settings:{...s.settings,silhouette:'notch'}}))`);

  // A new message under the pointer stays open in place.
  await show(items[1]);
  await wait(500);
  assert.deepEqual(await state(), { notice: "notification:2", expanded: true, open: false });
  assert.match(await evaluate(`document.querySelector('.nt-preview-title').innerText`), /Bartek/);

  // Open launches the source app and leaves the island closed.
  await click(await center(".nt-preview-actions .is-prominent"));
  await wait(50);
  assert.deepEqual(await state(), { notice: null, expanded: false, open: false });
  assert.deepEqual(await evaluate(`calls.open`), ["com.squirrel.Discord.Discord"]);

  // Leaving closes the preview but keeps the inbox.
  await away();
  await hoverOpen(items[0]);
  assert.equal((await state()).expanded, true);
  await away();
  assert.deepEqual(await state(), { notice: null, expanded: false, open: false });
  assert.equal(await evaluate(`inbox.getState().items.length`), 2);

  // Dismiss removes that message.
  await hoverOpen(items[0]);
  await click(await center(".nt-preview .nt-mini"));
  await wait(100);
  assert.deepEqual(await evaluate(`calls.dismiss`), [1]);
  assert.equal((await state()).notice, null);

  // Clear All asks; Cancel and Escape keep the inbox, confirming empties it.
  await away();
  await evaluate(`(()=>{inbox.getState().receive({access:'allowed',items:${JSON.stringify(items)}});
    island.getState().open('notifications')})()`);
  await wait(700);
  const clearAll = await center(".n-row-trailing .n-pill-button");
  await click(clearAll);
  await wait(250);
  assert.equal(await evaluate(`calls.clear`), 0, "Clear All must ask first");
  assert.equal(await evaluate(`island.getState().held`), true, "The question keeps the island open");
  assert.match(await evaluate(`document.querySelector('.tl-sheet').innerText`), /Clear all notifications\?/);
  await shoot("notification-clear-all");
  await click(await center(".tl-sheet-button:not(.is-default)"));
  await wait(100);
  assert.equal(await evaluate(`!!document.querySelector('.tl-sheet')`), false);
  assert.equal(await evaluate(`calls.clear`), 0);
  assert.equal(await evaluate(`island.getState().held`), false);
  await click(clearAll);
  await wait(100);
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await wait(100);
  assert.equal(await evaluate(`!!document.querySelector('.tl-sheet')`), false, "Escape cancels the question");
  assert.equal(await evaluate(`island.getState().expanded`), true, "Escape closes only the question");
  await click(clearAll);
  await wait(100);
  await click(await center(".tl-sheet-button.is-default"));
  await wait(100);
  assert.equal(await evaluate(`calls.clear`), 1);
  assert.equal(await evaluate(`inbox.getState().items.length`), 0);
  assert.equal(await evaluate(`!!document.querySelector('.tl-sheet')`), false);

  // The inbox emptied under the question (locking the PC) takes the question with it.
  await evaluate(`inbox.getState().receive({access:'allowed',items:${JSON.stringify(items)}})`);
  await wait(100);
  await click(await center(".n-row-trailing .n-pill-button"));
  await wait(100);
  assert.equal(await evaluate(`!!document.querySelector('.tl-sheet')`), true);
  await evaluate(`inbox.getState().receive({access:'allowed',items:[]})`);
  await wait(100);
  assert.deepEqual(
    await evaluate(
      `({sheet:!!document.querySelector('.tl-sheet'),asking:inbox.getState().confirmingClear,held:island.getState().held})`,
    ),
    { sheet: false, asking: false, held: false },
  );
  console.log(
    "Notification preview passed. Screenshots: build/notification-preview.png, build/notification-clear-all.png",
  );
}
