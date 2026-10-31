// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkCamera({ send, evaluate, base }) {
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
    window.prefs = await import('/src/shared/prefs.ts');
    window.cameraCalls = [];
    window.cameraStreams = [];
    window.cameraFailure = null;
    window.cameraDelay = false;
    window.cameraFinishes = [];
    window.devices = [{kind:'videoinput', deviceId:'front',label:'Front camera'}, {kind:'videoinput',deviceId:'usb',label:'USB camera'}, {kind:'audioinput', deviceId:'mic',label:'Microphone'}];
    window.fakeMedia = new EventTarget();
    fakeMedia.enumerateDevices = async () => devices;
    fakeMedia.getUserMedia = async (constraints) => {
      cameraCalls.push(constraints);
      if (cameraFailure) throw { name: cameraFailure };
      const id = constraints.video.deviceId?.exact || 'front';
      const canvas = document.createElement('canvas'); canvas.width = 640; canvas.height = 360;
      const ctx = canvas.getContext('2d');
      const gradient = ctx.createLinearGradient(0,0,640,360); gradient.addColorStop(0,'#273950'); gradient.addColorStop(1,'#70493f');
      ctx.fillStyle = gradient; ctx.fillRect(0,0,640,360);
      ctx.fillStyle = '#ddd';ctx.font = '24px sans-serif';ctx.fillText(id === 'usb' ? 'USB camera preview' : 'Front camera preview',28,175);
      const stream = canvas.captureStream(1);
      stream.getVideoTracks()[0].getSettings = () => ({deviceId:id});
      cameraStreams.push(stream);
      if(cameraDelay) await new Promise(done => { cameraFinishes.push(done); });
      return stream;
    };
    Object.defineProperty(navigator,'mediaDevices',{ configurable:true,value:fakeMedia });
    island.getState().open('tools'); island.getState().toggleSections();
  })()`);
  assert.equal(await evaluate(`cameraCalls.length`), 0, "Camera must stay off outside its page");
  await wait(`!!document.querySelector('.n-tile')`);
  await evaluate(
    `(async () => {const {t}=await import('/src/i18n/index.ts');[...document.querySelectorAll('.n-tile')].find(e=>e.textContent.includes(t('module.camera'))).click()})()`,
  );
  await wait(`!!document.querySelector('.cam-live') && document.querySelector('video').videoWidth === 640`);
  assert(await evaluate(`cameraCalls.every(c=>c.audio === false)`), "No microphone access");
  assert.equal(
    await evaluate(`document.querySelector('video').srcObject.getVideoTracks()[0].getSettings().deviceId`),
    "front",
  );
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('video')).objectFit`), "contain");
  await evaluate(`document.querySelector('[aria-label="Choose camera"]').click()`);
  await wait(`document.querySelectorAll('.u-menu-item').length === 3`);
  await evaluate(
    `[...document.querySelectorAll('.u-menu-item')].find(e=>e.textContent.includes('USB camera')).click()`,
  );
  await wait(
    `document.querySelector('.cam-picker')?.textContent.includes('USB camera') && !!document.querySelector('.cam-live')`,
  );
  assert.equal(
    await evaluate(`cameraStreams[0].getVideoTracks()[0].readyState`),
    "ended",
    "Switch releases the previous camera",
  );
  assert.equal(await evaluate(`(async()=> (await prefs.loadPrefs()).island.cameraDeviceId)()`), "usb");
  await evaluate(`document.querySelector('[aria-label="Mirror preview"]').click()`);
  assert.equal(await evaluate(`document.querySelector('video').classList.contains('is-mirrored')`), false);

  let cases = 0;
  for (const silhouette of ["notch", "capsule"]) {
    for (const layout of ["compact", "spacious", "custom"]) {
      await evaluate(
        `island.setState(s=>({settings:{...s.settings,silhouette:'${silhouette}',layout:'${layout}',customWidth:380,customHeight:250}}))`,
      );
      await evaluate(`new Promise(done=>setTimeout(done,250))`);
      assert(
        await evaluate(`(()=>{
        const page=document.querySelector('.cam-page').getBoundingClientRect(),frame=document.querySelector('.island-frame').getBoundingClientRect(),bar=document.querySelector('.cam-bar').getBoundingClientRect(),view=document.querySelector('.cam-view').getBoundingClientRect();
        return page.left>=frame.left && page.right<=frame.right+1 && page.bottom<=frame.bottom+1 && bar.top>=view.bottom && view.height>80;
      })()`),
        `${silhouette}/${layout}: preview and picker must fit`,
      );
      cases++;
    }
  }
  await evaluate(`island.setState(s=>({settings:{...s.settings,layout:'spacious',silhouette:'notch'}}))`);
  await evaluate(`new Promise(done=>setTimeout(done,250))`);
  await mkdir(resolve("../build"), { recursive: true });
  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(resolve("../build/camera-preview.png"), Buffer.from(screenshot.data, "base64"));

  await evaluate(`document.querySelector('[aria-label="Choose camera"]').click()`);
  await wait(`!!document.querySelector('.u-menu')`);
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape" });
  await wait(`!document.querySelector('.u-menu')`);
  assert(await evaluate(`island.getState().expanded`), "First Escape closes only the picker");

  await evaluate(`island.getState().collapse()`);
  await wait(`cameraStreams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))`);
  await evaluate(`island.getState().open('camera')`);
  await wait(`!!document.querySelector('.cam-live')`);
  assert.equal(await evaluate(`cameraCalls.at(-1).video.deviceId.exact`), "usb", "Reopening remembers the camera");
  await evaluate(`cameraFailure='NotAllowedError'; document.querySelector('[aria-label="Choose camera"]').click()`);
  await wait(`!!document.querySelector('.u-menu')`);
  await evaluate(
    `[...document.querySelectorAll('.u-menu-item')].find(e=>e.textContent.includes('Front camera')).click()`,
  );
  await wait(`document.querySelector('.cam-status')?.textContent.includes('denied')`);
  assert(await evaluate(`cameraStreams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))`));
  await evaluate(`cameraFailure=null; document.querySelector('.cam-status button').click()`);
  await wait(`!!document.querySelector('.cam-live')`);

  await evaluate(`island.getState().open('tools')`);
  await wait(`cameraStreams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))`);
  await evaluate(`cameraDelay=true; island.getState().open('camera')`);
  await wait(`cameraFinishes.length > 0`);
  await evaluate(`island.getState().collapse(); cameraFinishes.forEach(done => done())`);
  await wait(`cameraStreams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))`);
  console.log(
    `Camera passed: registration, live video, selection, preferences, mirror, ${cases} layouts, menu Escape, errors/retry, and stream cleanup including late permission replies.`,
  );
}
