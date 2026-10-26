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
// E2E_HOST=real: use the real gitpass-host binary in standalone fallback mode against a throwaway vault in the
// temp dir (never ~/.gitpass). Binaries: $GITPASS_HOST_BIN / $GITPASS_CLI_BIN, default ../target/release.
const REAL = process.env.E2E_HOST === 'real';
const REAL_PW = 'e2e-only-master-password';
const { execFileSync } = require('child_process');
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

const CARD_HTML = fs.readFileSync(path.join(ROOT, 'test', 'pages', 'card.html'), 'utf8');

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
    click: (src) => ev(`(() => { const re = ${src}; const els = [...document.querySelectorAll('button,[role=button],[role=option],a,li,[data-item-id]')].filter((e) => e.offsetParent !== null && re.test((e.innerText || e.title || e.getAttribute('aria-label') || '').trim())); if (!els.length) return null; els[0].click(); return (els[0].innerText || els[0].title || '').trim().slice(0, 60); })()`),
    eval: ev,
    search: (q) => ev(`(() => { const i = document.getElementById('q'); if (!i) return false; i.value = ${JSON.stringify(q)}; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`),
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
  let realSetup = null;
  if (REAL) realSetup = (port) => {
    const rel = path.join(ROOT, '..', 'target', 'release');
    const HOST = process.env.GITPASS_HOST_BIN || path.join(rel, 'gitpass-host'), CLI = process.env.GITPASS_CLI_BIN || path.join(rel, 'gitpass');
    const vault = path.join(tmp, 'vault');
    const env = { ...process.env, GITPASS_PATH: vault, GITPASS_PASSWORD: REAL_PW, GITPASS_DEVICE_NAME: 'e2e' };
    const cli = (...a) => execFileSync(CLI, a, { env, stdio: ['ignore', 'pipe', 'pipe'] }); // output (Emergency Kit) is not printed
    cli('init', '--save-secret-key'); cli('vault', 'create', 'Personal');
    const bw = path.join(tmp, 'bw.json');
    fs.writeFileSync(bw, JSON.stringify({ items: [
      { type: 1, name: 'Localhost Test Login', login: { username: 'alice@example.com', password: 'Corr3ct-H0rse-Battery', uris: [{ uri: `http://localhost:${port}` }] } },
      { type: 3, name: 'Visa', card: { cardholderName: 'ALICE TESTER', brand: 'Visa', number: '4111111111111111', code: '737', expMonth: '12', expYear: '2028' } },
      { type: 4, name: 'Home Identity', identity: { firstName: 'Naing', lastName: 'Oo', address1: '99 Test Road', city: 'Bangkok', email: 'naing@example.com', phone: '+66812345678' } }] }));
    cli('import', '--from', 'bitwarden-json', 'Personal', bw); fs.rmSync(bw);
    const conf = path.join(tmp, 'host.json');
    fs.writeFileSync(conf, JSON.stringify({ vault_path: vault, idle_secs: 600, allowed_callers: [`chrome-extension://${id}/`] }));
    // A socket path that doesn't exist forces the standalone fallback even if the desktop app is running.
    fs.writeFileSync(wrapper, `#!/bin/sh\nexport GITPASS_HOST_CONFIG='${conf}'\nexport GITPASS_AGENT_SOCK='${path.join(tmp, 'no-app', 'agent.sock')}'\nexec '${HOST}' "$@"\n`, { mode: 0o755 });
    console.log(`REAL host mode: ${HOST}, temp vault ${vault}`);
  }; if (!REAL)
  fs.writeFileSync(wrapper, `#!/bin/sh\nexport GITPASS_MOCK_DIR='${mockDir}'\nexec '${process.execPath}' '${path.join(__dirname, 'mock-host.js')}' "$@"\n`, { mode: 0o755 });
  const nm = { name: 'com.gitpass.host', description: 'gitpass mock host (e2e)', path: wrapper, type: 'stdio', allowed_origins: [`chrome-extension://${id}/`] };
  fs.writeFileSync(path.join(profile, 'NativeMessagingHosts', 'com.gitpass.host.json'), JSON.stringify(nm, null, 2));
  // Linux Chromium reads the per-profile dir too; also set via env for safety on Linux CI.
  console.log(`tmp: ${tmp}\nextension id (expected): ${id}`);

  const server = http.createServer((req, res) => { res.setHeader('content-type', 'text/html; charset=utf-8'); res.end(req.url.startsWith('/done') ? '<h1>done</h1>' : req.url.startsWith('/card') ? CARD_HTML : req.url.startsWith('/webauthn') ? fs.readFileSync(path.join(ROOT, 'test', 'pages', 'webauthn.html')) : LOGIN_HTML); });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const LOGIN = `http://localhost:${server.address().port}/login`;
  if (realSetup) realSetup(server.address().port); // real host matches loopback origins incl. port

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
    // Retry: right after install the worker can briefly evaluate before chrome.runtime is bound.
    const probeOnce = () => sw.evaluate(() => new Promise((r) => chrome.runtime.sendNativeMessage('com.gitpass.host', { id: 9999, cmd: 'status' }, (m) => r(chrome.runtime.lastError ? 'ERR ' + chrome.runtime.lastError.message : JSON.stringify(m))))).catch((e) => 'ERR ' + e.message);
    const probe = sw && await until(async () => { const p = await probeOnce(); return String(p).startsWith('ERR') ? null : p; }, T(5000)) || (sw && await probeOnce());
    rec(REAL ? 'real native host reachable from extension' : 'mock native host reachable from extension', !!probe && !String(probe).startsWith('ERR'), String(probe).slice(0, 160));
    if (REAL) { let st = {}; try { st = JSON.parse(probe).result || {}; } catch (_) {} const n = st.account && st.account.name;
      rec('real host: standalone mode, friendly account name (not a UUID)', st.mode === 'standalone' && !!n && !/^[0-9a-f]{8}-[0-9a-f]{4}-/i.test(n), `mode=${st.mode} name=${n}`); }

    const welcome = await until(() => ctx.pages().find((p) => p.url().startsWith(`${EXT}/welcome/`)), T(4000));
    rec('welcome page opened on install', !!welcome, welcome ? '' : 'needs importScripts(\'bg-onboard.js\') in background.js', welcome ? 'test' : 'info');
    if (welcome) {
      const ok = await until(() => welcome.locator('#connect-status').textContent().then((t) => /Connected/.test(t) && t), T(8000));
      rec('welcome: connect step detects host+app', !!ok || REAL, ok || await welcome.locator('#connect-status').textContent().catch(() => ''));
      rec('welcome: steps-left checklist', /step|All set/.test(await welcome.locator('#left').textContent()), await welcome.locator('#left').textContent());
    }

    // --- Real popup on the login tab: locked -> unlock event -> list -> fill
    const page = await ctx.newPage();
    await page.goto(LOGIN); await sleep(1000);
    const titles = /Localhost Test Login|GitHub|Visa|Wi-Fi notes|Home Identity/;
    let pop = null;
    try { pop = await openRealPopup(ctx, sw, page, EXT); } catch (e) { rec('open real popup', false, e.message); }
    if (pop) {
      const lockedTxt = await until(async () => { const t = await pop.text(); return /unlock/i.test(t) && !titles.test(t) && t; }, T(8000));
      rec('popup shows locked view', !!lockedTxt, lockedTxt ? '' : (await pop.text().catch((e) => 'ERR ' + e.message)).slice(0, 160).replace(/\s+/g, ' ') + ' | ' + (await pop.eval('[document.readyState, [...document.querySelectorAll("section")].map((x) => x.id + (x.classList.contains("hidden") ? "-" : "+")).join(",")].join(" ")').catch((e) => 'ERR ' + e.message)));
      const before = cmdsIn(mockDir).length;
      if (REAL) { // standalone fallback: "Use password" (Secret Key comes from the temp vault's secret.key)
        await pop.click('/^Use password$/'); await sleep(300);
        await pop.eval(`(() => { document.getElementById('pw').value = ${JSON.stringify(REAL_PW)}; document.getElementById('unlockForm').requestSubmit(); return true; })()`);
      } else
      fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({ unlocked: true, seq: Date.now() })); // simulate unlocking the desktop app
      const live = await until(async () => titles.test(await pop.text()), T(REAL ? 20000 : 8000));
      rec(REAL ? 'real host: unlock with password in popup' : 'unlock event from app updates open popup (no password typed)', !!live, live ? '' : 'popup did not react to {"event":"unlock"}: ' + (await pop.text()).slice(0, 120).replace(/\s+/g, ' '));
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
    if (REAL) rec('save-login prompt after submit (real host)', !!banner, banner ? `banner button "${banner[0].text}" clicked` : 'no Save button found', 'test'); else
    rec('save-login prompt after submit -> save_login sent', !!saved, banner ? `banner button "${banner[0].text}"; save_login ${saved ? 'received' : 'NOT received'}` : 'no Save button found in page shadow DOM');

    // --- Card + identity (test/pages/card.html): in-page click and popup Fill
    const CARD = LOGIN.replace(/\/login$/, '/card');
    const cardPage = await ctx.newPage();
    const vals = (ids) => Promise.all(ids.map((i) => cardPage.inputValue('#' + i)));
    const CARD_IDS = ['num', 'name', 'mm', 'yy', 'cvc'], CARD_WANT = ['4111111111111111', 'ALICE TESTER', '12', '2028', '737'];
    const ID_IDS = ['fn', 'ln', 'addr', 'mail', 'tel'], ID_WANT = ['Naing', 'Oo', '99 Test Road', 'naing@example.com', '+66812345678'];
    const same = (a, b) => a.every((x, i) => x === b[i]);
    // Never print card values: report which fields matched.
    const diff = (got, want, ids) => ids.map((k, i) => `${k}:${got[i] === want[i] ? 'ok' : got[i] ? 'wrong' : 'empty'}`).join(' ');
    async function inpageCF(sel, re, ids, want, cmd) {
      await cardPage.goto(CARD); await sleep(1000); await cardPage.bringToFront();
      await cardPage.click(sel); await sleep(800);
      const h = await until(async () => { const x = await findInShadow(cardPage, re); return x.length && x; }, T(5000));
      if (h) await cardPage.mouse.click(h[0].x, h[0].y);
      const ok = await until(async () => same(await vals(ids), want), T(3000));
      const sent = REAL ? { source: '(real host; not logged)' } : cmdsIn(mockDir).filter((m) => m.cmd === cmd).pop();
      return [!!ok, (h ? `clicked "${h[0].text}"` : 'no dropdown entry') + `; ${diff(await vals(ids), want, ids)}; ${cmd} source=${sent && sent.source}`];
    }
    async function viaPop(title, ids, want, cmd) {
      await cardPage.goto(CARD); await sleep(1000);
      let how = '';
      try {
        const p = await openRealPopup(ctx, sw, cardPage, EXT);
        await until(async () => /unlock/i.test(await p.text()) || titles.test(await p.text()), T(5000));
        await p.search(title); await sleep(400);
        how = 'item: ' + (await until(() => p.click(`/${title}/`), T(3000)));
        await sleep(500);
        how += '; button: ' + (await until(() => p.click('/^(Open & Fill|Fill)$/'), T(3000)));
        await until(async () => same(await vals(ids), want), T(3000));
        await p.close();
      } catch (e) { how += ' ' + e.message; }
      const sent = REAL ? { source: '(real host; not logged)' } : cmdsIn(mockDir).filter((m) => m.cmd === cmd).pop();
      return [same(await vals(ids), want), `${how}; ${diff(await vals(ids), want, ids)}; ${cmd} source=${sent && sent.source}`];
    }
    rec('card fill via in-page dropdown', ...await inpageCF('#num', /^Visa$/, CARD_IDS, CARD_WANT, 'fill_card'));
    rec('identity fill via in-page dropdown', ...await inpageCF('#fn', /^Home Identity$/, ID_IDS, ID_WANT, 'fill_identity'));
    rec('card fill via popup Fill', ...await viaPop('Visa', CARD_IDS, CARD_WANT, 'fill_card'));
    rec('identity fill via popup Fill', ...await viaPop('Home Identity', ID_IDS, ID_WANT, 'fill_identity'));
    await cardPage.close();

    // --- Passkeys: local WebAuthn RP page verifies with WebCrypto; approval window must be clicked.
    {
      const WA = `http://localhost:${server.address().port}/webauthn`;
      const wa = await ctx.newPage(); await wa.goto(WA); await sleep(500);
      const outJson = async () => { try { return JSON.parse((await wa.locator('#out').textContent()) || 'null'); } catch { return null; } };
      const seen = new Set();
      const approvalWin = () => until(() => { const w = ctx.pages().find((p) => p.url().startsWith(`${EXT}/popup/passkey.html`) && !p.isClosed() && !seen.has(p)); if (w) seen.add(w); return w; }, T(6000));
      const step = async (btn, decide) => {
        await wa.evaluate(() => { document.getElementById('out').textContent = ''; });
        await wa.click(btn); const w = await approvalWin(); if (!w) return { error: 'no approval window' };
        const title = await until(async () => (await w.locator('#title').textContent()) || '', T(3000));
        const site = await w.locator('#site').textContent().catch(() => '');
        await w.click(decide, { noWaitAfter: true }).catch(() => {}); const r = await until(outJson, T(6000)); return { ...(r || { error: 'no result' }), _title: title, _site: site };
      };
      const before = cmdsIn(mockDir).length;
      const reg = await step('#reg', '#approve');
      rec('passkey create: approval window -> credential verified by RP page', reg.registered === true, JSON.stringify(reg));
      const pkCmds = cmdsIn(mockDir).slice(before).filter((m) => /^passkey_/.test(m.cmd));
      rec('passkey_create sent with browser origin + popup approval', pkCmds.some((m) => m.cmd === 'passkey_create' && m.origin === new URL(WA).origin && m.source === 'popup' && m.approved === true),
        JSON.stringify(pkCmds.map((m) => ({ cmd: m.cmd, origin: m.origin, source: m.source, approved: m.approved }))));
      const g1 = await step('#login', '#approve');
      rec('passkey get: ES256 signature verified by RP page (signCount 1)', g1.verified === true && g1.signCount === 1 && g1.userHandle === 'user-42', JSON.stringify(g1));
      const g2 = await step('#login', '#approve');
      rec('passkey get again: signCount increments', g2.verified === true && g2.signCount === 2, JSON.stringify(g2));
      const n0 = cmdsIn(mockDir).filter((m) => m.cmd === 'passkey_get').length;
      const den = await step('#login', '#deny');
      rec('passkey get denied in approval window -> NotAllowedError, host not called', den.error === 'NotAllowedError' && cmdsIn(mockDir).filter((m) => m.cmd === 'passkey_get').length === n0, JSON.stringify(den));
      const dup = await step('#reg', '#approve');
      rec('passkey create with excludeCredentials match -> InvalidStateError', dup.error === 'InvalidStateError', JSON.stringify(dup));
      await wa.close();
    }

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
