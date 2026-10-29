'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');

const root = path.join(__dirname, '..');
const SCRIPTS = [
  'lib/i18n.js',
  'lib/icons.js',
  'popup/icons.js',
  'popup/view.js',
  'popup/newitem.js',
  'popup/generator.js',
  'popup/popup.js',
];

const ITEMS = [
  { id: 'v1:i1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'alice@github.com', url: 'https://github.com', favorite: false, last_used: 1793210000 },
  { id: 'v1:i2', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitLab', subtitle: 'git-admin', url: 'https://gitlab.com', favorite: false, last_used: 1793200000 },
  { id: 'v1:i3', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Slack', subtitle: 'alice@slack.com', url: 'https://slack.com', favorite: false },
  { id: 'v1:i4', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: true, last_used: 1793190000 },
];

function popup({ items = ITEMS, match = { items: [], origin: null } } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  w.__gitpassMock = (msg) => {
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items });
      case 'match_active': return ok(match);
      case 'get_item': return ok({ item: { ...items.find((i) => i.id === msg.item_id), fields: [] } });
      default: return ok({});
    }
  };
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); };

// =========================================================================
// 1. Pure function tests (getRecentlyUsed)
// =========================================================================

test('getRecentlyUsed: extracts items sorted newest last_used first up to max', () => {
  const sample = [
    { id: '1', title: 'Old', last_used: 100 },
    { id: '2', title: 'None' },
    { id: '3', title: 'Newest', last_used: 300 },
    { id: '4', title: 'Middle', last_used: 200 },
    { id: '5', title: 'Zero', last_used: 0 },
  ];

  const recent = V.getRecentlyUsed(sample, 2);
  assert.strictEqual(recent.length, 2);
  assert.strictEqual(recent[0].id, '3');
  assert.strictEqual(recent[1].id, '4');

  assert.deepStrictEqual(V.getRecentlyUsed(null), []);
  assert.deepStrictEqual(V.getRecentlyUsed([]), []);
});

// =========================================================================
// 2. DOM integration tests
// =========================================================================

test('popup DOM: renders Recently used section above full list when no search query and no site match', async () => {
  const { w } = popup({ items: ITEMS });
  await settle();

  const sec = w.document.getElementById('recentSection');
  assert.ok(sec, 'recentSection element exists');
  assert.ok(!sec.classList.contains('hidden'), 'recentSection is visible when no query and no site match');

  const header = sec.querySelector('.list-section-header');
  assert.ok(header, 'section header exists');
  assert.strictEqual(header.textContent, 'Recently used');

  const rows = sec.querySelectorAll('#recentList .row');
  assert.strictEqual(rows.length, 3, 'displays 3 recently used items');
  assert.strictEqual(rows[0].querySelector('.t').textContent, 'GitHub');
  assert.strictEqual(rows[1].querySelector('.t').textContent, 'GitLab');
  assert.strictEqual(rows[2].querySelector('.t').textContent, 'Visa');

  // Clicking a recent item triggers selection
  rows[1].click();
  await settle();
  const updatedRows = sec.querySelectorAll('#recentList .row');
  assert.ok(updatedRows[1].classList.contains('sel'), 'clicked recent item is selected');
  assert.strictEqual(w.document.querySelector('#detail h2').textContent, 'GitLab');
});

test('popup DOM: hides Recently used section when search query is entered', async () => {
  const { w } = popup({ items: ITEMS });
  await settle();

  const sec = w.document.getElementById('recentSection');
  assert.ok(!sec.classList.contains('hidden'), 'visible initially');

  const q = w.document.getElementById('q');
  q.value = 'slack';
  q.dispatchEvent(new w.Event('input', { bubbles: true }));
  await settle();

  assert.ok(sec.classList.contains('hidden'), 'hidden when search query is active');
});

test('popup DOM: hides Recently used section when site matches exist', async () => {
  const match = {
    origin: 'https://github.com',
    items: [ITEMS[0]],
  };
  const { w } = popup({ items: ITEMS, match });
  await settle();

  const sec = w.document.getElementById('recentSection');
  assert.ok(sec.classList.contains('hidden'), 'hidden when site matches exist');
});

test('popup DOM: hides Recently used section when no items have last_used', async () => {
  const itemsWithoutUsage = [
    { id: 'v1:i1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'alice' },
    { id: 'v1:i2', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitLab', subtitle: 'bob' },
  ];
  const { w } = popup({ items: itemsWithoutUsage });
  await settle();

  const sec = w.document.getElementById('recentSection');
  assert.ok(sec.classList.contains('hidden'), 'hidden when no items have last_used');
});
