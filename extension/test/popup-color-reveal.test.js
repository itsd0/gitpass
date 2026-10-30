// Unit tests for colour-coded revealed passwords in the popup detail (issue #336).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/capslock.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const REVEAL = 'Ab3-xY7!qP é';
const base = (id, title, extra = {}) => ({
  id, vault_id: 'v1', vault_name: 'Personal', category: 'login', title,
  subtitle: 'fake@example.invalid', url: 'https://' + title.toLowerCase() + '.example', favorite: false, has_totp: false, ...extra,
});
const ITEMS = () => [base('v1:i1', 'GitHub', { has_totp: true }), base('v1:i2', 'GitLab')];

// Popup DOM with a fake host (no chrome.*, no real vault). `trash_item`/`restore_item` really move
// items in and out of the mock list, so the list/toast assertions exercise the popup's own flow.
function popup() {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const items = ITEMS();
  const trashed = new Set();
  const live = () => items.filter((x) => !trashed.has(x.id));
  let locked = false;
  const sent = [];
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    const notFound = () => Promise.reject(Object.assign(new Error('not_found'), { code: 'not_found' }));
    switch (type) {
      case 'status':
        return ok(locked
          ? { unlocked: false, app_running: true }
          : { unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'lock': locked = true; return ok({ unlocked: false });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: live() });
      case 'match_active': return ok({ items: live(), origin: 'https://github.example' });
      case 'get_item': {
        const it = live().find((x) => x.id === msg.item_id);
        if (!it) return notFound();
        const fields = [
          { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
          { name: 'password', label: 'password', kind: 'password', concealed: true },
        ];
        if (it.has_totp) fields.push({ name: 'totp', label: 'one-time password', kind: 'totp', concealed: true });
        return ok({ item: { ...it, fields, tags: [] } });
      }
      case 'reveal': return live().some((x) => x.id === msg.item_id) ? ok({ value: REVEAL }) : notFound();
      case 'totp': return ok({ code: '123456', remaining: 20, period: 30 });
      case 'trash_item': {
        if (!live().some((x) => x.id === msg.item_id)) return notFound();
        trashed.add(msg.item_id);
        return ok({ ok: true });
      }
      case 'restore_item': {
        if (!trashed.has(msg.item_id)) return notFound();
        trashed.delete(msg.item_id);
        return ok({ ok: true });
      }
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent, trashed };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(50); await sleep(50); };
const V = require('../popup/view.js');
const pwRow = (w) => [...w.document.querySelectorAll('#detail .f')].find((r) => r.querySelector('.lbl').textContent === 'password');

test('charKind and secretRuns classify letters, digits, symbols, spaces and non-ASCII', () => {
  assert.deepStrictEqual(['a', 'Z', 'é', 'ก', '5', '٣', '!', '#', '·', ' ', '\t'].map(V.charKind),
    ['letter', 'letter', 'letter', 'letter', 'digit', 'digit', 'symbol', 'symbol', 'symbol', 'space', 'space']);
  const runs = V.secretRuns('ab3-- x');
  assert.deepStrictEqual(runs.map((r) => r.kind), ['letter', 'digit', 'symbol', 'space', 'letter']);
  assert.strictEqual(runs.map((r) => r.text).join(''), 'ab3-- x');
  assert.deepStrictEqual(V.secretRuns(''), []);
  assert.deepStrictEqual(V.secretRuns(null), []);
});

test('a revealed password is colour-coded, keeps its exact text, and reads as one string', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();

  const row = pwRow(w);
  const val = row.querySelector('.val');
  assert.strictEqual(val.querySelector('[class^="sec-"]'), null, 'masked before reveal');
  row.querySelector('.acts button[title="Reveal"]').click();
  await settle();

  assert.strictEqual(val.textContent, REVEAL, 'text is the exact value');
  assert.strictEqual(val.getAttribute('role'), 'img');
  assert.strictEqual(val.getAttribute('aria-label'), REVEAL);
  assert.deepStrictEqual([...val.querySelectorAll('.sec-digit')].map((n) => n.textContent), ['3', '7']);
  assert.deepStrictEqual([...val.querySelectorAll('.sec-symbol')].map((n) => n.textContent), ['-', '!']);
  assert.deepStrictEqual([...val.querySelectorAll('.sec-space')].map((n) => n.textContent), [' ']);
  assert.ok(sent.every((m) => m.type !== 'popup:copy'), 'revealing does not copy');

  // Hiding again drops the colour and the value.
  row.querySelector('.acts button').click();
  await settle();
  assert.ok(!val.textContent.includes('Ab3'), 'concealed again');
  assert.strictEqual(val.querySelector('[class^="sec-"]'), null);
  assert.strictEqual(val.getAttribute('aria-label'), null);
});

test('the option turns colour-coding off: plain text, no aria-label, same value', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();
  w.chrome = { storage: { local: { get: (k, cb) => cb(k === 'settings' ? { settings: { colorPasswords: false } } : {}), set: (d, cb) => cb && cb() } } };

  const row = pwRow(w);
  const val = row.querySelector('.val');
  row.querySelector('.acts button[title="Reveal"]').click();
  await settle();

  assert.strictEqual(val.textContent, REVEAL);
  assert.strictEqual(val.children.length, 0, 'no per-character spans');
  assert.strictEqual(val.getAttribute('aria-label'), null);
});

test('the options page has the colour-code toggle on by default, and the CSS themes the classes', () => {
  const html = fs.readFileSync(path.join(root, 'options', 'options.html'), 'utf8');
  assert.ok(html.includes('id="colorPasswords"'));
  const onboard = fs.readFileSync(path.join(root, 'lib', 'onboard-common.js'), 'utf8');
  assert.ok(/colorPasswords:\s*true/.test(onboard), 'on by default');
  const css = fs.readFileSync(path.join(root, 'popup', 'popup.css'), 'utf8');
  for (const c of ['.sec-digit', '.sec-symbol', '.sec-space']) assert.ok(css.includes(c), c);
});
