// Unit tests for list row quick actions (issue #255).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/recent.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

test('quickActions: per-category actions, Open website only for logins with an http(s) URL', () => {
  assert.deepStrictEqual(V.quickActions({ category: 'login', url: 'https://example.com' }), ['copyUsername', 'copyPassword', 'openWebsite']);
  assert.deepStrictEqual(V.quickActions({ category: 'login', url: '' }), ['copyUsername', 'copyPassword']);
  assert.deepStrictEqual(V.quickActions({ category: 'login' }), ['copyUsername', 'copyPassword']);
  assert.deepStrictEqual(V.quickActions({ category: 'login', url: 'javascript:alert(1)' }), ['copyUsername', 'copyPassword'], 'no open action for a non-http(s) URL');
  assert.deepStrictEqual(V.quickActions({ category: 'login', url: 'https://' }), ['copyUsername', 'copyPassword'], 'no open action for a URL without a host');
  assert.deepStrictEqual(V.quickActions({ category: 'credit_card' }), ['copyNumber']);
  assert.deepStrictEqual(V.quickActions({ category: 'api_credential' }), ['copyCredential']);
  for (const cat of ['secure_note', 'identity', 'document', 'ssh_key', 'wifi', 'passkey']) {
    assert.deepStrictEqual(V.quickActions({ category: cat }), [], `${cat} has no quick actions`);
  }
  assert.deepStrictEqual(V.quickActions(null), []);
  assert.deepStrictEqual(V.quickActions({}), []);
});

const item = (id, title, category, extra = {}) => ({
  id, vault_id: 'v1', vault_name: 'Personal', category, title,
  subtitle: category === 'login' ? 'user@example.invalid' : '—', favorite: false, ...extra,
});
const ITEMS = [
  item('v1:login', 'Alpha Login', 'login', { url: 'https://alpha.example' }),
  item('v1:login-nourl', 'Beta Login', 'login'),
  item('v1:card', 'Gamma Card', 'credit_card'),
  item('v1:api', 'Delta API', 'api_credential'),
  item('v1:note', 'Epsilon Note', 'secure_note'),
];

// Popup DOM with a fake host (no chrome.*, no real vault).
function popup() {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: ITEMS });
      case 'match_active': return ok({ items: ITEMS, origin: 'https://alpha.example' });
      case 'get_item': {
        const it = ITEMS.find((x) => x.id === msg.item_id);
        let fields = [];
        if (it.category === 'login') {
          fields = [
            { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
            { name: 'password', label: 'password', kind: 'password', concealed: true },
          ];
        } else if (it.category === 'credit_card') {
          fields = [{ name: 'card_number', label: 'card number', kind: 'card_number', concealed: true }];
        } else if (it.category === 'api_credential') {
          fields = [{ name: 'api_key', label: 'api key', kind: 'text', concealed: true }];
        }
        return ok({ item: { ...it, fields, tags: [] } });
      }
      case 'reveal': return ok({ value: 'FAKE-SECRET' });
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); };
const rowFor = (w, title) => [...w.document.querySelectorAll('#listInner .row')].find((r) => r.querySelector('.t').textContent === title);
const actionsOf = (w, title) => [...rowFor(w, title).querySelectorAll('.quick-btn')].map((b) => ({
  action: b.dataset.action, label: b.getAttribute('aria-label'), tabIndex: b.tabIndex,
}));

test('rows render the right quick actions; only the selected row is in the Tab order; nothing secret is in the DOM before a click', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();

  assert.deepStrictEqual(actionsOf(w, 'Alpha Login').map((a) => a.action), ['copyUsername', 'copyPassword', 'openWebsite']);
  assert.deepStrictEqual(actionsOf(w, 'Beta Login').map((a) => a.action), ['copyUsername', 'copyPassword'], 'login without a URL has no Open website');
  assert.deepStrictEqual(actionsOf(w, 'Gamma Card').map((a) => a.action), ['copyNumber']);
  assert.deepStrictEqual(actionsOf(w, 'Delta API').map((a) => a.action), ['copyCredential']);
  assert.deepStrictEqual(actionsOf(w, 'Epsilon Note'), [], 'other categories show no quick actions');

  assert.deepStrictEqual(actionsOf(w, 'Alpha Login').map((a) => a.label), ['Copy username', 'Copy password', 'Open website'], 'buttons carry accessible names');
  assert.ok(actionsOf(w, 'Alpha Login').every((a) => a.tabIndex === 0), 'the selected row is tabbable');
  assert.ok(actionsOf(w, 'Beta Login').every((a) => a.tabIndex === -1), 'unselected rows stay out of the Tab order');
  assert.ok(actionsOf(w, 'Gamma Card').every((a) => a.tabIndex === -1), 'unselected rows stay out of the Tab order');

  assert.ok(!w.document.body.textContent.includes('FAKE-SECRET'), 'no secret is preloaded into the list');
  assert.ok(!sent.some((m) => m.type === 'popup:reveal'), 'rendering the list must not reveal anything');
});

test('clicking Copy password on the selected login copies via reveal and shows the Copied toast', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();

  rowFor(w, 'Alpha Login').querySelector('.quick-btn[data-action="copyPassword"]').click();
  await settle();

  assert.strictEqual(w.__gitpassMockClipboard, 'FAKE-SECRET');
  assert.strictEqual(w.document.getElementById('toast').textContent, 'Password copied');
  const reveal = sent.filter((m) => m.type === 'popup:reveal');
  assert.strictEqual(reveal.length, 1, 'exactly one reveal, for the password field');
  assert.strictEqual(reveal[0].field, 'password');
});

test('clicking Copy username needs no reveal; clicking a card/API row selects it first, then copies its primary secret', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();

  rowFor(w, 'Alpha Login').querySelector('.quick-btn[data-action="copyUsername"]').click();
  await settle();
  assert.strictEqual(w.__gitpassMockClipboard, 'user@example.invalid');
  assert.strictEqual(w.document.getElementById('toast').textContent, 'Username copied');
  assert.ok(!sent.some((m) => m.type === 'popup:reveal'), 'the username is not a concealed value, so no reveal');

  rowFor(w, 'Gamma Card').querySelector('.quick-btn[data-action="copyNumber"]').click();
  await settle();
  assert.strictEqual(w.document.querySelector('#detail h2').textContent, 'Gamma Card', 'clicking an unselected row selects it first');
  assert.strictEqual(w.__gitpassMockClipboard, 'FAKE-SECRET');
  assert.strictEqual(w.document.getElementById('toast').textContent, 'Copied');
  assert.ok(sent.some((m) => m.type === 'popup:reveal' && m.field === 'card_number'), 'card copy reveals the card number');

  rowFor(w, 'Delta API').querySelector('.quick-btn[data-action="copyCredential"]').click();
  await settle();
  assert.strictEqual(w.document.querySelector('#detail h2').textContent, 'Delta API');
  assert.strictEqual(w.__gitpassMockClipboard, 'FAKE-SECRET');
  assert.ok(sent.some((m) => m.type === 'popup:reveal' && m.field === 'api_key'), 'API copy reveals the credential field');
});

test('clicking Open website opens the selected login URL and does not fill', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();
  const opened = [];
  w.open = (url) => { opened.push(url); return null; };

  rowFor(w, 'Alpha Login').querySelector('.quick-btn[data-action="openWebsite"]').click();
  await settle();

  assert.deepStrictEqual(opened, ['https://alpha.example']);
  assert.ok(!sent.some((m) => m.type === 'popup:open_fill'), 'a quick action must not trigger fill');
  assert.strictEqual(w.document.querySelector('#detail h2').textContent, 'Alpha Login', 'clicking a quick action keeps the selection');
});
