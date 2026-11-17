// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { mkdir, writeFile, readFile } from "node:fs/promises";
import { resolve } from "node:path";

export async function checkI18n({ send, evaluate, base }) {
  const wait = async (expression) => {
    for (let i = 0; i < 100; i++) {
      if (await evaluate(expression)) return;
      await new Promise((r) => setTimeout(r, 100));
    }
    assert.fail(`Timed out: ${expression}`);
  };
  await send("Page.navigate", { url: `${base}settings.html#general` });
  await wait(`!!document.querySelector('.st-language [role="combobox"]')`);
  const languages = await evaluate(`(async()=> (await import('/src/i18n/index.ts')).LANGUAGES)()`);
  assert.equal(languages.length, 15);
  for (const { value, label } of languages) {
    await evaluate(`document.querySelector('.st-language [role="combobox"]').click()`);
    await wait(`document.querySelectorAll('[role="option"]').length===16`);
    const options = await evaluate(`[...document.querySelectorAll('[role="option"]')].map(e=>e.textContent.trim())`);
    for (const lang of languages) assert(options.includes(lang.label), `Missing native name ${lang.label}`);
    await evaluate(
      `[...document.querySelectorAll('[role="option"]')].find(e=>e.textContent.trim()===${JSON.stringify(label)}).click()`,
    );
    await wait(
      `document.documentElement.lang===${JSON.stringify(value)} && document.querySelector('.st-page-title')?.textContent===(window.__i18nTitle)`.replace(
        "(window.__i18nTitle)",
        JSON.stringify(JSON.parse(await readFile(`src/i18n/generated/${value}.json`, "utf8")).s.tabGeneral),
      ),
    );
    assert.equal(
      await evaluate(`(async()=> (await import('/src/shared/prefs.ts')).loadPrefs().then(p=>p.island.language))()`),
      value,
    );
    assert.equal(await evaluate(`document.querySelector('.st-language [role="combobox"]').textContent.trim()`), label);
    const bounds = await evaluate(
      `(()=>{const e=document.querySelector('.st-language'),r=e.getBoundingClientRect();return {right:r.right,width:innerWidth,scroll:document.documentElement.scrollWidth}})()`,
    );
    assert(bounds.right <= bounds.width + 1 && bounds.scroll <= bounds.width + 1, `Settings overflow ${value}`);
    if (value === "de" || value === "ja") {
      const screenshot = await send("Page.captureScreenshot", { format: "png" });
      await mkdir(resolve("build/i18n"), { recursive: true });
      await writeFile(resolve(`build/i18n/settings-${value}.png`), Buffer.from(screenshot.data, "base64"));
    }
  }
  await send("Page.navigate", { url: `${base}island.html?sessions` });
  await wait(`!!document.querySelector('.island-frame')`);
  await evaluate(`(async()=>{
    window.island=(await import('/src/island/store.ts')).useIsland;
    window.sessions=(await import('/src/sessions/store.ts')).useSessions;
    const demo=(await import('/src/sessions/demo.ts')).demoSnapshot();
    sessions.setState({snapshot:demo,focused:demo.sessions[0].id,renaming:true,names:{}});
    island.setState(s=>({expanded:true,pinned:true,selected:'sessions',notice:null,showingSections:false,settings:{...s.settings,layout:'custom',customWidth:600,customHeight:420}}));
  })()`);
  await wait(`!!document.querySelector('.s-rename')`);
  await send("Input.insertText", { text: "Keep my draft 日本語" });
  await evaluate(`void (window.originalInput=document.querySelector('.s-rename'))`);
  for (const { value } of languages) {
    await evaluate(
      `(async()=>{await (await import('/src/shared/prefs.ts')).setPrefs({island:{language:${JSON.stringify(value)}}})})()`,
    );
    await wait(`document.documentElement.lang===${JSON.stringify(value)}`);
    assert(
      await evaluate(
        `document.querySelector('.s-rename')===originalInput && originalInput.value==='Keep my draft 日本語'`,
      ),
      `Language switch remounted input ${value}`,
    );
    assert(
      await evaluate(
        `(()=>{const r=originalInput.getBoundingClientRect(),p=document.querySelector('.n-page').getBoundingClientRect();return r.left>=p.left-1&&r.right<=p.right+1})()`,
      ),
      `Rename overflow ${value}`,
    );
  }
  console.log(
    "i18n browser checks passed: all 15 native language choices, live Settings title, saved preference, Settings fit, and live island switches preserving the rename input/draft. Screenshots: windows/build/i18n.",
  );
}
