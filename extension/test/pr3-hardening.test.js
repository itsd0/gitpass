// PR #3 review hardening: no cross-account update, no content-supplied item id, verbatim passwords,
// no page sessionStorage for username caching.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const { JSDOM } = require('jsdom');

globalThis.GitpassPolicy = require('../lib/policy.js');
const forms = require('../lib/forms.js');
const B = require('../bg-page.js');

test('different username never offers update of another account', async () => {
  const sender = { tab: { id: 501, url: 'https://ex.com/login' }, frameId: 0, url: 'https://ex.com/login' };
  const calls = [];
  const host = async (cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === 'match') return { items: [{ id: 'v:alice', title: 'ex.com', subtitle: 'alice', category: 'login' }] };
    if (cmd === 'fill') return { username: 'alice', password: 'alicePw' };
    if (cmd === 'save_login') return { item_id: 'v:new' };
    return {};
  };
  const r = await B.handle({ type: 'page:capture', kind: 'login', username: 'bob', password: 'bobPw' }, sender, host);
  assert.strictEqual(r.prompt, 'save');
  const u = await B.handle({ type: 'page:save_decision', action: 'update', item_id: 'v:alice', user_click: true }, sender, host).catch((e) => e);
  assert.ok(!calls.some((c) => c.cmd === 'update_password'), 'alice must not be overwritten');
  void u;
});

test('content-supplied item_id is ignored', async () => {
  const sender = { tab: { id: 502, url: 'https://ex.com/login' }, frameId: 0, url: 'https://ex.com/login' };
  const calls = [];
  const host = async (cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === 'match') return { items: [{ id: 'v:mine', title: 'ex.com', subtitle: '', category: 'login' }] };
    if (cmd === 'fill') return { username: '', password: 'old' };
    return { ok: true };
  };
  const r = await B.handle({ type: 'page:capture', kind: 'login', username: 'me', password: 'new' }, sender, host);
  assert.strictEqual(r.prompt, 'update');
  await B.handle({ type: 'page:save_decision', action: 'update', item_id: 'v:victim', username: 'evil', user_click: true }, sender, host);
  const up = calls.find((c) => c.cmd === 'update_password');
  assert.strictEqual(up.args.item_id, 'v:mine');
  assert.strictEqual(up.args.username, 'me');
});

test('passwords are captured verbatim, usernames trimmed', () => {
  const dom = new JSDOM(`<form><input name="username" value="  bob "><input type="password" name="password" value=" pw with spaces "></form>`, { url: 'https://ex.com/' });
  const f = forms.detectForms(dom.window.document, { ignoreLayout: true })[0];
  assert.deepStrictEqual(forms.captureCredentials(f), { kind: 'login', username: 'bob', password: ' pw with spaces ' });
});

test('content script never uses the page sessionStorage/localStorage', () => {
  const src = fs.readFileSync(path.join(__dirname, '..', 'content.js'), 'utf8').replace(/\/\/.*$/gm, '');
  assert.ok(!/sessionStorage|localStorage/.test(src));
});
