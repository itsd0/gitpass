// ⌘⇧L / Ctrl+Shift+L "fill-login" command (#124): command -> action mapping in bg-command.js, the
// manifest/locale wiring, and the content-script side (menu anchored to a login field, in-page notice).
// Fake host, fake tabs, jsdom pages only; all data is fake.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
// content.js + content/*.js in manifest order, evaluated as ONE script: a browser shares
// top-level let/const across content scripts, but jsdom w.eval calls do not share them.
const CONTENT_SRC = ['content.js', 'content/messaging.js', 'content/settings.js', 'content/detection.js', 'content/totp.js', 'content/dropdown.js', 'content/username.js', 'content/banner.js', 'content/command.js', 'content/prompts.js', 'content/wiring.js'].map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n');
globalThis.GitpassPolicy = require('../lib/policy.js');
const C = require('../bg-command.js');

const login = (id, extra) => ({ id, category: 'login', title: 'Site ' + id, subtitle: 'me', ...extra });
const hostErr = (code, message) => Object.assign(new Error(message || code), { code });

function deps({ url = 'https://site.example/login', items = [], matchError = null, fillError = null, tab } = {}) {
  const calls = [];
  return {
    calls,
    activeTab: async () => (tab !== undefined ? tab : { id: 7, url }),
    host: async (cmd, args) => { calls.push(['host', cmd, args]); if (matchError) throw matchError; return { items }; },
    fillTab: async (tabId, origin, itemId, extra) => { calls.push(['fill', tabId, origin, itemId, extra]); if (fillError) throw fillError; return true; },
    sendToTab: async (tabId, msg) => { calls.push(['send', tabId, msg]); },
  };
}

test('decide: one login fills, several open the menu, none is a notice', () => {
  assert.deepStrictEqual(C.decide({ items: [login('v:1')] }), { action: 'fill', item_id: 'v:1' });
  assert.deepStrictEqual(C.decide({ items: [login('v:1'), login('v:2')] }), { action: 'menu', count: 2 });
  assert.deepStrictEqual(C.decide({ items: [] }), { action: 'notice', code: 'no_match' });
  assert.deepStrictEqual(C.decide(null), { action: 'notice', code: 'no_match' });
  // Only logins count: a card or a note matching the site is never filled by the shortcut.
  assert.deepStrictEqual(C.decide({ items: [login('v:1'), { id: 'v:c', category: 'credit_card' }, { id: 'v:n', category: 'secure_note' }] }), { action: 'fill', item_id: 'v:1' });
  assert.deepStrictEqual(C.decide({ items: [{ id: 'v:c', category: 'credit_card' }] }), { action: 'notice', code: 'no_match' });
});

test('decide: host errors map to locked / app_not_running / failed', () => {
  assert.strictEqual(C.decide(null, hostErr('locked')).code, 'locked');
  assert.strictEqual(C.decide(null, new Error('vault is locked')).code, 'locked');
  assert.strictEqual(C.decide(null, hostErr('app_not_running')).code, 'app_not_running');
  assert.strictEqual(C.decide(null, new Error('Specified native messaging host not found.')).code, 'app_not_running');
  assert.strictEqual(C.decide(null, new Error('native host disconnected')).code, 'app_not_running');
  assert.strictEqual(C.decide(null, hostErr('origin_mismatch')).code, 'failed');
  for (const e of [hostErr('locked'), hostErr('x')]) assert.ok(C.NOTICE_CODES.has(C.decide(null, e).code));
});

test('run: exactly one match fills the top frame through fillTab with the tab origin', async () => {
  const d = deps({ items: [login('v:1')] });
  const r = await C.run(d);
  assert.deepStrictEqual(r, { action: 'fill', item_id: 'v:1' });
  assert.deepStrictEqual(d.calls[0], ['host', 'match', { origin: 'https://site.example' }]);
  assert.deepStrictEqual(d.calls[1], ['fill', 7, 'https://site.example', 'v:1', { via: 'command' }]);
  assert.strictEqual(d.calls.length, 2, 'no notice or menu message after a fill');
});

test('run: several matches ask the content script for the inline menu, no fill', async () => {
  const d = deps({ items: [login('v:1'), login('v:2')] });
  assert.strictEqual((await C.run(d)).action, 'menu');
  assert.ok(!d.calls.some((c) => c[0] === 'fill'));
  assert.deepStrictEqual(d.calls.at(-1), ['send', 7, { type: 'gitpass-command-menu', origin: 'https://site.example' }]);
});

test('run: no match, locked and app not running show a notice and never fill', async () => {
  for (const [opts, code] of [[{ items: [] }, 'no_match'], [{ matchError: hostErr('locked') }, 'locked'], [{ matchError: hostErr('app_not_running') }, 'app_not_running']]) {
    const d = deps(opts);
    const r = await C.run(d);
    assert.deepStrictEqual(r, { action: 'notice', code });
    assert.ok(!d.calls.some((c) => c[0] === 'fill'), code);
    assert.deepStrictEqual(d.calls.at(-1), ['send', 7, { type: 'gitpass-command-notice', code, origin: 'https://site.example' }]);
  }
});

test('run: a fill the host refuses turns into a notice', async () => {
  const d = deps({ items: [login('v:1')], fillError: hostErr('locked') });
  assert.deepStrictEqual(await C.run(d), { action: 'notice', code: 'locked' });
  assert.deepStrictEqual(d.calls.at(-1), ['send', 7, { type: 'gitpass-command-notice', code: 'locked', origin: 'https://site.example' }]);
});

test('run: non-http(s) and plain-http pages do nothing and do not throw', async () => {
  for (const url of ['chrome://extensions/', 'https://chromewebstore.google.com/detail/x', 'https://chrome.google.com/webstore/detail/x', 'chrome-extension://abc/popup.html', 'file:///tmp/x.html', 'http://site.example/', 'about:blank', '']) {
    const d = deps({ url, items: [login('v:1')] });
    const r = await C.run(d);
    assert.deepStrictEqual(r, { action: 'ignore' }, url);
    assert.strictEqual(d.calls.length, 0, url);
  }
  assert.deepStrictEqual(await C.run(deps({ tab: null })), { action: 'ignore' });
  assert.deepStrictEqual(await C.run({ ...deps(), activeTab: async () => { throw new Error('no window'); } }), { action: 'ignore' });
});

test('run: a tab without a content script (send fails) does not throw', async () => {
  const d = deps({ items: [] });
  d.sendToTab = async () => { throw new Error('Could not establish connection. Receiving end does not exist.'); };
  assert.deepStrictEqual(await C.run(d), { action: 'notice', code: 'no_match' });
});

test('manifest declares fill-login (⌘⇧L / Ctrl+Shift+L) with a localized description in en/my/th', () => {
  for (const f of ['manifest.json', 'manifest.firefox.json']) {
    const m = JSON.parse(fs.readFileSync(path.join(root, f), 'utf8'));
    const c = m.commands['fill-login'];
    assert.ok(c, f);
    assert.deepStrictEqual(c.suggested_key, { default: 'Ctrl+Shift+L', mac: 'Command+Shift+L' }, f);
    assert.strictEqual(c.description, '__MSG_cmdFillLogin__', f);
    assert.ok(m.commands._execute_action, f + ': popup shortcut kept');
  }
  const ff = JSON.parse(fs.readFileSync(path.join(root, 'manifest.firefox.json'), 'utf8'));
  assert.ok(ff.background.scripts.indexOf('bg-command.js') < ff.background.scripts.indexOf('background.js'));
  for (const loc of ['en', 'my', 'th']) {
    const msgs = JSON.parse(fs.readFileSync(path.join(root, '_locales', loc, 'messages.json'), 'utf8'));
    for (const k of ['cmdFillLogin', 'csCmdAppNotRunning', 'csCmdNoForm', 'csCmdFailed', 'optFillShortcut', 'optFillShortcutBody']) assert.ok(msgs[k] && msgs[k].message, `${loc}.${k}`);
  }
});

// ---- content script side ----
function bootContent(match) {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window, sent = [];
  let listener = null;
  w.HTMLElement.prototype.getBoundingClientRect = function () { return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 }; };
  const attach = w.Element.prototype.attachShadow; // open the closed shadow roots so the test can read the UI
  w.Element.prototype.attachShadow = function (o) { return attach.call(this, { ...o, mode: 'open' }); };
  w.chrome = { runtime: { id: 'ext', onMessage: { addListener(l) { listener = l; } },
    sendMessage: async (m) => { sent.push(m.type); return m.type === 'page:match' ? { ok: true, result: { origin: 'https://site.example', items: match } } : { ok: true, result: { prompt: null } }; } },
  i18n: { getMessage: () => '' } };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js']) w.eval(fs.readFileSync(path.join(root, f), 'utf8')); w.eval(CONTENT_SRC);
  const deliver = (msg) => listener(msg, { id: 'ext' }, () => {});
  const uiText = () => [...w.document.querySelectorAll('gitpass-ui')].map((h) => h.shadowRoot.textContent).join(' | ');
  return { w, sent, deliver, uiText };
}
const tick = (ms = 30) => new Promise((r) => setTimeout(r, ms));

test('content: gitpass-command-menu opens the inline menu on the focused login field', async () => {
  const c = bootContent([login('v:1', { title: 'Work account' }), login('v:2', { title: 'Home account' })]);
  await tick();
  const pw = c.w.document.getElementById('pw');
  pw.focus();
  c.deliver({ type: 'gitpass-command-menu', origin: 'https://site.example' });
  await tick();
  assert.ok(c.sent.includes('page:match'));
  assert.match(c.uiText(), /Work account/);
  assert.match(c.uiText(), /Home account/);
  assert.strictEqual(c.w.document.activeElement, pw, 'menu anchored to the focused field');
  assert.strictEqual(pw.value, '', 'opening the menu never fills');
  c.w.close();
});

test('content: gitpass-command-menu without focus anchors to the first login field', async () => {
  const c = bootContent([login('v:1'), login('v:2')]);
  await tick();
  c.deliver({ type: 'gitpass-command-menu', origin: 'https://site.example' });
  await tick();
  assert.strictEqual(c.w.document.activeElement, c.w.document.getElementById('em'));
  c.w.close();
});

test('content: notices are localized text; messages for another origin are dropped', async () => {
  const c = bootContent([]);
  await tick();
  c.deliver({ type: 'gitpass-command-notice', code: 'locked', origin: 'https://evil.example' });
  await tick();
  assert.doesNotMatch(c.uiText(), /Unlock gitpass first/);
  c.deliver({ type: 'gitpass-command-notice', code: 'locked', origin: 'https://site.example' });
  await tick();
  assert.match(c.uiText(), /Unlock gitpass first, then try again/);
  c.deliver({ type: 'gitpass-command-notice', code: 'no_match', origin: 'https://site.example' });
  await tick();
  assert.match(c.uiText(), /No gitpass logins for this site/);
  assert.doesNotMatch(c.uiText(), /Unlock gitpass first/, 'one notice at a time');
  c.deliver({ type: 'gitpass-command-notice', code: 'app_not_running', origin: 'https://site.example' });
  await tick();
  assert.match(c.uiText(), /Open the gitpass app, then try again/);
  c.w.close();
});

test('content: a command fill on a page with no login form shows the no-form notice', async () => {
  const dom = new JSDOM('<!doctype html><p>No form here</p>', { url: 'https://site.example/', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window; let listener = null;
  const attach = w.Element.prototype.attachShadow;
  w.Element.prototype.attachShadow = function (o) { return attach.call(this, { ...o, mode: 'open' }); };
  w.chrome = { runtime: { id: 'ext', onMessage: { addListener(l) { listener = l; } }, sendMessage: async () => ({ ok: true, result: { prompt: null } }) }, i18n: { getMessage: () => '' } };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js']) w.eval(fs.readFileSync(path.join(root, f), 'utf8')); w.eval(CONTENT_SRC);
  await tick();
  listener({ type: 'gitpass-fill', via: 'command', origin: 'https://site.example', username: 'FAKE-user', password: 'FAKE-pass' }, { id: 'ext' }, () => {});
  await tick();
  const text = [...w.document.querySelectorAll('gitpass-ui')].map((h) => h.shadowRoot.textContent).join(' ');
  assert.match(text, /No login form found on this page/);
  listener({ type: 'gitpass-command-menu', origin: 'https://site.example' }, { id: 'ext' }, () => {});
  await tick();
  assert.match([...w.document.querySelectorAll('gitpass-ui')].map((h) => h.shadowRoot.textContent).join(' '), /No login form found/);
  w.close();
});
