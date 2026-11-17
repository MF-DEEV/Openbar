import { chromium } from 'playwright';
import fs from 'node:fs';
const [,, url, out, w, h, frames, period, dpr] = process.argv;
const b = await chromium.launch({ args: ['--use-angle=metal', '--ignore-gpu-blocklist'] });
const p = await b.newPage({ viewport: { width: +w, height: +h }, deviceScaleFactor: +(dpr ?? 1) });
p.on('console', m => console.log('console:', m.text())); p.on('pageerror', e => console.log('err:', e.message));
await p.goto(url); await p.waitForFunction(() => window.ready === true, null, { timeout: 30000 });
const n = +frames;
if (n <= 1) { await p.screenshot({ path: out }); }
else { fs.mkdirSync(out, { recursive: true });
  for (let i = 0; i < n; i++) { await p.evaluate((t) => window.render(t), i * (+period) / n); await p.screenshot({ path: `${out}/f${String(i).padStart(4,'0')}.png` }); } }
await b.close(); console.log('done');
