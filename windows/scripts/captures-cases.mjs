// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";

export async function checkCaptures({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}island.html` });
  const wait = async (expression) => {
    for (let i = 0; i < 100; i++) {
      if (await evaluate(expression)) return;
      await new Promise((r) => setTimeout(r, 50));
    }
    assert.fail(`Timed out: ${expression}`);
  };
  await wait(`!!document.querySelector('.n-activation')`);
  await evaluate(`(async () => {
    window.island = (await import('/src/island/store.ts')).useIsland;
    window.captures = (await import('/src/captures/store.ts')).useCaptures;
    const { bridge } = await import('/src/shared/bridge.ts');
    window.captureBridge = bridge.capture;
    const canvas = document.createElement('canvas'); canvas.width = 200; canvas.height = 100;
    const ctx = canvas.getContext('2d'); ctx.fillStyle = '#4070a0'; ctx.fillRect(0, 0, 200, 100);
    window.historyEntries = Array.from({length: 8}, (_, i) => ({id: 'capture-' + i, createdAt: Date.now() - i * 1000, thumbnail: canvas.toDataURL()}));
    bridge.capture.list = async () => [...window.historyEntries];
    bridge.capture.onChanged = async (fn) => { window.captureChanged = fn; return () => {}; };
    bridge.capture.open = async (id) => { window.openedCapture = id; };
    bridge.capture.remove = async (id) => { window.historyEntries = window.historyEntries.filter(e => e.id !== id); window.captureChanged(); };
    bridge.capture.clear = async () => { window.historyEntries = []; window.captureChanged(); };
    island.getState().open('tools');
    island.getState().toggleSections();
    await document.fonts.ready;
  })()`);
  await wait(`!!document.querySelector('.n-tile')`);
  await evaluate(
    `(async () => {
      const { t } = await import('/src/i18n/index.ts');
      Array.from(document.querySelectorAll('.n-tile')).find(e => e.querySelector('.n-tile-title').textContent === t('module.captures')).click();
    })()`,
  );
  await wait(`document.querySelectorAll('.cp-card').length === 8`);
  assert.equal(await evaluate(`document.querySelector('.cp-preview img').naturalWidth`), 200);
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('.cp-preview img')).objectFit`), "contain");
  for (const layout of ["compact", "spacious"]) {
    await evaluate(`island.setState(s => ({settings: {...s.settings, layout: '${layout}'}}))`);
    await evaluate(`(async()=>{for(let i=0;i<12;i++)await new Promise(requestAnimationFrame)})()`);
    const fits = await evaluate(`(() => {
      const page = document.querySelector('.cp-page').getBoundingClientRect();
      const frame = document.querySelector('.island-frame').getBoundingClientRect();
      const rail = document.querySelector('.cp-rail');
      return page.left >= frame.left && page.right <= frame.right + 1 && page.bottom <= frame.bottom + 1 && rail.scrollWidth > rail.clientWidth;
    })()`);
    assert(fits, `Captures must fit inside the ${layout} island and scroll horizontally`);
  }
  await evaluate(`document.querySelector('.cp-actions button').click()`);
  await wait(`window.openedCapture === 'capture-0' && !captures.getState().busy`);
  await evaluate(`document.querySelector('[aria-label="Delete capture"]').click()`);
  await wait(`document.querySelectorAll('.cp-card').length === 7`);
  await evaluate(
    `window.historyEntries.unshift({id:'new',createdAt:Date.now(),thumbnail:null}); window.captureChanged()`,
  );
  await wait(`document.querySelectorAll('.cp-card').length === 8`);
  await evaluate(`document.querySelector('[aria-label="Clear all recent captures"]').click()`);
  await wait(`!!document.querySelector('[role="alertdialog"]')`);
  await evaluate(`document.querySelector('.cp-confirm .n-pill-button').click()`);
  assert.equal(await evaluate(`window.historyEntries.length`), 8, "Cancel must preserve history");
  await evaluate(`document.querySelector('[aria-label="Clear all recent captures"]').click()`);
  await wait(`!!document.querySelector('.cp-confirm')`);
  await evaluate(`document.querySelector('.cp-confirm .is-prominent').click()`);
  await wait(`document.querySelector('.n-empty-title')?.textContent === 'No recent captures'`);
  assert(await evaluate(`document.querySelector('[aria-label="Clear all recent captures"]').disabled`));
  await evaluate(
    `window.captureBridge.list = async () => { throw Error('unavailable') }; captures.getState().refresh()`,
  );
  await wait(`!!document.querySelector('.cp-error')`);
  await evaluate(`window.captureBridge.list = async () => []; document.querySelector('.cp-error button').click()`);
  await wait(`!document.querySelector('.cp-error')`);
  await evaluate(`island.getState().open('tools')`);
  await wait(`!!document.querySelector('.tl-page') || !document.querySelector('.cp-page')`);
  assert.equal(await evaluate(`island.getState().selected`), "tools");
  console.log(
    "Captures passed: registration, layouts, previews, open, delete, live additions, clear/cancel, empty state, errors and navigation.",
  );
}
