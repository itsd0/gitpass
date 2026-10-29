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

// Two-page login (x.com style): step 1 takes the username, step 2 only the password. The step-2 form has
// no username field, so saving it relies on the background's per-tab lastUsername from step 1.
const MULTISTEP_USER_HTML = `<!doctype html><html><head><meta charset="utf-8"><title>Test sign in</title></head><body>
<h1>Sign in to X</h1>
<form id="f" action="/multistep/password" method="get"><input id="u" name="username" type="text" autocomplete="username" placeholder="Phone, email, or username">
<button id="next" type="submit">Next</button></form>
<script>document.getElementById('f').addEventListener('submit',e=>{e.preventDefault();setTimeout(()=>location.assign('/multistep/password'),150);});</script></body></html>`;

const OTP_HTML = `<!doctype html><html><head><meta charset="utf-8"><title>Two-factor</title></head><body>
<h1>Enter your verification code</h1><form id="f"><input id="otp" name="otp" autocomplete="one-time-code" inputmode="numeric" maxlength="6" placeholder="Code"><button type="submit">Verify</button></form></body></html>`;
const MULTISTEP_PW_HTML = `<!doctype html><html><head><meta charset="utf-8"><title>Enter your password</title></head><body>
<h1>Enter your password</h1><div id="msg"></div>
<form id="f"><input id="p" name="password" type="password" autocomplete="current-password" placeholder="Password">
<button id="go" type="submit">Log in</button></form>
<script>document.getElementById('f').addEventListener('submit',e=>{e.preventDefault();document.getElementById('msg').textContent='signed in';});</script></body></html>`;

const SIGNUP_RULES_HTML = fs.readFileSync(path.join(ROOT, 'test', 'pages', 'signup-rules.html'), 'utf8');
const CARD_HTML = fs.readFileSync(path.join(ROOT, 'test', 'pages', 'card.html'), 'utf8');

// Find visible elements by text, piercing CLOSED shadow roots via CDP (DOM.getDocument pierce:true).
async function findInShadow(page, re) {
  const s = await page.context().newCDPSession(page);
  try {
    const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true });
    const hits = [];
    const walk = (n, inShadow, chain) => {
      if (n.nodeType === 3 && inShadow && re.test(n.nodeValue || '')) hits.push({ text: n.nodeValue.trim(), chain });
      if (n.nodeType === 1 && inShadow) { const a = n.attributes || []; for (let i = 0; i < a.length; i += 2) if (/^(title|aria-label|placeholder)$/.test(a[i]) && re.test(a[i + 1])) hits.push({ text: a[i + 1], chain: [...chain, n] }); }
      const kids = [...(n.children || []), ...(n.shadowRoots || []), ...(n.contentDocument ? [n.contentDocument] : [])];
      for (const k of kids) walk(k, inShadow || (n.shadowRoots || []).includes(k) || k.nodeType === 11, n.nodeType === 1 ? [...chain, n] : chain);
    };
    walk(root, false, []);
    const out = [];
    for (const h of hits) {
      // Prefer the nearest clickable ancestor (button / [role] / li / input), else the parent element.
      const rev = [...h.chain].reverse();
      const el = rev.find((e) => /^(BUTTON|A|LI|INPUT)$/.test(e.nodeName) || (e.attributes || []).some((a, i, arr) => i % 2 === 0 && a === 'role' && /button|option|menuitem/.test(arr[i + 1]))) || rev[0];
      try { const { model } = await s.send('DOM.getBoxModel', { nodeId: el.nodeId }); const q = model.content; if (model.width > 0 && model.height > 0) out.push({ text: h.text, tag: el.nodeName, x: (q[0] + q[4]) / 2, y: (q[1] + q[5]) / 2 }); } catch (_) { /* not rendered */ }
    }
    return out;
  } finally { await s.detach().catch(() => {}); }
}


// Node id of the first element matching `selector` inside any <gitpass-ui> closed shadow root.
// (Closed roots are invisible to page JS; the DOM agent can still pierce them.)
async function shadowNode(page, selector) {
  const s = await cdpSession(page);
  await s.send('DOM.enable');
  const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true });
  const roots = [];
  const walk = (n) => { if (n.nodeName === 'GITPASS-UI') roots.push(...(n.shadowRoots || [])); for (const k of (n.children || [])) walk(k); };
  walk(root);
  for (const sr of roots) {
    let nodeIds = [];
    try { ({ nodeIds } = await s.send('DOM.querySelectorAll', { nodeId: sr.nodeId, selector })); } catch (_) { continue; }
    if (nodeIds.length) return nodeIds[0];
  }
  return null;
}

async function shadowFocus(page, selector) {
  const nodeId = await shadowNode(page, selector);
  if (nodeId == null) return false;
  await (await cdpSession(page)).send('DOM.focus', { nodeId });
  return true;
}

async function shadowSetValue(page, selector, value) {
  const s = await cdpSession(page);
  const nodeId = await shadowNode(page, selector);
  if (nodeId == null) return false;
  const { object } = await s.send('DOM.resolveNode', { nodeId });
  await s.send('Runtime.callFunctionOn', {
    objectId: object.objectId,
    functionDeclaration: 'function (v) { this.value = v; this.dispatchEvent(new Event("change", { bubbles: true })); }',
    arguments: [{ value }],
  });
  return true;
}


// CSS.getComputedStyleForNode (with CSS.enable) sees shadow-DOM nodes found via DOM.getDocument pierce,
// so prefers-color-scheme emulation can be asserted without opening the shadow root.
async function shadowComputed(page, selector) {
  const s = await cdpSession(page);
  await s.send('DOM.enable');
  await s.send('CSS.enable');
  const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true });
  const roots = [];
  const walk = (n) => {
    if (n.nodeName === 'GITPASS-UI') roots.push(...(n.shadowRoots || []));
    for (const k of (n.children || [])) walk(k);
  };
  walk(root);
  for (const sr of roots) {
    let nodeIds = [];
    try { ({ nodeIds } = await s.send('DOM.querySelectorAll', { nodeId: sr.nodeId, selector })); } catch (_) { continue; }
    if (!nodeIds.length) continue;
    const { computedStyle } = await s.send('CSS.getComputedStyleForNode', { nodeId: nodeIds[0] });
    const out = {};
    for (const p of computedStyle) out[p.name] = p.value;
    return out;
  }
  return null;
}
const ORANGE = /rgba?\(242, 140, 40\b/;

// Persistent CDP sessions for the in-page passkey prompt. A WebAuthn virtual-authenticator
// environment lives on the session, and closed-shadow buttons are found with the same session —
// so do not detach these. Only buttons inside a <gitpass-ui> shadow root match (the RP page has
// its own "Sign in" button).
const cdpSessions = new WeakMap();
async function cdpSession(page) { if (!cdpSessions.has(page)) cdpSessions.set(page, await page.context().newCDPSession(page)); return cdpSessions.get(page); }
async function shadowButton(page, re) {
  const s = await cdpSession(page);
  try {
    const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true });
    const found = [];
    const walk = (n, ours) => {
      if (n.nodeName === 'BUTTON' && ours) found.push(n);
      for (const sr of (n.shadowRoots || [])) walk(sr, ours || n.nodeName === 'GITPASS-UI');
      for (const k of (n.children || [])) walk(k, ours);
    };
    walk(root, false);
    for (const b of found) {
      const txt = (b.children || []).map((c) => c.nodeValue || '').join('');
      if (!re.test(txt)) continue;
      try { const { model } = await s.send('DOM.getBoxModel', { nodeId: b.nodeId }); const q = model.content; return { text: txt.trim(), x: (q[0] + q[4]) / 2, y: (q[1] + q[5]) / 2 }; } catch (_) { /* not rendered */ }
    }
    return null;
  } catch (_) { return null; }
}
async function shadowText(page) {
  const s = await cdpSession(page);
  try {
    const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true });
    const t = [];
    const walk = (n, ours) => {
      if (n.nodeType === 3 && ours) t.push(n.nodeValue);
      for (const sr of (n.shadowRoots || [])) walk(sr, ours || n.nodeName === 'GITPASS-UI');
      for (const k of (n.children || [])) walk(k, ours);
    };
    walk(root, false);
    return t.join(' | ');
  } catch (_) { return ''; }
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

  const server = http.createServer((req, res) => { res.setHeader('content-type', 'text/html; charset=utf-8'); res.end(req.url.startsWith('/done') ? '<h1>done</h1>' : req.url.startsWith('/2fa') ? OTP_HTML : req.url.startsWith('/multistep/password') ? MULTISTEP_PW_HTML : req.url.startsWith('/multistep') ? MULTISTEP_USER_HTML : req.url.startsWith('/signup-rules') ? SIGNUP_RULES_HTML : req.url.startsWith('/card') ? CARD_HTML : req.url.startsWith('/webauthn') ? fs.readFileSync(path.join(ROOT, 'test', 'pages', 'webauthn.html')) : LOGIN_HTML); });
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

      if (!REAL) {
        // --- Live sync: open popup refreshes on host changed/lock/unlock events (#42)
        // 1. Mock host adds an item and emits changed; open popup shows it within ~2s without reopening
        const NEW_TITLE = 'Live Alpha Item';
        const startAdd = Date.now();
        fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({
          add_item: {
            id: 'v1:live_synced',
            category: 'login',
            title: NEW_TITLE,
            subtitle: 'live@example.com',
            url: 'http://localhost',
            favorite: false,
            _u: 'live@example.com',
            _p: 'live-secret-pw',
          },
          event: 'changed',
          seq: Date.now(),
        }));
        const addedVisible = await until(async () => (await pop.text()).includes(NEW_TITLE), 2000);
        const addElapsed = Date.now() - startAdd;
        rec('open popup shows new item on host changed event within 2s', !!addedVisible, `visible=${!!addedVisible} elapsed=${addElapsed}ms`);

        // 2. An edit (title change) pushed via changed updates the visible title
        const RENAMED_TITLE = 'Live Beta Item';
        const startEdit = Date.now();
        fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({
          edit_item: {
            id: 'v1:live_synced',
            title: RENAMED_TITLE,
          },
          event: 'changed',
          seq: Date.now(),
        }));
        const editedVisible = await until(async () => {
          const txt = await pop.text();
          return txt.includes(RENAMED_TITLE) && !txt.includes(NEW_TITLE);
        }, 2000);
        const editElapsed = Date.now() - startEdit;
        rec('open popup updates item title on host changed event', !!editedVisible, `visible=${!!editedVisible} elapsed=${editElapsed}ms`);

        // 3. Mock host emits lock; the open popup switches to the locked state. unlock restores the list.
        fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({
          unlocked: false,
          seq: Date.now(),
        }));
        const lockedState = await until(async () => {
          const t = await pop.text();
          return /unlock/i.test(t) && !titles.test(t);
        }, T(4000));
        rec('open popup switches to locked state on host lock event', !!lockedState, lockedState ? 'locked view shown' : 'popup did not lock');

        fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({
          unlocked: true,
          seq: Date.now(),
        }));
        const restoredList = await until(async () => {
          const t = await pop.text();
          return titles.test(t);
        }, T(4000));
        rec('open popup restores item list on host unlock event', !!restoredList, restoredList ? 'item list restored' : 'popup did not restore');

        // --- TOTP code in the popup list (#62): the mock host returns 123456 / 20 s / 30 s.
        const totpShown = await until(async () => /123 456/.test(await pop.text()), T(5000));
        rec('popup list shows a formatted TOTP code for a login with a secret', !!totpShown,
          totpShown ? 'code 123 456 visible' : 'code not visible: ' + (await pop.text()).slice(0, 160).replace(/\s+/g, ' '));
        if (totpShown) {
          await pop.eval(`(() => {
            window.__copied = [];
            try { Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: (x) => { window.__copied.push(x); return Promise.resolve(); } } }); } catch (_) {}
            return true;
          })()`);
          const chip = await pop.click('/^123 456$/');
          const copied = await until(async () => { const c = await pop.eval('window.__copied || []'); return Array.isArray(c) && c.includes('123456') && c; }, T(3000));
          rec('popup TOTP code click copies the code', !!copied,
            `${chip ? `clicked "${chip}"` : 'code chip not found'}; copied=${JSON.stringify(copied || [])}${await pop.text().then((t) => /Copied/.test(t) ? '; toast shown' : '; no toast').catch(() => '')}`);
        }

        // --- Keyboard shortcuts (#138): ⌘⇧U copies username; ? opens and Esc closes the shortcuts sheet
        try {
          await pop.eval(`(() => {
            window.__copied = [];
            try {
              Object.defineProperty(navigator, 'clipboard', {
                configurable: true,
                value: { writeText: (x) => { window.__copied.push(x); return Promise.resolve(); } }
              });
            } catch (_) {}
            const rows = document.querySelectorAll('#listInner .row');
            if (rows.length) rows[0].click();
            return true;
          })()`);
          await sleep(300);

          await pop.eval(`(() => {
            const isMac = /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent);
            document.dispatchEvent(new KeyboardEvent('keydown', {
              key: 'u',
              metaKey: isMac,
              ctrlKey: !isMac,
              shiftKey: true,
              bubbles: true,
              cancelable: true
            }));
          })()`);

          const copiedUser = await until(async () => {
            const c = await pop.eval('window.__copied || []');
            return Array.isArray(c) && c.includes('alice@example.com') && c;
          }, T(3000));
          rec('popup shortcuts: ⌘⇧U copies selected item username', !!copiedUser, `copied=${JSON.stringify(copiedUser || [])}`);

          await pop.eval(`(() => {
            if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
            document.dispatchEvent(new KeyboardEvent('keydown', { key: '?', bubbles: true, cancelable: true }));
          })()`);

          const sheetOpened = await until(async () => {
            return await pop.eval(`(() => {
              const p = document.getElementById('panel');
              return p && !p.classList.contains('hidden') && p.classList.contains('shortcuts-panel');
            })()`);
          }, T(3000));
          rec('popup shortcuts: ? opens shortcuts sheet', !!sheetOpened, sheetOpened ? 'sheet visible' : 'sheet not opened');

          await pop.eval(`(() => {
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
          })()`);

          const sheetClosed = await until(async () => {
            return await pop.eval(`(() => {
              const p = document.getElementById('panel');
              return p && p.classList.contains('hidden');
            })()`);
          }, T(3000));
          rec('popup shortcuts: Esc closes shortcuts sheet', !!sheetClosed, sheetClosed ? 'sheet closed' : 'sheet still open');
        } catch (e) {
          rec('popup shortcuts (#138)', false, e.message);
        }

        // --- Watchtower warnings in popup (#155): warning dot in list and warning strip in detail
        try {
          const dotVisible = await until(async () => {
            return pop.eval(`(() => {
              const rows = [...document.querySelectorAll('#listInner .row')];
              const target = rows.find((r) => /Localhost Test Login/.test(r.innerText));
              return Boolean(target && target.querySelector('.warn-dot[aria-label]'));
            })()`);
          }, T(4000));
          rec('popup list shows warning dot on item with findings', !!dotVisible);

          const stripVisible = await until(async () => {
            const t = await pop.text();
            return /Weak password/.test(t) && /Reused on 3 items/.test(t) && /Generate & update/.test(t) && /Change on site/.test(t);
          }, T(4000));
          rec('popup detail shows watchtower warning strip with chips and action buttons', !!stripVisible, stripVisible ? 'chips and action buttons visible' : (await pop.text()).slice(0, 160));
        } catch (e) {
          rec('popup watchtower findings e2e', false, e.message);
        }

        // --- New item (#41): the popup's New item form sends exactly one create_item per item, with the
        // desktop New Item template's category/section/field ids and kinds.
        const panel = () => pop.eval(`(() => {
          const p = document.getElementById('panel');
          const f = p && p.querySelector('form.newitem');
          if (!f) return { open: false };
          const err = f.querySelector('.err');
          return { open: true, type: f.querySelector('select').value, err: err ? err.textContent : '',
            bad: [...f.querySelectorAll('.bad')].map((x) => x.dataset.field) };
        })()`);
        const openType = async (type) => {
          if (!(await panel()).open) await pop.click('/^New item$/');
          const ok = await until(() => pop.eval(`(() => {
            const f = document.querySelector('#panel form.newitem'); if (!f) return false;
            const sel = f.querySelector('select'); if (!sel) return false;
            if (sel.value !== ${JSON.stringify(type)}) { sel.value = ${JSON.stringify(type)}; sel.dispatchEvent(new Event('change', { bubbles: true })); return false; }
            return !!f.querySelector('[data-field=title]');
          })()`), T(5000));
          return !!ok;
        };
        const fillNew = (vals) => pop.eval(`(() => {
          const f = document.querySelector('#panel form.newitem'); if (!f) return 'no form';
          for (const [k, v] of Object.entries(${JSON.stringify(vals)})) {
            const i = f.querySelector('[data-field="' + k + '"]'); if (!i) return 'missing ' + k;
            i.value = v;
          }
          return true;
        })()`);
        const submitNew = () => pop.eval(`(() => { const f = document.querySelector('#panel form.newitem'); if (!f) return false; f.requestSubmit(); return true; })()`);
        const createdIn = (mark) => cmdsIn(mockDir).slice(mark).filter((m) => m.cmd === 'create_item');
        const fieldMap = (msg) => Object.fromEntries(((msg && msg.item.sections) || []).flatMap((s) => (s.fields || []).map((f) => [f.id, f])));
        const desc = (msg) => {
          const it = msg && msg.item;
          if (!it) return 'no create_item received';
          return `category=${it.category} sections=${(it.sections || []).map((s) => s.id).join(',')} fields=${(it.sections || []).flatMap((s) => s.fields.map((f) => `${f.id}:${f.kind}`)).join(',')}`;
        };

        // Required-field validation: an empty API credential, then a card number that fails Luhn.
        let mark = cmdsIn(mockDir).length;
        await openType('api_credential');
        await fillNew({ title: 'FAKE Incomplete Key' });
        await submitNew();
        const apiErr = await until(async () => { const s = await panel(); return s.err && s; }, T(3000));
        rec('popup new item: empty API credential shows an error and sends nothing',
          !!apiErr && apiErr.err === 'Key / secret is required' && apiErr.bad.includes('credential') && createdIn(mark).length === 0,
          apiErr ? `error="${apiErr.err}" bad=${apiErr.bad.join(',')} create_item=${createdIn(mark).length}` : 'no validation error shown');

        await openType('credit_card');
        await fillNew({ title: 'FAKE Test Card', number: '4242 4242 4242 4241' });
        await submitNew();
        const cardErr = await until(async () => { const s = await panel(); return s.err && s; }, T(3000));
        rec('popup new item: invalid card number shows an error and sends nothing',
          !!cardErr && cardErr.err === 'Card number is not valid' && cardErr.bad.includes('number') && createdIn(mark).length === 0,
          cardErr ? `error="${cardErr.err}" bad=${cardErr.bad.join(',')} create_item=${createdIn(mark).length}` : 'no validation error shown');

        // Create one item of each type through the form; each must send exactly one create_item.
        const addItem = async (type, values) => {
          const m0 = cmdsIn(mockDir).length;
          const opened = await openType(type);
          const filled = await fillNew(values);
          await submitNew();
          const msg = await until(() => createdIn(m0)[0], T(8000));
          const closed = await until(async () => !(await panel()).open, T(4000));
          return { opened, filled, msg, count: createdIn(m0).length, closed };
        };

        const api = await addItem('api_credential', { title: 'FAKE Stripe Key', credential: 'FAKE-sk_test_notreal', hostname: 'api.example.invalid' });
        const af = fieldMap(api.msg);
        rec('popup new item: API Key / Credential -> one create_item, concealed credential + url hostname',
          api.opened && api.filled === true && !!api.msg && api.count === 1 && api.closed &&
          api.msg.item.category === 'api_credential' && api.msg.item.title === 'FAKE Stripe Key' && api.msg.item.vaultId === 'v1' &&
          (api.msg.item.sections || []).length === 1 && api.msg.item.sections[0].id === 'main' &&
          af.credential && af.credential.kind === 'concealed' && af.credential.value === 'FAKE-sk_test_notreal' &&
          af.hostname && af.hostname.kind === 'url' && af.hostname.value === 'https://api.example.invalid' && !af.username,
          `${desc(api.msg)}; create_item=${api.count}; panelClosed=${!!api.closed}`);

        const card = await addItem('credit_card', { title: 'FAKE Test Card', number: '4242 4242 4242 4242', expiry: '8/2029', cvv: '123' });
        const cf = fieldMap(card.msg);
        rec('popup new item: Credit Card -> one create_item, concealed number, MM/YY expiry',
          card.opened && card.filled === true && !!card.msg && card.count === 1 && card.closed &&
          card.msg.item.category === 'credit_card' && card.msg.item.title === 'FAKE Test Card' &&
          cf.number && cf.number.kind === 'concealed' && cf.number.value === '4242424242424242' &&
          cf.cvv && cf.cvv.kind === 'concealed' && cf.expiry && cf.expiry.kind === 'month_year' && cf.expiry.value === '08/29',
          `${desc(card.msg)}; create_item=${card.count}; panelClosed=${!!card.closed}`);

        const note = await addItem('secure_note', { title: 'FAKE Recovery Codes', note: 'FAKE-code-1\nFAKE-code-2' });
        const nf = fieldMap(note.msg);
        rec('popup new item: Secure Note -> one create_item with a multiline note',
          note.opened && note.filled === true && !!note.msg && note.count === 1 && note.closed &&
          note.msg.item.category === 'secure_note' && note.msg.item.title === 'FAKE Recovery Codes' && !('notes' in note.msg.item) &&
          (note.msg.item.sections || []).length === 1 && note.msg.item.sections[0].id === 'main' &&
          nf.note && nf.note.kind === 'multiline' && nf.note.value === 'FAKE-code-1\nFAKE-code-2',
          `${desc(note.msg)}; create_item=${note.count}; panelClosed=${!!note.closed}`);
      }

      // --- Password generator (#68): open from the header, copy, and save as a new Login.
      let genStep = 'open';
      try {
        const panelClick = (label) => pop.eval(`(() => {
          const p = document.getElementById('panel'); if (!p) return null;
          const b = [...p.querySelectorAll('button')].find((x) => (x.innerText || '').trim() === ${JSON.stringify(label)});
          if (!b) return null; b.click(); return ${JSON.stringify(label)};
        })()`);
        const opened = await pop.eval('(() => { const b = document.getElementById("genBtn"); if (!b) return false; b.click(); return true; })()');
        const gen = await until(() => pop.eval(`(() => {
          const c = document.querySelector('#panel .gen-code'); if (!c) return null;
          if (!c.textContent || c.textContent === '…') return null; // still loading
          const sel = document.querySelector('#panel .gen-mode.sel');
          const meter = document.querySelector('#panel .gen-strength');
          return { text: c.textContent, mode: sel ? sel.textContent : '', strength: Number(meter ? meter.getAttribute('aria-valuenow') : 0) };
        })()`), T(4000));
        rec('popup generator: opens from the header with a host-generated password',
          !!opened && !!gen && gen.text === 'Gen-' + 'x'.repeat(16) && gen.mode === 'Random' && gen.strength > 0,
          `${opened ? 'clicked generator' : 'no generator button'}; ${JSON.stringify(gen)}`);

        await pop.eval(`(() => {
          window.__copied = [];
          try { Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: (x) => { window.__copied.push(x); return Promise.resolve(); } } }); } catch (_) {}
          return true;
        })()`);
        genStep = 'copy';
        await panelClick('Copy');
        const copiedGen = await until(async () => { const c = await pop.eval('window.__copied || []'); return Array.isArray(c) && c.includes('Gen-' + 'x'.repeat(16)) && c; }, T(3000));
        rec('popup generator: Copy copies the password', !!copiedGen, `copied=${JSON.stringify(copiedGen || [])}`);

        const m0 = cmdsIn(mockDir).length;
        genStep = 'memorable';
        await panelClick('Memorable');
        const words = await until(() => pop.eval(`(() => { const c = document.querySelector('#panel .gen-code'); const t = c ? c.textContent : ''; return t && t !== '…' ? t : ''; })()`), T(4000));
        const wordCmd = cmdsIn(mockDir).slice(m0).find((m) => m.cmd === 'generate' && m.words === true);
        rec('popup generator: Memorable asks the host for words', !!wordCmd && /^amber-coral-lunar-ivory-ocean/.test(words), `cmd=${JSON.stringify(wordCmd || null)} text=${JSON.stringify(words)}`);

        const before = cmdsIn(mockDir).filter((m) => m.cmd === 'create_item').length;
        genStep = 'save-as-login';
        await panelClick('Save as new Login…');
        const prefill = await until(() => pop.eval(`(() => {
          const f = document.querySelector('#panel form.newitem'); if (!f) return null;
          const v = (k) => { const i = f.querySelector('[data-field="' + k + '"]'); return i ? i.value : null; };
          return { type: f.querySelector('select').value, title: v('title'), url: v('url'), password: v('password') };
        })()`), T(4000));
        const after = cmdsIn(mockDir).filter((m) => m.cmd === 'create_item').length;
        rec('popup generator: Save as new Login prefills title, site URL and password',
          !!prefill && prefill.type === 'login' && prefill.title === 'localhost' && prefill.url === new URL(LOGIN).origin &&
          prefill.password === words && after === before,
          `${JSON.stringify(prefill)}; create_item delta=${after - before}`);
      } catch (e) { rec('popup generator', false, `${genStep}: ${e.message}`); }
    }

    if (!REAL) {
      // --- Inline edit (#144): change a login's username in the popup, then reopen it.
      try {
        if (!(await pop.eval(`document.getElementById('panel').classList.contains('hidden')`))) {
          await pop.eval(`(() => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); return true; })()`);
          await sleep(300);
        }
        await pop.search('GitHub'); await sleep(300);
        const mark = cmdsIn(mockDir).length;
        const picked = await pop.click('/GitHub/');
        await sleep(300);
        const openedEdit = await pop.click('/^Edit$/');
        const editForm = await until(() => pop.eval(`(() => {
          const f = document.querySelector('#detail form.inline-edit');
          if (!f) return null;
          return { title: document.getElementById('edit-title').value, username: document.getElementById('edit-username').value };
        })()`), T(4000));
        await pop.eval(`(() => {
          const u = document.getElementById('edit-username');
          if (!u) return false;
          u.value = 'octo-edited@example.com';
          u.dispatchEvent(new Event('input', { bubbles: true }));
          return true;
        })()`);
        await pop.click('/^Save$/');
        const editMsg = await until(() => cmdsIn(mockDir).slice(mark).find((m) => m.cmd === 'update_login'), T(4000));
        const editSaved = await until(async () => /Saved/.test(await pop.text()) && true, T(3000));
        rec('popup inline edit: Save sends update_login with the edited username',
          !!editForm && !!openedEdit && !!editMsg && editMsg.source === 'popup' && editMsg.item_id === 'v1:i2' && editMsg.username === 'octo-edited@example.com' && !!editSaved,
          `form=${JSON.stringify(editForm)}; edit=${JSON.stringify(editMsg && { item_id: editMsg.item_id, username: editMsg.username, source: editMsg.source })}; toast=${!!editSaved}; picked=${picked}`);

        await pop.close();
        const popEdited = await openRealPopup(ctx, sw, page, EXT);
        await until(async () => titles.test(await popEdited.text()), T(5000));
        await popEdited.search('GitHub'); await sleep(300);
        await popEdited.click('/GitHub/');
        const persisted = await until(async () => { const text = await popEdited.text(); return text.includes('octo-edited@example.com') && text; }, T(4000));
        rec('popup inline edit: edited username is visible after reopening the popup', !!persisted, persisted ? 'new username shown' : 'username not shown');
        await popEdited.close();
      } catch (e) { rec('popup inline edit (#144)', false, e.message); }
    }
    if (pop) await pop.close();

    // In-page dropdown (closed shadow DOM, pierced via CDP) first; popup "Fill" as fallback.
    const PW = 'Corr3ct-H0rse-Battery';
    const isFilled = async () => (await page.inputValue('#p')) === PW && (await page.inputValue('#u')) === 'alice@example.com';
    await page.bringToFront(); await page.click('#u'); await sleep(800);
    const hits = await until(async () => { const h = await findInShadow(page, /Localhost Test Login|alice@example\.com/); return h.length && h; }, T(5000));

    // --- Theme (#74): injected UI follows prefers-color-scheme; the orange accent is constant.
    const themeStyles = async (sel) => {
      await page.emulateMedia({ colorScheme: 'dark' });
      const dark = await shadowComputed(page, sel);
      await page.emulateMedia({ colorScheme: 'light' });
      const light = await shadowComputed(page, sel);
      await page.emulateMedia({ colorScheme: 'dark' });
      return { dark, light };
    };
    const menuTheme = hits ? await themeStyles('.menu') : { dark: null, light: null };
    rec('in-page menu follows prefers-color-scheme (dark/light)',
      !!menuTheme.dark && !!menuTheme.light &&
      menuTheme.dark['background-color'] === 'rgb(28, 28, 30)' && menuTheme.light['background-color'] === 'rgb(255, 255, 255)' &&
      menuTheme.dark.color === 'rgb(236, 237, 240)' && menuTheme.light.color === 'rgb(28, 28, 30)',
      `dark ${menuTheme.dark && menuTheme.dark['background-color']}/${menuTheme.dark && menuTheme.dark.color}; light ${menuTheme.light && menuTheme.light['background-color']}/${menuTheme.light && menuTheme.light.color}`);
    const rowTheme = hits ? await themeStyles('.it.active') : { dark: null, light: null };
    rec('active menu row keeps the orange accent in both themes',
      !!rowTheme.dark && !!rowTheme.light && ORANGE.test(rowTheme.dark['box-shadow'] || '') && ORANGE.test(rowTheme.light['box-shadow'] || ''),
      `dark ${rowTheme.dark && rowTheme.dark['background-color']}; light ${rowTheme.light && rowTheme.light['background-color']}`);
    const badgeTheme = await themeStyles('.badge');
    rec('field badge follows prefers-color-scheme and keeps the orange ring',
      !!badgeTheme.dark && !!badgeTheme.light &&
      badgeTheme.dark['background-color'] === 'rgb(28, 28, 30)' && badgeTheme.light['background-color'] === 'rgb(255, 255, 255)' &&
      ORANGE.test(badgeTheme.dark['box-shadow'] || '') && ORANGE.test(badgeTheme.light['box-shadow'] || ''),
      `dark ${badgeTheme.dark && badgeTheme.dark['background-color']}; light ${badgeTheme.light && badgeTheme.light['background-color']}`);

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
    // Keyboard regression (#74): the theme work is CSS-only; the menu still opens with Alt+ArrowDown
    // and closes with Escape, leaving the field value untouched.
    await page.focus('#u');
    await page.keyboard.down('Alt'); await page.keyboard.press('ArrowDown'); await page.keyboard.up('Alt');
    const reopened = await until(async () => !!(await shadowComputed(page, '.menu')), T(3000));
    await page.keyboard.press('Escape');
    const escaped = await until(async () => (await shadowComputed(page, '.menu')) === null, T(3000));
    rec('inline menu keyboard: Alt+ArrowDown opens, Escape closes (value untouched)',
      !!reopened && !!escaped && (await page.inputValue('#u')) === 'alice@example.com',
      `opened=${!!reopened} closed=${!!escaped} u=${JSON.stringify(await page.inputValue('#u'))}`);
    await page.fill('#u', ''); await page.fill('#p', '');

    // --- Issue #80: "Search gitpass…" in inline menu with cross-site confirm & add-site prompt
    let inlineSearchOk = false, inlineSearchDetail = '';
    try {
      // 1. Re-open menu with Alt+ArrowDown
      await page.focus('#u');
      await page.keyboard.down('Alt'); await page.keyboard.press('ArrowDown'); await page.keyboard.up('Alt');
      await until(async () => !!(await shadowComputed(page, '.menu')), T(3000));

      // 2. Click "Search gitpass…" row
      const searchRowHits = await until(async () => {
        const h = await findInShadow(page, /Search gitpass/i);
        const btn = h.find((x) => x.tag === 'BUTTON');
        return btn && [btn];
      }, T(3000));
      if (!searchRowHits) throw new Error('Search gitpass row not found in menu');
      await page.mouse.click(searchRowHits[0].x, searchRowHits[0].y);

      // 3. Header transforms into search input and receives focus
      const searchInputHits = await until(async () => {
        const h = await findInShadow(page, /Search gitpass/i);
        const inp = h.find((x) => x.tag === 'INPUT');
        return inp && [inp];
      }, T(3000));
      if (!searchInputHits) throw new Error('search input not rendered');
      await page.mouse.click(searchInputHits[0].x, searchInputHits[0].y);

      // 4. Type non-matching login name (GitHub)
      await page.keyboard.type('github');
      const ghResult = await until(async () => {
        const h = await findInShadow(page, /^GitHub$/);
        return h.length && h;
      }, T(3000));
      if (!ghResult) throw new Error('GitHub not found in search results');

      // 5. Escape returns to suggestions; second Escape closes menu (value untouched)
      await page.keyboard.press('Escape');
      const backToSug = await until(async () => (await shadowComputed(page, '.search-input')) === null && !!(await shadowComputed(page, '.menu')), T(3000));
      await page.keyboard.press('Escape');
      const menuClosed = await until(async () => (await shadowComputed(page, '.menu')) === null, T(3000));
      if (!backToSug || !menuClosed || (await page.inputValue('#u')) !== '') {
        throw new Error(`Escape navigation failed: backToSug=${!!backToSug} menuClosed=${!!menuClosed} u=${await page.inputValue('#u')}`);
      }

      // 6. Open search again to test cross-site confirm and fill
      await page.focus('#u');
      await page.keyboard.down('Alt'); await page.keyboard.press('ArrowDown'); await page.keyboard.up('Alt');
      await until(async () => !!(await shadowComputed(page, '.menu')), T(3000));
      const searchRowHits2 = await until(async () => {
        const h = await findInShadow(page, /Search gitpass/i);
        const btn = h.find((x) => x.tag === 'BUTTON');
        return btn && [btn];
      }, T(3000));
      if (!searchRowHits2) throw new Error('Search gitpass row not found in menu on re-open');
      await page.mouse.click(searchRowHits2[0].x, searchRowHits2[0].y);
      const searchInputHits2 = await until(async () => {
        const h = await findInShadow(page, /Search gitpass/i);
        const inp = h.find((x) => x.tag === 'INPUT');
        return inp && [inp];
      }, T(3000));
      if (!searchInputHits2) throw new Error('search input 2 not rendered');
      await page.mouse.click(searchInputHits2[0].x, searchInputHits2[0].y);
      await page.keyboard.type('github');
      const ghResult2 = await until(async () => {
        const h = await findInShadow(page, /^GitHub$/);
        return h.length && h;
      }, T(3000));

      // 7. Verify no secrets in DOM before fill
      const shadowBefore = (await findInShadow(page, /.*/)).map((h) => h.text).join(' ');
      if (shadowBefore.includes('gh-pass-123') || shadowBefore.includes('Corr3ct-H0rse')) {
        throw new Error('Secret password found in shadow DOM before fill');
      }

      // 8. Click GitHub -> Extension-owned cross-site confirm popup window opens
      await page.mouse.click(ghResult2[0].x, ghResult2[0].y);
      const confirmWin = await until(() => ctx.pages().find((p) => p.url().includes('popup/confirm.html')), T(4000));
      if (!confirmWin) throw new Error('cross-site confirm popup window not opened');

      const confirmMsg = await until(async () => {
        const t = await confirmWin.locator('#msg').textContent().catch(() => '');
        return /Fill on localhost\? This login is for github\.com/i.test(t) && t;
      }, T(3000));
      if (!confirmMsg) throw new Error('cross-site confirm window message not matching');

      // 9. Cancel with Escape in confirm window leaves inputs untouched and returns to search
      await confirmWin.keyboard.press('Escape');
      await until(() => !ctx.pages().some((p) => p.url().includes('popup/confirm.html')), T(3000));

      let cancelU = '', cancelP = '', cancelSearchInp = false;
      const cancelledOk = await until(async () => {
        cancelU = await page.inputValue('#u');
        cancelP = await page.inputValue('#p');
        cancelSearchInp = !!(await shadowComputed(page, '.search-input'));
        return cancelU === '' && cancelP === '' && cancelSearchInp;
      }, T(3000));
      if (!cancelledOk) throw new Error(`Escape cancel failed: u=${cancelU} p=${cancelP} searchInp=${cancelSearchInp}`);

      // Also verify clicking Cancel button in confirm window
      const ghResultRetry = await until(async () => {
        const h = await findInShadow(page, /^GitHub$/);
        return h.length && h;
      }, T(3000));
      await page.mouse.click(ghResultRetry[0].x, ghResultRetry[0].y);
      const confirmWinRetry = await until(() => ctx.pages().find((p) => p.url().includes('popup/confirm.html')), T(4000));
      if (!confirmWinRetry) throw new Error('confirm window retry not opened');
      await confirmWinRetry.click('#cancelBtn');
      await until(() => !ctx.pages().some((p) => p.url().includes('popup/confirm.html')), T(3000));

      // 10. Click GitHub again and confirm fill via "Fill" button in confirm window
      const ghResult3 = await until(async () => {
        const h = await findInShadow(page, /^GitHub$/);
        return h.length && h;
      }, T(3000));
      await page.mouse.click(ghResult3[0].x, ghResult3[0].y);
      const confirmWinFill = await until(() => ctx.pages().find((p) => p.url().includes('popup/confirm.html')), T(4000));
      if (!confirmWinFill) throw new Error('confirm window fill not opened');
      await confirmWinFill.click('#fillBtn');
      await until(() => !ctx.pages().some((p) => p.url().includes('popup/confirm.html')), T(3000));

      // 11. Credentials filled (octo-edited@example.com / gh-pass-123)
      const fillFilled = await until(async () => ['alice', 'octo-edited@example.com'].includes(await page.inputValue('#u')) && (await page.inputValue('#p')) === 'gh-pass-123', T(3000));
      if (!fillFilled) throw new Error(`Fill failed: u=${await page.inputValue('#u')}`);

      // 12. Post-fill prompt: "Also add this site to the item"
      const addSitePrompt = await until(async () => {
        const h = await findInShadow(page, /Also add this site to the item/i);
        return h.length && h;
      }, T(3000));
      if (!addSitePrompt) throw new Error('Add site prompt not shown');

      const markAddUrl = cmdsIn(mockDir).length;
      await page.mouse.click(addSitePrompt[0].x, addSitePrompt[0].y);
      const siteAddedNotice = await until(async () => {
        const h = await findInShadow(page, /Site added/i);
        return h.length && h;
      }, T(3000));
      if (!siteAddedNotice) throw new Error('Site added notice not shown');

      const addUrlCmd = cmdsIn(mockDir).slice(markAddUrl).find((m) => m.cmd === 'add_url');
      if (!addUrlCmd && !REAL) throw new Error('add_url command not sent to host');

      inlineSearchOk = true;
      inlineSearchDetail = `u=${await page.inputValue('#u')} p=[set]; cross-site confirm verified; add_url sent`;
    } catch (e) {
      inlineSearchDetail = e.message;
    }
    rec('inline search: search gitpass, cross-site confirm, fill, Escape cancel, add-site prompt', inlineSearchOk, inlineSearchDetail);
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

    // --- Empty popup state (#52): a site with no saved login offers "Save a login for <host>".
    // The mock host hides localhost matches so the Suggested filter is empty (the popup falls back to All Items).
    try {
      fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({ unlocked: true, no_match_hosts: ['localhost'], seq: Date.now() }));
      await sleep(300); // the mock host polls control.json every 100 ms
      const emptyPop = await openRealPopup(ctx, sw, page, EXT);
      const loaded = await until(async () => /Localhost Test Login/.test(await emptyPop.text()), T(8000));
      await emptyPop.eval(`(() => { const s = document.getElementById('filter'); s.value = 'suggested'; s.dispatchEvent(new Event('change', { bubbles: true })); return s.value; })()`);
      const emptyTxt = await until(async () => { const x = await emptyPop.text(); return /No saved logins for this site/.test(x) && x; }, T(5000));
      rec('popup empty state: no saved logins offers Save a login for localhost',
        !!loaded && !!emptyTxt && /Save a login for localhost/.test(emptyTxt),
        emptyTxt ? 'empty state shown' : (await emptyPop.text().catch(() => 'ERR')).slice(0, 160).replace(/\s+/g, ' '));
      const mark = cmdsIn(mockDir).length;
      const clicked = await emptyPop.click('/^Save a login for localhost$/');
      const form = await until(() => emptyPop.eval(`(() => {
        const sel = document.querySelector('#panel select');
        const title = document.querySelector('#panel [data-field=title]');
        const url = document.querySelector('#panel [data-field=url]');
        return sel && title && url ? { type: sel.value, title: title.value, url: url.value } : null;
      })()`), T(5000));
      rec('popup empty state: Save a login opens the Login form with the URL prefilled',
        !!form && form.type === 'login' && form.title === 'localhost' && form.url === `http://localhost:${server.address().port}`,
        `${clicked ? `clicked "${clicked}"` : 'button not clicked'}; ${JSON.stringify(form)}`);
      const created = cmdsIn(mockDir).slice(mark).filter((m) => m.cmd === 'create_item');
      rec('popup empty state: nothing is saved until the user submits', created.length === 0, `create_item count=${created.length}`);
      await emptyPop.close();
    } catch (e) { rec('popup empty state: no saved logins', false, e.message); } finally {
      fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({ unlocked: true, seq: Date.now() }));
      await sleep(300);
    }

    // --- Save prompt after submit
    await page.goto(LOGIN); await sleep(1000);
    await page.fill('#u', 'newuser@example.com'); await page.fill('#p', 'Brand-New-Pass-42'); await page.click('#go');
    const banner = await until(async () => { const h = await findInShadow(page, /^\s*(Save|บันทึก|သိမ်းရန်)\s*$/i)  /* the button, not the 'Save login to gitpass?' title */; return h.length && h; }, T(6000));
    const bannerTheme = banner ? await themeStyles('.banner') : { dark: null, light: null };
    rec('save banner follows prefers-color-scheme (dark/light)',
      !!bannerTheme.dark && !!bannerTheme.light &&
      bannerTheme.dark['background-color'] === 'rgb(28, 28, 30)' && bannerTheme.light['background-color'] === 'rgb(255, 255, 255)' &&
      bannerTheme.dark.color === 'rgb(236, 237, 240)' && bannerTheme.light.color === 'rgb(28, 28, 30)',
      `dark ${bannerTheme.dark && bannerTheme.dark['background-color']}/${bannerTheme.dark && bannerTheme.dark.color}; light ${bannerTheme.light && bannerTheme.light['background-color']}/${bannerTheme.light && bannerTheme.light.color}`);
    const primaryTheme = banner ? await themeStyles('.btn.pri') : { dark: null, light: null };
    rec('save banner primary button keeps the orange accent in both themes',
      !!primaryTheme.dark && !!primaryTheme.light &&
      [primaryTheme.dark, primaryTheme.light].every((s) => s['background-color'] === 'rgb(242, 140, 40)' && s.color === 'rgb(28, 28, 30)'),
      `dark ${primaryTheme.dark && primaryTheme.dark['background-color']}; light ${primaryTheme.light && primaryTheme.light['background-color']}`);
    if (banner) { await page.mouse.click(banner[0].x, banner[0].y); }
    const saved = await until(() => cmdsIn(mockDir).find((m) => m.cmd === 'save_login' && m.username === 'newuser@example.com'), T(4000));
    if (REAL) rec('save-login prompt after submit (real host)', !!banner, banner ? `banner button "${banner[0].text}" clicked` : 'no Save button found', 'test'); else
    rec('save-login prompt after submit -> save_login sent', !!saved, banner ? `banner button "${banner[0].text}"; save_login ${saved ? 'received' : 'NOT received'}` : 'no Save button found in page shadow DOM');

    // --- Save banner (#148): editable title + vault picker, success state, Esc dismiss.
    if (!REAL) {
      await page.goto(LOGIN); await sleep(1000);
      await page.fill('#u', 'vaultuser@example.com'); await page.fill('#p', 'Vault-Pass-42'); await page.click('#go');
      const fieldsReady = await until(async () => (await shadowNode(page, '.banner input.inp')) != null && (await shadowNode(page, '.banner select')) != null, T(6000));
      rec('save banner: title field and vault picker appear', !!fieldsReady, fieldsReady ? 'title + select' : 'not found');
      const focusedUi = await page.evaluate(() => document.activeElement && document.activeElement.tagName);
      rec('save banner: focus moves into the banner', focusedUi === 'GITPASS-UI', `activeElement=${focusedUi}`);
      const typed = await shadowFocus(page, '.banner input.inp');
      if (typed) { await page.keyboard.press(process.platform === 'darwin' ? 'Meta+A' : 'Control+A'); await page.keyboard.type('Work Custom Title'); }
      const vaultSet = await shadowSetValue(page, '.banner select', 'v2');
      const mark148 = cmdsIn(mockDir).length;
      const saveBtn = await until(async () => { const h = await findInShadow(page, /^\s*Save\s*$/); return h.length && h; }, T(4000));
      if (saveBtn) await page.mouse.click(saveBtn[0].x, saveBtn[0].y);
      const custom = await until(() => cmdsIn(mockDir).slice(mark148).find((m) => m.cmd === 'save_login' && m.username === 'vaultuser@example.com'), T(5000));
      rec('save banner: custom title and non-default vault reach save_login',
        !!(typed && vaultSet && custom && custom.title === 'Work Custom Title' && custom.vault_id === 'v2'),
        custom ? `title=${JSON.stringify(custom.title)} vault=${custom.vault_id}` : 'save_login NOT received');
      const success = await until(async () => { const h = await findInShadow(page, /Open in gitpass/); return h.length && h; }, T(4000));
      rec('save banner: success state offers Open in gitpass', !!success, success ? 'shown' : 'not shown');
      await page.mouse.move(4, 4); // stop hovering the banner; programmatic focus must not pin it open
      const autoClosed = await until(async () => !(await findInShadow(page, /Open in gitpass/)).length, T(8000), 300);
      rec('save banner: success auto-closes after 4 s', !!autoClosed, autoClosed ? 'closed' : 'still open');

      // Esc = Not now on a fresh banner; nothing is saved.
      await page.goto(LOGIN); await sleep(1000);
      await page.fill('#u', 'escuser@example.com'); await page.fill('#p', 'Esc-Pass-42'); await page.click('#go');
      const escBanner = await until(async () => { const h = await findInShadow(page, /^\s*Save\s*$/); return h.length && h; }, T(6000));
      const escMark = cmdsIn(mockDir).length;
      if (escBanner) { await shadowFocus(page, '.banner input.inp'); await page.keyboard.press('Escape'); }
      const escClosed = await until(async () => { const h = await findInShadow(page, /^\s*Save\s*$/); return !h.length; }, T(3000));
      const after = cmdsIn(mockDir).slice(escMark);
      const escSaved = after.some((m) => m.cmd === 'save_login' && m.username === 'escuser@example.com');
      rec('save banner: Esc dismisses without saving', !!(escBanner && escClosed && !escSaved),
        `banner=${!!escBanner} closed=${!!escClosed} saved=${!!escSaved}`);
    }

    // --- Multi-step login (username page -> password page). The step-2 form has no username input, so
    // save_login can only carry both values if the step-1 username survives the page load in the background.
    // Each step must show exactly one icon (the anchor badge) for its single input field.
    const MS_USER = 'multistep@example.com', MS_PASS = 'Two-St3p-Login!';
    const badgeHosts = (pg) => pg.evaluate(() => document.querySelectorAll('gitpass-ui').length);
    const decorated = (pg, sel) => pg.evaluate((s) => { const el = document.querySelector(s); return !!el && el.dataset.gitpassDecorated === 'true'; }, sel);
    const ms = await ctx.newPage();
    const MS1 = LOGIN.replace(/\/login$/, '/multistep');
    await ms.goto(MS1); await sleep(1000);
    const step1Badge = await until(async () => (await badgeHosts(ms)) === 1 && await decorated(ms, '#u'), T(5000));
    rec('multi-step step 1: exactly one gitpass icon on the username field', !!step1Badge, `icons=${await badgeHosts(ms)}`);
    await ms.fill('#u', MS_USER);
    await Promise.all([ms.waitForURL(/\/multistep\/password$/, { timeout: T(8000) }), ms.click('#next')]);
    await sleep(1000);
    const step2Badge = await until(async () => (await badgeHosts(ms)) === 1 && await decorated(ms, '#p'), T(5000));
    rec('multi-step step 2: exactly one gitpass icon on the password field', !!step2Badge, `icons=${await badgeHosts(ms)}`);
    await ms.fill('#p', MS_PASS); await ms.click('#go');
    const msBanner = await until(async () => { const h = await findInShadow(ms, /multistep@example\.com/); return h.length && h; }, T(6000));
    rec('multi-step login: banner shows the step-1 username before saving', !!msBanner, msBanner ? `"${msBanner[0].text}"` : 'no username in the save banner');
    const msSave = await until(async () => { const h = await findInShadow(ms, /^\s*(Save|บันทึก|သိမ်းရန်)\s*$/i); return h.length && h; }, T(6000));
    if (msSave) await ms.mouse.click(msSave[0].x, msSave[0].y);
    const msSavedAll = await until(() => { const l = cmdsIn(mockDir).filter((m) => m.cmd === 'save_login' && m.username === MS_USER); return l.length && l; }, T(5000)) || [];
    const msSaved = msSavedAll[0];
    rec('multi-step login saves username + password from two pages',
      REAL ? !!(msBanner && msSave) : !!(msSaved && msSaved.password === MS_PASS && msSaved.origin === `http://localhost:${server.address().port}`),
      REAL ? 'real host (temp vault; not logged)' : msSaved
        ? `save_login username=${msSaved.username} password=${msSaved.password === MS_PASS ? 'matches' : 'MISMATCH'} origin=${msSaved.origin}`
        : 'save_login NOT received');
    rec('multi-step login saves exactly once', REAL || msSavedAll.length === 1, REAL ? 'real host (not logged)' : `save_login count=${msSavedAll.length}`);
    await ms.close();

    // --- Site password rules (#188): maxlength + passwordrules shape the suggested password
    try {
      const rulesPage = await ctx.newPage();
      await rulesPage.goto(LOGIN.replace(/\/login$/, '/signup-rules')); await sleep(1000); await rulesPage.bringToFront();
      await rulesPage.click('input[name=username]'); await sleep(800); // sign-up forms anchor the menu on the username field
      const sug = await until(async () => { const x = await findInShadow(rulesPage, /Suggest strong password/); return x.length && x; }, T(5000));
      const sub = sug ? await findInShadow(rulesPage, /characters|gitpass will save/) : [];
      if (sug) await rulesPage.mouse.click(sug[0].x, sug[0].y);
      const gen = await until(async () => (await rulesPage.inputValue('#np')) || false, T(3000));
      const pw = gen || '';
      const sent = REAL ? null : cmdsIn(mockDir).filter((m) => m.cmd === 'generate' && m.rules).pop();
      // Never print the password itself: report which rules held.
      rec('signup with maxlength + passwordrules gets a compliant suggested password',
        pw.length === 16 && /^[A-Za-z0-9_-]+$/.test(pw) && /[a-z]/.test(pw) && /[A-Z]/.test(pw) && /[0-9]/.test(pw),
        `row=${!!sug}; subtitle=${sub.length ? JSON.stringify(sub[0].text) : 'missing'}; length=${pw.length}; charset ok=${/^[A-Za-z0-9_-]*$/.test(pw)}; host rules=${sent ? sent.rules.classes.length + ' classes, len ' + sent.len : '(real host)'}`);
      await rulesPage.close();
    } catch (e) { rec('signup password rules (#188)', false, e.message); }

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
    // Card form: one gitpass icon per logical group, on the card number input — never one per card
    // field. /card also holds a second card form and the identity form used below, so the page-wide
    // icon count is 3; these checks scope the count to the #pay card form.
    await cardPage.goto(CARD); await cardPage.bringToFront();
    const readCardIcons = () => cardPage.evaluate(() => {
      const hosts = [...document.querySelectorAll('gitpass-ui')];
      const over = (el) => {
        const r = el.getBoundingClientRect();
        const left = r.right + window.scrollX - 28;
        const top = r.top + window.scrollY + (r.height - 22) / 2;
        return hosts.filter((h) => Math.abs(parseFloat(h.style.left) - left) < 1 && Math.abs(parseFloat(h.style.top) - top) < 1);
      };
      const fields = [...document.querySelectorAll('#pay input, #pay select')].map((el) => ({ id: el.id, decorated: el.dataset.gitpassDecorated === 'true', icons: over(el).length }));
      const numHost = over(document.getElementById('num'))[0];
      const box = numHost && numHost.getBoundingClientRect();
      return { pageIcons: hosts.length, fields, badge: box && box.width && box.height ? { x: box.left + box.width / 2, y: box.top + box.height / 2 } : null };
    });
    const cardIcons = await until(async () => {
      const r = await readCardIcons();
      const n = r.fields.find((f) => f.id === 'num');
      return r.badge && n && n.decorated && n.icons === 1 ? r : null;
    }, T(5000)) || await readCardIcons();
    const cardGroupIcons = cardIcons.fields.reduce((n, f) => n + f.icons, 0);
    const numField = cardIcons.fields.find((f) => f.id === 'num');
    rec('card form: exactly one gitpass icon, on the card number field',
      cardGroupIcons === 1 && !!numField && numField.decorated && numField.icons === 1,
      `card form icons=${cardGroupIcons} on=${cardIcons.fields.filter((f) => f.icons).map((f) => f.id).join(',') || 'none'}; page icons=${cardIcons.pageIcons} (2 card groups + identity)`);
    rec('card form: name/expiry/CVC are not separately decorated',
      ['name', 'mm', 'yy', 'cvc'].every((id) => { const f = cardIcons.fields.find((x) => x.id === id); return !!f && !f.decorated && f.icons === 0; }),
      cardIcons.fields.filter((f) => f.id !== 'num').map((f) => `${f.id}:${f.decorated || f.icons ? 'decorated' : 'no badge'}`).join(' '));
    // The icon itself (not the field) must open the dropdown, and picking a card fills every card field.
    if (cardIcons.badge) await cardPage.mouse.click(cardIcons.badge.x, cardIcons.badge.y);
    const iconRow = await until(async () => { const h = await findInShadow(cardPage, /^Visa$/); return h.length && h; }, T(5000));
    if (iconRow) await cardPage.mouse.click(iconRow[0].x, iconRow[0].y);
    const iconFilled = await until(async () => same(await vals(CARD_IDS), CARD_WANT), T(3000));
    rec('card number icon fills all card fields', !!iconFilled,
      `${iconRow ? `clicked "${iconRow[0].text}"` : (cardIcons.badge ? 'no dropdown entry after icon click' : 'no icon found on #num')}; ${diff(await vals(CARD_IDS), CARD_WANT, CARD_IDS)}`);
    rec('card fill via in-page dropdown', ...await inpageCF('#num', /^Visa$/, CARD_IDS, CARD_WANT, 'fill_card'));
    rec('identity fill via in-page dropdown', ...await inpageCF('#fn', /^Home Identity$/, ID_IDS, ID_WANT, 'fill_identity'));
    rec('card fill via popup Fill', ...await viaPop('Visa', CARD_IDS, CARD_WANT, 'fill_card'));
    rec('identity fill via popup Fill', ...await viaPop('Home Identity', ID_IDS, ID_WANT, 'fill_identity'));
    await cardPage.close();

    // --- Passkeys: the in-page prompt (closed shadow DOM) approves create/get; cancel (create or get)
    // or a get with no gitpass match falls back to the browser's own authenticator (CDP virtual
    // authenticator) and must not open a gitpass window.
    {
      const WA = `http://localhost:${server.address().port}/webauthn`;
      const outJson = async (pg) => { try { return JSON.parse((await pg.locator('#out').textContent()) || 'null'); } catch { return null; } };
      const clearOut = (pg) => pg.evaluate(() => { document.getElementById('out').textContent = ''; });
      const passkeyWindows = () => ctx.pages().filter((p) => p.url().startsWith(`${EXT}/popup/passkey.html`)).length;
      const waitPrompt = (pg, re) => until(() => shadowButton(pg, re), T(8000));
      const clickShadow = async (pg, re) => { const b = await waitPrompt(pg, re); if (!b) return false; await pg.bringToFront(); await pg.mouse.click(b.x, b.y); return true; };
      const promptGone = (pg, re) => until(async () => !(await shadowButton(pg, re)), T(6000));
      const clickContinue = async (pg) => { // native fallback can need a fresh user gesture after cancel
        const btn = await until(async () => { const l = pg.getByRole('button', { name: 'Continue' }); return (await l.count()) ? l.first() : null; }, T(2000));
        if (btn) await btn.click().catch(() => {});
        return !!btn;
      };
      const noNewWindow = (pages) => passkeyWindows() === 0 && ctx.pages().length === pages;
      let pkTheme = null;
      const act = async (pg, sel, promptRe, native) => {
        await clearOut(pg); await pg.bringToFront(); await pg.click(sel);
        if (promptRe) {
          if (!(await waitPrompt(pg, promptRe))) return { error: 'no in-page prompt' };
          if (!pkTheme) { // #74: the passkey card follows the system theme too
            await pg.emulateMedia({ colorScheme: 'dark' });
            const dark = await shadowComputed(pg, '.pk-card');
            await pg.emulateMedia({ colorScheme: 'light' });
            const light = await shadowComputed(pg, '.pk-card');
            await pg.emulateMedia({ colorScheme: 'dark' });
            pkTheme = { dark, light };
          }
          await sleep(800); // the primary button ignores trusted clicks for its first 600 ms
          if (native) await clickShadow(pg, /^Cancel$/); else await clickShadow(pg, promptRe);
        } else { // no prompt expected: gitpass has no matching credential
          const unexpected = await until(() => shadowButton(pg, /^Sign in$/), T(1200));
          if (unexpected) return { error: 'unexpected in-page prompt', text: unexpected.text };
        }
        if (native) await clickContinue(pg);
        const r = await until(() => outJson(pg), T(12000));
        return r || { error: 'no result' };
      };

      // 1. Cancel (create) and no matching passkey (get) hand the request to the browser.
      const nativePage = await ctx.newPage(); await nativePage.goto(WA); await sleep(500); await nativePage.bringToFront();
      const cdp = await cdpSession(nativePage);
      await cdp.send('WebAuthn.enable', { enableUI: false });
      const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', { options: { protocol: 'ctap2', transport: 'internal', hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true } });
      let pages = ctx.pages().length;
      let mark = cmdsIn(mockDir).length;
      const cancelReg = await act(nativePage, '#reg', /^Save passkey$/, true);
      const cancelCmds = cmdsIn(mockDir).slice(mark).filter((m) => /^passkey_/.test(m.cmd));
      rec('passkey overlay follows prefers-color-scheme (dark/light)',
        !!pkTheme && !!pkTheme.dark && !!pkTheme.light &&
        pkTheme.dark['background-color'] === 'rgb(28, 28, 30)' && pkTheme.light['background-color'] === 'rgb(255, 255, 255)' &&
        pkTheme.dark.color === 'rgb(236, 237, 240)' && pkTheme.light.color === 'rgb(28, 28, 30)',
        `dark ${pkTheme && pkTheme.dark && pkTheme.dark['background-color']}/${pkTheme && pkTheme.dark && pkTheme.dark.color}; light ${pkTheme && pkTheme.light && pkTheme.light['background-color']}/${pkTheme && pkTheme.light && pkTheme.light.color}`);
      rec('passkey create cancelled -> browser passkey, no gitpass window',
        cancelReg.registered === true && noNewWindow(pages) && (REAL || !cancelCmds.some((m) => m.cmd === 'passkey_create')),
        JSON.stringify(cancelReg) + (REAL ? ' (real host; not logged)' : `; cmds=${JSON.stringify(cancelCmds.map((m) => m.cmd))}`));
      pages = ctx.pages().length; mark = cmdsIn(mockDir).length;
      const noMatch = await act(nativePage, '#login', null, true);
      const noMatchCmds = cmdsIn(mockDir).slice(mark).filter((m) => /^passkey_/.test(m.cmd));
      rec('passkey get with no gitpass match -> native fallback verified',
        noMatch.verified === true && noNewWindow(pages) && (REAL || !noMatchCmds.some((m) => m.cmd === 'passkey_get')),
        JSON.stringify(noMatch) + (REAL ? ' (real host; not logged)' : `; cmds=${JSON.stringify(noMatchCmds.map((m) => m.cmd))}`));

      // Cancel an in-page get prompt that gitpass did match: the same request must go to the browser.
      // Seed a gitpass passkey through the in-page create flow, then give the CDP virtual authenticator
      // the same key so the native fallback can actually sign (headless has no platform authenticator).
      if (REAL) {
        rec('passkey get cancelled in-page -> native fallback', false, 'skipped in real-host mode: needs a CDP virtual authenticator seeded with the gitpass key', 'info');
      } else {
        const seeded = await act(nativePage, '#reg', /^Save passkey$/, false);
        await promptGone(nativePage, /^Save passkey$/);
        let cancelGet = { error: 'seed create failed: ' + JSON.stringify(seeded) };
        let cancelGetCmds = [];
        if (seeded.registered === true) {
          try {
            const pks = JSON.parse(fs.readFileSync(path.join(mockDir, 'passkeys.json'), 'utf8'));
            const pk = pks.find((p) => p.rp_id === 'localhost');
            if (!pk) throw new Error('no localhost passkey in the mock store');
            const b64 = (s) => Buffer.from(s, 'base64url').toString('base64');
            await cdp.send('WebAuthn.addCredential', { authenticatorId, credential: { credentialId: b64(pk.credential_id), isResidentCredential: true, rpId: pk.rp_id, privateKey: b64(pk.pkcs8), userHandle: b64(pk.user_handle), signCount: 1 } });
            pages = ctx.pages().length; mark = cmdsIn(mockDir).length;
            cancelGet = await act(nativePage, '#login', /^Sign in$/, true);
            cancelGetCmds = cmdsIn(mockDir).slice(mark).filter((m) => /^passkey_/.test(m.cmd));
          } catch (e) { cancelGet = { error: 'native seed failed: ' + e.message }; }
        }
        rec('passkey get cancelled in-page -> native authenticator, no gitpass window',
          cancelGet.verified === true && noNewWindow(pages) && !cancelGetCmds.some((m) => m.cmd === 'passkey_get') && cancelGetCmds.some((m) => m.cmd === 'passkey_query'),
          JSON.stringify(cancelGet) + (cancelGetCmds.length ? `; cmds=${JSON.stringify(cancelGetCmds.map((m) => m.cmd))}` : ''));
      }
      await nativePage.close();

      // 2. In-page prompt approval: create, get (ES256 verified by the RP), signCount, duplicates.
      const wa = await ctx.newPage(); await wa.goto(WA); await sleep(500); await wa.bringToFront();
      pages = ctx.pages().length; const before = cmdsIn(mockDir).length;
      const save = await act(wa, '#reg', /^Save passkey$/, false);
      rec('passkey create: in-page approve -> credential verified by RP page', save.registered === true, JSON.stringify(save));
      const pkCmds = cmdsIn(mockDir).slice(before).filter((m) => /^passkey_/.test(m.cmd));
      rec('passkey_create sent with browser origin + in-page approval',
        REAL || pkCmds.some((m) => m.cmd === 'passkey_create' && m.origin === new URL(WA).origin && m.source === 'popup' && m.approved === true),
        REAL ? 'real host (not logged)' : JSON.stringify(pkCmds.map((m) => ({ cmd: m.cmd, origin: m.origin, source: m.source, approved: m.approved }))));
      await promptGone(wa, /^Save passkey$/);

      const g1 = await act(wa, '#login', /^Sign in$/, false);
      rec('passkey get: ES256 signature verified by RP page (signCount 1)', g1.verified === true && g1.signCount === 1 && g1.userHandle === 'user-42', JSON.stringify(g1));
      await promptGone(wa, /^Sign in$/);
      const g2 = await act(wa, '#login', /^Sign in$/, false);
      rec('passkey get again: signCount increments', g2.verified === true && g2.signCount === 2, JSON.stringify(g2));
      await promptGone(wa, /^Sign in$/);

      // Duplicate: the prompt reports "already saved"; Close resolves the page as InvalidStateError.
      await clearOut(wa); await wa.click('#reg');
      if (await waitPrompt(wa, /^Save passkey$/)) { await sleep(800); await clickShadow(wa, /^Save passkey$/); }
      const closeB = await until(() => shadowButton(wa, /^Close$/), T(8000));
      rec('duplicate create shows the error with Close', !!closeB, closeB ? 'Close shown' : await shadowText(wa));
      if (closeB) await clickShadow(wa, /^Close$/);
      const dup = await until(() => outJson(wa), T(8000));
      rec('passkey create with excludeCredentials match -> InvalidStateError', dup && dup.error === 'InvalidStateError', JSON.stringify(dup));
      rec('passkey flows never opened a gitpass window', noNewWindow(pages), `pages=${ctx.pages().length}, passkey windows=${passkeyWindows()}`);
      await wa.close();
    }

    // --- ⌘⇧L / Ctrl+Shift+L "fill-login" command (#124). Playwright can't press extension shortcuts, so the
    // test calls the same handler chrome.commands.onCommand runs, from the service worker.
    if (!REAL) {
      const ctrl = (o) => fs.writeFileSync(path.join(mockDir, 'control.json'), JSON.stringify({ ...o, seq: Date.now() }));
      const runCmd = () => sw.evaluate(() => globalThis.gitpassFillLoginCommand()).catch((e) => ({ error: e.message }));
      const vals = () => page.evaluate(() => [document.getElementById('u').value, document.getElementById('p').value]);
      ctrl({ unlocked: true, only_match_ids: ['v1:i1'] }); await sleep(400);
      await page.goto(LOGIN); await page.bringToFront(); await sleep(800);
      const one = await runCmd();
      const filled = await until(async () => { const v = await vals(); return v[0] === 'alice@example.com' && v[1] === 'Corr3ct-H0rse-Battery' && v; }, T(4000));
      rec('fill-login command: one match fills username + password', one && one.action === 'fill' && !!filled, `${JSON.stringify(one)} values=${JSON.stringify(await vals())}`);

      // One-time code on the next page (#181): the login just filled has a code, so /2fa on the same site gets
      // the chip; the same page on another host (127.0.0.1 vs localhost) never does.
      const OTP = LOGIN.replace(/\/login$/, '/2fa');
      await page.goto(OTP); await page.bringToFront();
      const chip = await until(async () => /Fill one-time code/.test(await shadowText(page)), T(5000));
      rec('one-time code chip is offered on the next page after a login fill', !!chip && (await page.inputValue('#otp')) === '', chip ? 'chip shown, field empty' : 'no chip: ' + (await shadowText(page)).slice(0, 120));
      if (chip) { const h = await findInShadow(page, /Fill one-time code/); if (h.length) await page.mouse.click(h[0].x, h[0].y); }
      const code = await until(async () => (await page.inputValue('#otp')) === '123456', T(4000));
      rec('clicking the chip fills the one-time code', !!code, `otp=${JSON.stringify(await page.inputValue('#otp'))}`);
      const other = await ctx.newPage();
      await other.goto(OTP.replace('localhost', '127.0.0.1')); await other.bringToFront(); await sleep(1200);
      rec('one-time code chip is not offered on a different site', !/Fill one-time code/.test(await shadowText(other)), 'checked 127.0.0.1');
      await other.close(); await page.bringToFront();

      ctrl({ unlocked: true, only_match_ids: ['v1:i1', 'v1:cmd2'], add_item: { id: 'v1:cmd2', category: 'login', title: 'Second Command Login', subtitle: 'bob@example.com', url: 'http://localhost', favorite: false, _u: 'bob@example.com', _p: 'FAKE-second-pass' } }); await sleep(400);
      await page.goto(LOGIN); await page.bringToFront(); await sleep(800);
      const two = await runCmd();
      const menu = await until(async () => { const t = await shadowText(page); return /Localhost Test Login/.test(t) && /Second Command Login/.test(t) && t; }, T(4000));
      const empty = (await vals()).every((v) => v === '');
      rec('fill-login command: two matches open the inline menu, nothing filled', two && two.action === 'menu' && !!menu && empty, `${JSON.stringify(two)} menu=${!!menu} empty=${empty}`);
      if (menu) { const row = await until(async () => { const h = await findInShadow(page, /Second Command Login/); return h.length && h[0]; }, T(2000)); if (row) await page.mouse.click(row.x, row.y); }
      const picked = await until(async () => { const v = await vals(); return v[0] === 'bob@example.com' && v; }, T(4000));
      rec('fill-login command: picking from the menu fills that login', !!picked, JSON.stringify(await vals()));

      ctrl({ unlocked: false }); await sleep(400);
      await page.goto(LOGIN); await page.bringToFront(); await sleep(800);
      const locked = await runCmd();
      const lockedNote = await until(async () => { const t = await shadowText(page); return /Unlock gitpass first/.test(t) && t; }, T(4000));
      rec('fill-login command: locked shows the in-page notice, nothing filled', locked && locked.action === 'notice' && locked.code === 'locked' && !!lockedNote && (await vals()).every((v) => v === ''), `${JSON.stringify(locked)} notice=${!!lockedNote}`);

      const pagesBefore = ctx.pages().length;
      const ext = await ctx.newPage(); await ext.goto(`${EXT}/options/options.html`); await ext.bringToFront(); await sleep(300);
      const ignored = await runCmd();
      rec('fill-login command: extension/non-http page is ignored without errors', ignored && ignored.action === 'ignore', JSON.stringify(ignored));
      await ext.close();
      rec('fill-login command never opened a gitpass window', ctx.pages().length === pagesBefore, `pages=${ctx.pages().length}`);
      ctrl({ unlocked: true, delete_item: 'v1:cmd2' }); await sleep(400);
    }

    // --- "Never for this site" banner flow (#56)
    await page.goto(LOGIN); await sleep(1000);
    await page.fill('#u', 'neveruser@example.com'); await page.fill('#p', 'Never-Pass-123'); await page.click('#go');
    const neverBtn = await until(async () => { const h = await findInShadow(page, /^\s*(Never for this site|ဤဆိုက်အတွက် ဘယ်တော့မှမသိမ်းပါနှင့်|ไม่ต้องสำหรับไซต์นี้)\s*$/i); return h.length && h; }, T(6000));
    rec('save banner offers "Never for this site"', !!neverBtn, neverBtn ? `button "${neverBtn[0].text}"` : 'no Never button found');
    if (neverBtn) {
      await page.mouse.click(neverBtn[0].x, neverBtn[0].y);
      await sleep(500);
    }
    // Submit again on the same site -> banner must NOT appear
    await page.fill('#u', 'neveruser@example.com'); await page.fill('#p', 'Never-Pass-456'); await page.click('#go');
    await sleep(1500);
    const suppressedBanner = await findInShadow(page, /^\s*(Save|บันทึก|သိမ်းရန်)\s*$/i);
    rec('excepted site: submitting login form does not show save banner', !suppressedBanner || suppressedBanner.length === 0, `banner count=${(suppressedBanner || []).length}`);

    // --- Options page (#56)
    const popup = await ctx.newPage();
    await popup.goto(`${EXT}/options/options.html`);
    const opt = await until(async () => (await popup.locator('#autofill').count()) && (await popup.locator('#shortcut').textContent()) !== '—', T(4000));
    rec('options page renders settings + shortcut', !!opt, opt ? `shortcut=${await popup.locator('#shortcut').textContent()}` : '');

    // Verify excepted site is listed in "Never save on these sites" section
    const exceptedRow = await until(async () => {
      const rows = await popup.locator('#never-save-list .t').allTextContents();
      return rows.some((r) => r.includes('localhost')) ? rows : null;
    }, T(4000));
    rec('options page lists excepted domain', !!exceptedRow, JSON.stringify(exceptedRow));

    // Remove the exception
    const removeBtn = popup.locator('#never-save-list button.sec');
    if (await removeBtn.count()) {
      await removeBtn.first().click();
      const emptyState = await until(async () => await popup.locator('#never-save-empty').isVisible(), T(4000));
      rec('options page: removing exception shows empty state', !!emptyState);
    }

    // Go back to login page and submit again -> save banner reappears!
    await page.goto(LOGIN); await sleep(1000);
    await page.fill('#u', 'neveruser@example.com'); await page.fill('#p', 'Re-Enabled-Pass-789'); await page.click('#go');
    const reenabledBanner = await until(async () => { const h = await findInShadow(page, /^\s*(Save|บันทึก|သိမ်းရန်)\s*$/i); return h.length && h; }, T(6000));
    rec('removing exception in options re-enables save banner', !!reenabledBanner, reenabledBanner ? `button "${reenabledBanner[0].text}"` : 'banner did not reappear');
    if (reenabledBanner) await page.mouse.click(reenabledBanner[0].x, reenabledBanner[0].y);
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
