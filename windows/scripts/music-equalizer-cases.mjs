// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import assert from "node:assert/strict";

// Exercise actual React effects and bar styles in Edge, using only mocked IPC.
// Native audio itself is covered by the opt-in Rust WASAPI tests.
export async function checkMusicEqualizer({ send, evaluate, base }) {
  await send("Page.navigate", { url: `${base}bar.html` });
  const wait = async (expression) => {
    for (let i = 0; i < 100; i++) {
      if (await evaluate(expression)) return;
      await new Promise((resolve) => setTimeout(resolve, 30));
    }
    assert.fail(`Timed out: ${expression}`);
  };
  await wait("document.readyState === 'complete'");
  await evaluate(`(async () => {
    const React = await import('/node_modules/.vite/deps/react.js');
    const ReactDOM = await import('/node_modules/.vite/deps/react-dom_client.js');
    const {createElement} = React.default ?? React;
    const {createRoot} = ReactDOM.default ?? ReactDOM;
    const {EqualizerBars} = await import('/src/music/EqualizerBars.tsx');
    window.music = (await import('/src/music/store.ts')).useMusic;
    const {bridge} = await import('/src/shared/bridge.ts');
    window.watches = []; window.unlistens = 0;
    bridge.music.watchLevels = async on => watches.push(on);
    bridge.music.onLevels = async handler => { window.audioFrame = handler; return () => unlistens++; };
    music.setState({playback:{source:'player-a',isPlaying:true}});
    const host = document.createElement('div'); host.id = 'equalizer-test'; document.body.append(host);
    window.equalizerRoot = createRoot(host);
    window.renderBars = moving => equalizerRoot.render(createElement(EqualizerBars, {count:7,width:2,height:16,color:'#fff',moving}));
    window.heights = () => [...host.querySelectorAll('.m-bars > span')].map(el => parseFloat(el.style.height));
    window.frame = (levels, source='player-a', mode='process') => audioFrame({levels,source,mode});
    renderBars(true);
  })()`);
  await wait("!!document.querySelector('#equalizer-test .m-bars')");
  assert.deepEqual(await evaluate("heights()"), Array(7).fill(2));
  assert.equal(await evaluate("watches.length"), 0, "Reduced motion should not request capture");
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await wait("watches.at(-1) === true && typeof audioFrame === 'function'");
  await evaluate("frame([0,.1,.2,.3,.4,.5,1])");
  assert.deepEqual(await evaluate("heights()"), [7.6, 3.4, 9, 2, 6.2, 16, 4.8]);
  await evaluate("frame(Array(7).fill(.1))");
  assert.deepEqual(await evaluate("heights()"), Array(7).fill(3.4), "Quiet data must reduce height");
  await evaluate("frame(Array(7).fill(0))");
  assert.deepEqual(await evaluate("heights()"), Array(7).fill(2), "Silence must rest as dots");
  await evaluate("frame(Array(7).fill(1), 'player-b')");
  assert.deepEqual(await evaluate("heights()"), Array(7).fill(2), "Reject another source");
  await evaluate("frame(Array(7).fill(1), 'player-a', 'system')");
  assert(await evaluate("document.querySelector('#equalizer-test .m-bars').title.includes('System audio')"));
  await wait("heights().every(h => h === 2)");
  await evaluate("frame(Array(7).fill(1)); renderBars(false)");
  await wait("watches.at(-1) === false && heights().every(h => h === 2)");
  await evaluate("music.setState({playback:{source:'player-b',isPlaying:true}}); renderBars(true)");
  await wait("watches.at(-1) === true");
  await evaluate("frame(Array(7).fill(.5), 'player-b')");
  assert.deepEqual(await evaluate("heights()"), Array(7).fill(9), "Follow a newly selected player");
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await wait("watches.at(-1) === false && heights().every(h => h === 2)");
  await evaluate("equalizerRoot.unmount()");
  assert.equal(await evaluate("unlistens"), 2, "Every registered listener should be removed");
  console.log(
    "Music equalizer: real bar data, volume, silence, stale events, pause, source selection, reduced motion and cleanup passed.",
  );
}
