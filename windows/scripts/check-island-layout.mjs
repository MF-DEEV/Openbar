// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
// Run on Windows with Microsoft Edge installed: node scripts/check-island-layout.mjs
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, realpathSync } from "node:fs";
import { mkdtemp, readFile, rm, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join, dirname, basename } from "node:path";
import { createServer } from "vite";
import { checkSettingsSelect } from "./settings-select-cases.mjs";
import { checkQuickAccessEditor } from "./quick-access-cases.mjs";
import { checkActivityPicker } from "./activity-picker-cases.mjs";
import { checkAutoAllow } from "./auto-allow-cases.mjs";
import { checkMusicLyrics } from "./music-lyrics-cases.mjs";
import { checkSessionNotice } from "./session-notice-cases.mjs";
import { checkNotificationPreview } from "./notification-preview-cases.mjs";
import { checkAllDisplays } from "./all-displays-cases.mjs";
import { checkDownloads } from "./downloads-cases.mjs";
import { checkIdleContent } from "./idle-content-cases.mjs";
import { checkGestures } from "./gestures-cases.mjs";
import { checkIslandShortcut } from "./island-shortcut-cases.mjs";
import { checkScreenshotPreview } from "./screenshot-preview-cases.mjs";
import { checkCaptures } from "./captures-cases.mjs";
import { checkMusicSources } from "./music-source-cases.mjs";
import { checkCamera } from "./camera-cases.mjs";
import { checkMusicEqualizer } from "./music-equalizer-cases.mjs";
import { checkMusicOpenPlayer } from "./music-open-player-cases.mjs";
import { checkIslandContextMenu } from "./island-context-menu-cases.mjs";
import { checkMascot } from "./mascot-cases.mjs";
import { checkSessionRename } from "./session-rename-cases.mjs";

const browserPath = [
  process.env.ISLAND_TEST_BROWSER,
  `${process.env["ProgramFiles(x86)"]}/Microsoft/Edge/Application/msedge.exe`,
  `${process.env.ProgramFiles}/Microsoft/Edge/Application/msedge.exe`,
].find((path) => path && existsSync(path));
assert(browserPath, "Microsoft Edge not found; set ISLAND_TEST_BROWSER to a Chromium executable.");
const profile = await mkdtemp(join(tmpdir(), "vorssaint-layout-"));
const server = await createServer({
  server: {
    host: "127.0.0.1",
    port: 0,
    strictPort: false,
    fs: { allow: [resolve("."), realpathSync("node_modules")] },
  },
});
let browser, socket;
try {
  await server.listen();
  browser = spawn(
    browserPath,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "about:blank",
    ],
    { windowsHide: true, stdio: "ignore" },
  );
  let port;
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      port = (await readFile(join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0];
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  assert(port, "Browser did not start its debugging endpoint.");
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  socket = new WebSocket(targets.find((target) => target.type === "page").webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = reject;
  });
  let sequence = 0;
  const pending = new Map();
  socket.onmessage = (event) => {
    const response = JSON.parse(event.data);
    const waiting = pending.get(response.id);
    if (waiting) {
      pending.delete(response.id);
      response.error ? waiting.reject(response.error) : waiting.resolve(response.result);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++sequence;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  await send("Emulation.setDeviceMetricsOverride", {
    width: 1440,
    height: 1000,
    deviceScaleFactor: 1.25,
    mobile: false,
  });
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  if (process.argv.includes("--session-rename")) {
    await checkSessionRename({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--island-shortcut")) {
    await checkIslandShortcut({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--context-menu")) {
    await checkIslandContextMenu({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--music-sources")) {
    await checkMusicSources({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--music-equalizer")) {
    await checkMusicEqualizer({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--mascot")) {
    await checkMascot({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--camera")) {
    await checkCamera({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--music-open-player")) {
    await checkMusicOpenPlayer({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--music-lyrics")) {
    await checkMusicLyrics({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--gestures")) {
    await checkGestures({ send, evaluate, base: server.resolvedUrls.local[0] });
    console.log("Gestures: open, close, skip, settings and blocked surfaces passed.");
  } else if (process.argv.includes("--idle-content")) {
    await checkIdleContent({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--captures")) {
    await checkCaptures({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--screenshot-preview")) {
    await checkScreenshotPreview({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--session-notice")) {
    await checkSessionNotice({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--downloads")) {
    await checkDownloads({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--all-displays")) {
    await checkAllDisplays({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--notification-preview")) {
    await checkNotificationPreview({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--auto-allow")) {
    await checkAutoAllow({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--activity-picker")) {
    await checkActivityPicker({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--quick-access")) {
    await checkQuickAccessEditor({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else if (process.argv.includes("--settings")) {
    await checkSettingsSelect({ send, evaluate, base: server.resolvedUrls.local[0] });
  } else {
    await send("Page.navigate", { url: `${server.resolvedUrls.local[0]}island.html?music&sessions` });
    for (let attempt = 0; attempt < 100; attempt++) {
      if (await evaluate(`!!document.querySelector('.n-activation, .m-strip, .s-strip')`)) break;
      await new Promise((r) => setTimeout(r, 100));
    }
    await evaluate(`(async () => {
    window.island = (await import('/src/island/store.ts')).useIsland;
    window.music = (await import('/src/music/store.ts')).useMusic;
    window.music.setState(s => ({playback: {...s.playback, title: 'A very long song title '.repeat(8)}}));
    await document.fonts.ready;
  })()`);
    let count = 0;
    const microphoneTarget = await evaluate(`(async () => {
    island.setState(s => ({ expanded: false, notice: null, selected: 'controls', settings: {...s.settings, returnHome: true, homeModule: 'sessions'} }));
    island.setState({activities: {microphone: {wing: 54, capsuleContent: 84, opens: 'sessions'}}});
    for (let i = 0; i < 5; i++) await new Promise(requestAnimationFrame);
    const indicator = document.querySelector('.mic-strip');
    if (!indicator) throw new Error('Microphone strip did not render');
    indicator.click();
    for (let i = 0; i < 5; i++) await new Promise(requestAnimationFrame);
    return island.getState().selected;
  })()`);
    assert.equal(microphoneTarget, "sessions", "Clicking the microphone strip must respect the Sessions home page.");
    for (const silhouette of ["notch", "capsule"]) {
      for (const selected of ["music", "sessions", "downloads", "notifications", "controls", "sections"]) {
        for (const [width, height] of [
          [360, 260],
          [600, 260],
          [360, 640],
          [600, 640],
          [240, 120],
        ]) {
          const result = await evaluate(`(async () => {
          island.setState(s => ({ expanded: true, pinned: true, selected: ${JSON.stringify(selected === "sections" ? "music" : selected)}, showingSections: ${selected === "sections"}, settings: {...s.settings, silhouette: ${JSON.stringify(silhouette)}, layout: 'custom', customWidth: ${width}, customHeight: ${height}} }));
          for (let i = 0; i < 5; i++) await new Promise(requestAnimationFrame);
          const expanded = [...document.querySelectorAll('.n-expanded')].at(-1);
          const page = expanded.querySelector('.n-page');
          const frame = expanded.getBoundingClientRect();
          const canvas = expanded.querySelector('.n-expanded-layout').getBoundingClientRect();
          const rect = e => { const r = e.getBoundingClientRect(); return {left:r.left, top:r.top, right:r.right, bottom:r.bottom, width:r.width, height:r.height}; };
          const inside = (a,b) => a.left >= b.left - 1 && a.right <= b.right + 1 && a.top >= b.top - 1 && a.bottom <= b.bottom + 1;
          const parts = [...expanded.querySelectorAll('.m-head, .m-timeline, .m-transport')];
          return { fits: inside(canvas,frame), overflow: getComputedStyle(page).overflow, musicFits: parts.every(e => inside(e.getBoundingClientRect(), page.getBoundingClientRect())), overlaps: parts.some((e,i) => i && e.getBoundingClientRect().top < parts[i-1].getBoundingClientRect().bottom - 1), frame: rect(expanded), page: rect(page), parts: parts.map(rect) };
        })()`);
          const description = `${silhouette} ${selected} ${width}x${height}`;
          assert(result.fits, `Layout escapes island: ${description}`);
          assert.equal(result.overflow, "clip", `Page can scroll: ${description}`);
          assert(
            result.musicFits && !result.overlaps,
            `Music clipped or overlapping: ${description} ${JSON.stringify(result)}`,
          );
          count++;
        }
      }
    }
    const downloads = await evaluate(`(async () => {
    const {useDownloads} = await import('/src/downloads/Downloads.tsx');
    useDownloads.setState({items: Array.from({length: 30}, (_,i) => ({id: String(i), name: 'Download '+i+'.zip', path: '', source: 'Browser', received: 1000, total: 2000, speed: 100, status: 'downloading'}))});
    island.setState({selected: 'downloads', showingSections: false});
    for (let i = 0; i < 5; i++) await new Promise(requestAnimationFrame);
    const list = document.querySelector('.dl-page');
    list.scrollTop = list.scrollHeight;
    return {scrollTop: list.scrollTop, scrollbar: getComputedStyle(list).scrollbarWidth};
  })()`);
    assert(
      downloads.scrollTop > 0 && downloads.scrollbar === "none",
      "Download history must remain reachable without a visible scrollbar.",
    );
    // The long title must move independently, leaving its row and buttons still.
    await send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-reduced-motion", value: "no-preference" }],
    });
    await evaluate(
      `island.setState(s => ({selected: 'music', showingSections: false, settings: {...s.settings, silhouette: 'notch', customWidth: 440, customHeight: 300}}))`,
    );
    await new Promise((r) => setTimeout(r, 700));
    const marquee = await evaluate(
      `(() => { const e = document.querySelector('.m-title'); return {overflows: e.firstElementChild.scrollWidth > e.clientWidth, animations: e.firstElementChild.getAnimations().length}; })()`,
    );
    assert(marquee.overflows && marquee.animations > 0, "Long title has no scrolling animation.");
    const artifact = resolve("../build/island-layout.png");
    await mkdir(resolve("../build"), { recursive: true });
    const screenshot = await send("Page.captureScreenshot", { format: "png" });
    await writeFile(artifact, Buffer.from(screenshot.data, "base64"));
    console.log(`${count} browser layout cases passed; long title animation passed. Screenshot: ${artifact}`);
  }
} finally {
  socket?.close();
  if (browser && browser.exitCode === null) {
    browser.kill();
    await new Promise((resolve) => browser.once("exit", resolve));
  }
  await server.close();
  assert(dirname(resolve(profile)) === resolve(tmpdir()) && basename(profile).startsWith("vorssaint-layout-"));
  await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }).catch(() => {});
}
