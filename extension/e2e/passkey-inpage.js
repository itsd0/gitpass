#!/usr/bin/env node
// In-page passkey prompt e2e against the REAL gitpass-host (standalone mode, throwaway vault in $TMPDIR).
// Serves strict, hyperbeat-like WebAuthn options over https on app.hyperbeat.test (mapped to 127.0.0.1)
// so a parent-domain rp.id is exercised. Clicks the in-page "Save passkey" with a trusted mouse click.
'use strict';
const fs = require('fs'), os = require('os'), path = require('path'), https = require('https'), crypto = require('crypto');
const { execFileSync } = require('child_process');
const { chromium } = require('playwright');
const ROOT = path.join(__dirname, '..');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(fn, ms, step = 200) { const end = Date.now() + ms; let last; while (Date.now() < end) { try { last = await fn(); if (last) return last; } catch (_) {} await sleep(step); } return last; }
const extId = (p) => crypto.createHash('sha256').update(p).digest('hex').slice(0, 32).split('').map((c) => String.fromCharCode(97 + parseInt(c, 16))).join('');
// One long-lived CDP session per page: detaching a session tears down the WebAuthn virtual-authenticator
// environment Chrome keeps for the target, so helpers must not open/close their own sessions.
const sessions = new WeakMap();
async function session(page) { if (!sessions.has(page)) sessions.set(page, await page.context().newCDPSession(page)); return sessions.get(page); }
async function shadowButton(page, re) {
  const s = await session(page);
  try {
    const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true });
    const found = [];
    const walk = (n) => { if (n.nodeName === 'BUTTON') found.push(n); for (const k of [...(n.children || []), ...(n.shadowRoots || [])]) walk(k); };
    walk(root);
    for (const b of found) {
      const txt = (b.children || []).map((c) => c.nodeValue || '').join('');
      if (!re.test(txt)) continue;
      const { model } = await s.send('DOM.getBoxModel', { nodeId: b.nodeId }); const q = model.content;
      return { x: (q[0] + q[4]) / 2, y: (q[1] + q[5]) / 2 };
    }
    return null;
  } finally { /* keep the session */ }
}
async function shadowText(page) {
  const s = await session(page);
  try { const { root } = await s.send('DOM.getDocument', { depth: -1, pierce: true }); const t = []; const walk = (n, inS) => { if (n.nodeType === 3 && inS) t.push(n.nodeValue); for (const k of [...(n.children || []), ...(n.shadowRoots || [])]) walk(k, inS || (n.shadowRoots || []).includes(k)); }; walk(root, false); return t.join(' | '); }
  finally { /* keep the session */ }
}
(async () => {
  const tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'gitpass-pk-')));
  const extDir = path.join(tmp, 'ext'), profile = path.join(tmp, 'profile');
  fs.cpSync(ROOT, extDir, { recursive: true, filter: (src) => !/[/\\](node_modules|e2e|dist|\.git)([/\\]|$)/.test(src.slice(ROOT.length)) });
  const id = extId(extDir);
  fs.mkdirSync(path.join(profile, 'NativeMessagingHosts'), { recursive: true });
  const rel = path.join(ROOT, '..', 'target', 'release');
  const HOST = process.env.GITPASS_HOST_BIN || path.join(rel, 'gitpass-host'), CLI = process.env.GITPASS_CLI_BIN || path.join(rel, 'gitpass');
  const vault = path.join(tmp, 'vault'); const PW = 'e2e-only-master-password';
  const env = { ...process.env, HOME: tmp, GITPASS_PATH: vault, GITPASS_PASSWORD: PW, GITPASS_DEVICE_NAME: 'e2e' };
  const cli = (...a) => execFileSync(CLI, a, { env, stdio: ['ignore', 'pipe', 'pipe'] }).toString();
  cli('init', '--save-secret-key'); cli('vault', 'create', 'Personal');
  const conf = path.join(tmp, 'host.json');
  fs.writeFileSync(conf, JSON.stringify({ vault_path: vault, idle_secs: 600, allowed_callers: [`chrome-extension://${id}/`] }));
  const wrapper = path.join(tmp, 'host.sh');
  fs.writeFileSync(wrapper, `#!/bin/sh\nexport HOME='${tmp}'\nexport GITPASS_HOST_CONFIG='${conf}'\nexport GITPASS_AGENT_SOCK='${path.join(tmp, 'no-app', 'agent.sock')}'\nexec '${HOST}' "$@" 2>>'${tmp}/host.err'\n`, { mode: 0o755 });
  fs.writeFileSync(path.join(profile, 'NativeMessagingHosts', 'com.gitpass.host.json'), JSON.stringify({ name: 'com.gitpass.host', description: 'e2e', path: wrapper, type: 'stdio', allowed_origins: [`chrome-extension://${id}/`] }));
  execFileSync('openssl', ['req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1', '-nodes', '-keyout', path.join(tmp, 'k.pem'), '-out', path.join(tmp, 'c.pem'), '-days', '1', '-subj', '/CN=app.hyperbeat.test', '-addext', 'subjectAltName=DNS:app.hyperbeat.test'], { stdio: 'ignore' });
  const page = fs.readFileSync(path.join(ROOT, 'test', 'pages', 'webauthn-strict.html'));
  const server = https.createServer({ key: fs.readFileSync(path.join(tmp, 'k.pem')), cert: fs.readFileSync(path.join(tmp, 'c.pem')) }, (q, r) => { r.setHeader('content-type', 'text/html'); r.end(page); });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const port = server.address().port;
  const ctx = await chromium.launchPersistentContext(profile, { headless: !process.env.HEADED, channel: 'chromium', ignoreHTTPSErrors: true,
    args: [`--disable-extensions-except=${extDir}`, `--load-extension=${extDir}`, '--host-resolver-rules=MAP *.hyperbeat.test 127.0.0.1', '--ignore-certificate-errors'] });
  let code = 1; const results = [];
  const rec = (n, ok, d) => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'}  ${n}  — ${d}`); };
  try {
    const sw = ctx.serviceWorkers()[0] || await ctx.waitForEvent('serviceworker', { timeout: 15000 });
    sw.on('console', (m) => console.log('[sw]', m.text()));
    const unlock = await sw.evaluate((pw) => host('unlock', { password: pw }).then((r) => r, (e) => ({ error: e.code })), PW);
    console.log('unlock:', JSON.stringify(unlock).slice(0, 120));
    const p = await ctx.newPage();
    p.on('console', (m) => console.log('[page]', m.text()));
    const hostCall = (cmd, a) => sw.evaluate(([c, x]) => host(c, x).then((r) => r, (e) => ({ error: e.code || e.message })), [cmd, a || {}]);
    const out = () => p.locator('#out').textContent();
    const open = async (q, before) => {
      await p.goto(`https://app.hyperbeat.test:${port}/pay/app/settings/security?${q}`); await sleep(800);
      if (before) await before();
      await p.evaluate(() => { document.getElementById('out').textContent = ''; });
      await p.click('#reg');
      return until(() => shadowButton(p, /^Save passkey$/), 6000);
    };
    const clickShadow = async (re) => { const b = await until(() => shadowButton(p, re), 8000); if (!b) return false; await p.mouse.click(b.x, b.y); return true; };
    const items = () => cli('list', 'Personal').split('\n').filter((l) => /hyperbeat/i.test(l)).length;

    // 1. Strict shapes (parent-domain rp.id, RK + UV required, ES256/RS256/EdDSA, excludeCredentials, direct, credProps).
    let n0 = items();
    if (!await open('')) rec('strict: in-page prompt shown', false, await out());
    else {
      await sleep(Number(process.env.CLICK_DELAY_MS || 800)); await clickShadow(/^Save passkey$/);
      const o = await until(async () => (await out()) || '', 15000);
      const txt = await shadowText(p);
      rec('strict create (rp=parent domain) registered', /"registered":true/.test(o), o);
      rec('strict: dialog reports "Saved to gitpass"', /Saved to gitpass/.test(txt), txt.split('|').slice(-4).join('|'));
      rec('strict: passkey item stored', items() === n0 + 1, String(items()));
    }
    // 2. Hyperbeat's real shape (rp.id = hostname, residentKey discouraged, uv preferred, 36-byte user.id, attestation none).
    n0 = items();
    if (await open('rp=app.hyperbeat.test&uid=36')) {
      await sleep(800); await clickShadow(/^Save passkey$/);
      const o = await until(async () => (await out()) || '', 15000);
      rec('hyperbeat shape registered + stored', /"registered":true/.test(o) && items() === n0 + 1, o);
    } else rec('hyperbeat shape prompt', false, await out());
    // 3. Locked vault: error shown in the dialog (not silent); Retry after unlocking succeeds.
    await hostCall('lock'); n0 = items();
    if (await open('rp=app.hyperbeat.test')) {
      await sleep(800); await clickShadow(/^Save passkey$/);
      const errBtn = await until(() => shadowButton(p, /^Unlock & retry$/), 8000);
      const txt = await shadowText(p);
      rec('locked: dialog shows the error + Unlock & retry', !!errBtn && /locked/i.test(txt), txt.split('|').slice(-5).join('|'));
      rec('locked: page not resolved yet (waiting for the user)', !(await out()), await out());
      console.log('unlock again:', JSON.stringify(await hostCall('unlock', { password: PW })));
      await clickShadow(/^Unlock & retry$/);
      const o = await until(async () => (await out()) || '', 15000);
      rec('locked -> retry after unlock registers', /"registered":true/.test(o) && items() === n0 + 1, o);
    }
    // 4. Locked + "Use device passkey": the browser's own authenticator (CDP virtual platform authenticator) is used.
    await hostCall('lock'); n0 = items();
    let cdp; const addAuth = async () => {
      cdp = await session(p);
      await cdp.send('WebAuthn.enable', { enableUI: false }); await cdp.send('WebAuthn.addVirtualAuthenticator', { options: { protocol: 'ctap2', transport: 'internal', hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true } });
    };
    if (await open('rp=app.hyperbeat.test&uid=36', addAuth)) {
      await sleep(800); await clickShadow(/^Save passkey$/);
      await clickShadow(/^Use device passkey$/);
      let o = await until(async () => (await out()) || '', 8000);
      if (!o) { // activation was consumed: the main-world fallback bar asks for one more click
        await p.click('button:has-text("Continue")', { timeout: 5000 }).catch((e) => console.log('no bar', e.message));
        o = await until(async () => (await out()) || '', 8000);
      }
      rec('locked -> "Use device passkey" registers with the native authenticator, nothing stored in gitpass', /"registered":true/.test(o) && items() === n0, o);
    }
    code = results.every(Boolean) ? 0 : 1;
  } catch (e) { console.error(e); }
  finally {
    try { console.log('host stderr:', fs.readFileSync(path.join(tmp, 'host.err'), 'utf8').slice(-1500)); } catch (_) {}
    await ctx.close().catch(() => {}); server.close(); if (!process.env.KEEP_TMP) fs.rmSync(tmp, { recursive: true, force: true }); process.exit(code);
  }
})();
