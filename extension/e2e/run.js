#!/usr/bin/env node
// gitpass extension end-to-end test. `npm run e2e` (from extension/). See e2e/README.md.
// Launches Playwright Chromium with a COPY of the unpacked extension, a throwaway user-data-dir,
// and a mock native host registered only in <user-data-dir>/NativeMessagingHosts. Never touches the real install.
'use strict';
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http'), crypto = require('crypto');
let chromium;
try { ({ chromium } = require('playwright')); } catch { console.error('playwright missing: run `npm install` in extension/ then `npx playwright install chromium`'); process.exit(2); }

const ROOT = path.join(__dirname, '..');
const HEADED = !!process.env.HEADED;
const KEEP = !!process.env.KEEP_TMP;
const T = (ms) => Number(process.env.E2E_TIMEOUT_MS || 0) || ms;
const results = [];
const rec = (name, ok, detail = '', kind = 'test') => { results.push({ name, ok, detail, kind }); console.log(`${ok ? 'PASS' : kind === 'info' ? 'INFO' : 'FAIL'}  ${name}${detail ? '  — ' + detail : ''}`); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(fn, ms, step = 200) { const end = Date.now() + ms; let last; while (Date.now() < end) { try { last = await fn(); if (last) return last; } catch (_) { /* retry */ } await sleep(step); } return last; }

// Chrome's unpacked-extension id: sha256(abs path) first 32 hex -> a..p.
const extId = (p) => crypto.createHash('sha256').update(p).digest('hex').slice(0, 32).split('').map((c) => String.fromCharCode(97 + parseInt(c, 16))).join('');

const LOGIN_HTML = `<!doctype html><html><head><meta charset="utf-8"><title>Test login</title></head><body>
<h1>Test login</h1><div id="msg"></div>
<form id="f" action="/done" method="post"><input id="u" name="username" type="text" autocomplete="username">
<input id="p" name="password" type="password" autocomplete="current-password"><button id="go" type="submit">Sign in</button></form>
<script>document.getElementById('f').addEventListener('submit',e=>{e.preventDefault();document.getElementById('msg').textContent='signed in as '+document.getElementById('u').value;});</script></body></html>`;

// Find visible elements by text, piercing CLOSED shadow roots via CDP (DOM.getDocument pierce:true).
async function findInShadow(page, re) {
  const s = await page.context().newCDPSession(page);
  try {
    const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true });
    const hits = [];
    const walk = (n, inShadow, chain) => {
      if (n.nodeType === 3 && inShadow && re.test(n.nodeValue || '')) hits.push({ text: n.nodeValue.trim(), chain });
      if (n.nodeType === 1 && inShadow) { const a = n.attributes || []; for (let i = 0; i < a.length; i += 2) if (/^(title|aria-label)$/.test(a[i]) && re.test(a[i + 1])) hits.push({ text: a[i + 1], chain: [...chain, n] }); }
      const kids = [...(n.children || []), ...(n.shadowRoots || []), ...(n.contentDocument ? [n.contentDocument] : [])];
      for (const k of kids) walk(k, inShadow || (n.shadowRoots || []).includes(k) || k.nodeType === 11, n.nodeType === 1 ? [...chain, n] : chain);
    };
    walk(root, false, []);
    const out = [];
    for (const h of hits) {
      // Prefer the nearest clickable ancestor (button / [role] / li), else the parent element.
      const rev = [...h.chain].reverse();
      const el = rev.find((e) => /^(BUTTON|A|LI)$/.test(e.nodeName) || (e.attributes || []).some((a, i, arr) => i % 2 === 0 && a === 'role' && /button|option|menuitem/.test(arr[i + 1]))) || rev[0];
      try { const { model } = await s.send('DOM.getBoxModel', { nodeId: el.nodeId }); const q = model.content; if (model.width > 0 && model.height > 0) out.push({ text: h.text, tag: el.nodeName, x: (q[0] + q[4]) / 2, y: (q[1] + q[5]) / 2 }); } catch (_) { /* not rendered */ }
    }
    return out;
  } finally { await s.detach().catch(() => {}); }
}


// The REAL action popup (chrome.action.openPopup) is not a Playwright page; drive it over CDP.
// It runs as a genuine popup (no sender.tab), with the given tab active, exactly like a user click.
async function openRealPopup(ctx, sw, tabPage, EXT) {
  await tabPage.bringToFront();
  const r = await sw.evaluate(() => chrome.action.openPopup().then(() => 'ok', (e) => String(e && e.message || e))).catch((e) => String(e.message));
  if (r !== 'ok') throw new Error('chrome.action.openPopup failed: ' + r);
  const cdp = await ctx.newCDPSession(tabPage);
  const t = await until(async () => (await cdp.send('Target.getTargets')).targetInfos.find((x) => x.url.startsWith(`${EXT}/popup/`)), 5000);
  if (!t) throw new Error('popup target not found');
  // Playwright's CDPSession can't address flattened child sessions, so use non-flat sendMessageToTarget.
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId: t.targetId, flatten: false });
  let n = 0; const waiting = new Map();
  cdp.on('Target.receivedMessageFromTarget', (e) => { if (e.sessionId !== sessionId) return; const m = JSON.parse(e.message); const w = waiting.get(m.id); if (w) { waiting.delete(m.id); w(m); } });
  const ev = async (expr) => {
    const id = ++n; const p = new Promise((r) => waiting.set(id, r));
    await cdp.send('Target.sendMessageToTarget', { sessionId, message: JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression: expr, awaitPromise: true, returnByValue: true } }) });
    const m = await Promise.race([p, sleep(5000).then(() => ({ error: { message: 'cdp timeout' } }))]);
    if (m.error) throw new Error(m.error.message); if (m.result.exceptionDetails) throw new Error(m.result.exceptionDetails.text); return m.result.result.value;
  };
  return {
    text: () => ev('document.body ? document.body.innerText : ""'),
    click: (src) => ev(`(() => { const re = ${src}; const els = [...document.querySelectorAll('button,[role=button],a,li,[data-item-id]')].filter((e) => e.offsetParent !== null && re.test((e.innerText || e.title || e.getAttribute('aria-label') || '').trim())); if (!els.length) return null; els[0].click(); return (els[0].innerText || els[0].title || '').trim().slice(0, 60); })()`),
    close: () => cdp.send('Target.closeTarget', { targetId: t.targetId }).catch(() => {}),
  };
}

function mockLog(dir) { try { return fs.readFileSync(path.join(dir, 'log.jsonl'), 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l)); } catch { return []; } }
const cmdsIn = (dir) => mockLog(dir).filter((e) => e.dir === 'in').map((e) => e.msg);

(async () => {
  const tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'gitpass-e2e-')));
  const extDir = path.join(tmp, 'ext'), profile = path.join(tmp, 'profile'), mockDir = path.join(tmp, 'mock');
  fs.cpSync(ROOT, extDir, { recursive: true, filter: (src) => !/[/\\](node_modules|e2e|dist|\.git)([/\\]|$)/.test(src.slice(ROOT.length)) });
  const id = extId(extDir);
  fs.mkdirSync(path.join(profile, 'NativeMessagingHosts'), { recursive: true }); fs.mkdirSync(mockDir);
  const wrapper = path.join(tmp, 'mock-host.sh');
  fs.writeFileSync(wrapper, `#!/bin/sh\nexport GITPASS_MOCK_DIR='${mockDir}'\nexec '${process.execPath}' '${path.join(__dirname, 'mock-host.js')}' "$@"\n`, { mode: 0o755 });
  const nm = { name: 'com.gitpass.host', description: 'gitpass mock host (e2e)', path: wrapper, type: 'stdio', allowed_origins: [`chrome-extension://${id}/`] };
  fs.writeFileSync(path.join(profile, 'NativeMessagingHosts', 'com.gitpass.host.json'), JSON.stringify(nm, null, 2));
  // Linux Chromium reads the per-profile dir too; also set via env for safety on Linux CI.
  console.log(`tmp: ${tmp}\nextension id (expected): ${id}`);

  const server = http.createServer((req, res) => { res.setHeader('content-type', 'text/html; charset=utf-8'); res.end(req.url.startsWith('/done') ? '<h1>done</h1>' : LOGIN_HTML); });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const LOGIN = `http://localhost:${server.address().port}/login`;

  const ctx = await chromium.launchPersistentContext(profile, {
    headless: !HEADED, channel: 'chromium', viewport: { width: 1100, height: 800 },
    args: [`--disable-extensions-except=${extDir}`, `--load-extension=${extDir}`],
  });
  let exitCode = 1;
  try {
    let sw = ctx.serviceWorkers()[0] || await ctx.waitForEvent('serviceworker', { timeout: T(15000) }).catch(() => null);
    const swId = sw && new URL(sw.url()).host;
    rec('extension service worker started', !!sw, sw ? sw.url() : 'no service worker');
    if (swId && swId !== id) { rec('extension id matches native-host allowed_origins', false, `got ${swId}, expected ${id}`); throw new Error('id mismatch'); }
    const EXT = `chrome-extension://${id}`;
    const probe = sw && await sw.evaluate(() => new Promise((r) => chrome.runtime.sendNativeMessage('com.gitpass.host', { id: 9999, cmd: 'status' }, (m) => r(chrome.runtime.lastError ? 'ERR ' + chrome.runtime.lastError.message : JSON.stringify(m))))).catch((e) => 'ERR ' + e.message);
    rec('mock native host reachable from extension', !!probe && !String(probe).startsWith('ERR'), String(probe).slice(0, 120));

    const welcome = await until(() => ctx.pages().find((p) => p.url().startsWith(`${EXT}/welcome/`)), T(4000));
    rec('welcome page opened on install', !!welcome, welcome ? '' : 'needs importScripts(\'bg-onboard.js\') in background.js', welcome ? 'test' : 'info');
    if (welcome) {
      const ok = await until(() => welcome.locator('#connect-status').textContent().then((t) => /Connected/.test(t) && t), T(8000));
      rec('welcome: connect step detects host+app', !!ok, ok || await welcome.locator('#connect-status').textContent().catch(() => ''));
      rec('welcome: steps-left checklist', /step|All set/.test(await welcome.locator('#left').textContent()), await welcome.locator('#left').textContent());
    }

    // --- Real popup on the login tab: locked -> unlock event -> list -> fill
    const page = await ctx.newPage();
    await page.goto(LOGIN); await sleep(1000);
    const titles = /Localhost Test Login|GitHub|Visa|Wi-Fi notes/;
    let pop = null;
    try { pop = await openRealPopup(ctx, sw, page, EXT); } catch (e) { rec('open real popup', false, e.message); }
    if (pop) {
      const lockedTxt = await until(async () => { const t = await pop.text(); return /unlock/i.test(t) && !titles.test(t) && t; }, T(8000));
      rec('popup shows locked view', !!lockedTxt, lockedTxt ? '' : (await pop.text()).slice(0, 160).replace(/\s+/g, ' '));
      const before = cmdsIn(mockDir).length;
      fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({ unlocked: true, seq: Date.now() })); // simulate unlocking the desktop app
      const live = await until(async () => titles.test(await pop.text()), T(8000));
      rec('unlock event from app updates open popup (no password typed)', !!live, live ? '' : 'popup did not react to {"event":"unlock"}: ' + (await pop.text()).slice(0, 120).replace(/\s+/g, ' '));
      if (!live) { await pop.close(); pop = await openRealPopup(ctx, sw, page, EXT).catch(() => null); }
      const listed = pop && await until(async () => { const t = await pop.text(); return titles.test(t) && t.match(titles)[0]; }, T(8000));
      const after = cmdsIn(mockDir).slice(before).map((m) => m.cmd);
      rec('popup lists items from host', !!listed, `${listed || 'none visible'}; host cmds after unlock: ${[...new Set(after)].join(',') || 'none'}`);
    }
    if (pop) await pop.close();

    // In-page dropdown (closed shadow DOM, pierced via CDP) first; popup "Fill" as fallback.
    const PW = 'Corr3ct-H0rse-Battery';
    const isFilled = async () => (await page.inputValue('#p')) === PW && (await page.inputValue('#u')) === 'alice@example.com';
    await page.bringToFront(); await page.click('#u'); await sleep(800);
    const hits = await until(async () => { const h = await findInShadow(page, /Localhost Test Login|alice@example\.com/); return h.length && h; }, T(5000));
    let hitsUsed = hits, badgeInfo = '';
    if (!hits) {
      // Older UI: a field badge opens the menu. Click the badge, then the entry.
      const badge = await findInShadow(page, /gitpass/i);
      badgeInfo = `; badge found=${badge.length}`;
      if (badge.length) { await page.mouse.click(badge[0].x, badge[0].y); await sleep(800); badgeInfo += '; menu text=' + JSON.stringify((await findInShadow(page, /\S/)).map((h) => h.text).slice(0, 6)); hitsUsed = await until(async () => { const h = await findInShadow(page, /Localhost Test Login|alice@example\.com/); return h.length && h; }, T(4000)); }
    }
    if (hitsUsed) await page.mouse.click(hitsUsed[0].x, hitsUsed[0].y);
    const hits2 = hitsUsed;
    const inpage = await until(isFilled, T(3000));
    rec('fill via in-page dropdown', !!inpage, (hits2 ? `clicked ${hits2[0].tag} "${hits2[0].text}"` : 'no dropdown entry found in shadow DOM') + badgeInfo + `; u=${JSON.stringify(await page.inputValue('#u'))}`);
    await page.fill('#u', ''); await page.fill('#p', '');
    let viaPopup = false, how = '';
    try {
      const p2 = await openRealPopup(ctx, sw, page, EXT);
      await until(async () => titles.test(await p2.text()), T(5000));
      how = 'clicked item: ' + (await p2.click('/Localhost Test Login/'));
      await sleep(500);
      how += '; clicked: ' + (await p2.click('/^(Open & Fill|Fill|Autofill)\\b/i'));
      viaPopup = await until(isFilled, T(3000));
      await p2.close();
    } catch (e) { how += ' ' + e.message; }
    rec('fill via popup', !!viaPopup, `${how}; u=${JSON.stringify(await page.inputValue('#u'))} p=${(await page.inputValue('#p')) ? '[set]' : '[empty]'}; fill cmd sent=${cmdsIn(mockDir).some((m) => m.cmd === 'fill')}`);

    // --- Save prompt after submit
    await page.goto(LOGIN); await sleep(1000);
    await page.fill('#u', 'newuser@example.com'); await page.fill('#p', 'Brand-New-Pass-42'); await page.click('#go');
    const banner = await until(async () => { const h = await findInShadow(page, /^\s*(Save|บันทึก|သိမ်းရန်)\s*$/i)  /* the button, not the 'Save login to gitpass?' title */; return h.length && h; }, T(6000));
    if (banner) { await page.mouse.click(banner[0].x, banner[0].y); }
    const saved = await until(() => cmdsIn(mockDir).find((m) => m.cmd === 'save_login' && m.username === 'newuser@example.com'), T(4000));
    rec('save-login prompt after submit -> save_login sent', !!saved, banner ? `banner button "${banner[0].text}"; save_login ${saved ? 'received' : 'NOT received'}` : 'no Save button found in page shadow DOM');

    // --- Options page
    const popup = await ctx.newPage();
    await popup.goto(`${EXT}/options/options.html`);
    const opt = await until(async () => (await popup.locator('#autofill').count()) && (await popup.locator('#shortcut').textContent()) !== '—', T(4000));
    rec('options page renders settings + shortcut', !!opt, opt ? `shortcut=${await popup.locator('#shortcut').textContent()}` : '');
    const failed = results.filter((r) => !r.ok && r.kind !== 'info');
    exitCode = failed.length ? 1 : 0;
  } catch (e) { rec('harness', false, e.message); } finally {
    fs.writeFileSync(path.join(tmp, 'results.json'), JSON.stringify(results, null, 2));
    console.log(`\n${results.filter((r) => r.ok).length}/${results.length} passed. mock host log: ${path.join(mockDir, 'log.jsonl')}`);
    await ctx.close().catch(() => {}); server.close();
    if (!KEEP && exitCode === 0) fs.rmSync(tmp, { recursive: true, force: true });
    process.exit(exitCode);
  }
})();
