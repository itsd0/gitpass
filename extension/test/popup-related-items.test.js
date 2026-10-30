// Unit and DOM tests for Related items in extension popup item detail (issue #361).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

const root = path.join(__dirname, '..');
const SCRIPTS = [
  'lib/i18n.js',
  'lib/icons.js',
  'popup/icons.js',
  'popup/view.js',
  'popup/newitem.js',
  'popup/generator.js',
  'popup/capslock.js',
  'popup/skeletons.js',
  'popup/totp-list.js',
  'popup/detail.js',
  'popup/edit.js',
  'popup/largetype.js',
  'popup/quick-actions.js',
  'popup/panels.js',
  'popup/settings.js',
  'popup/popup.js',
];

const TEST_ITEMS = [
  {
    id: 'v1:item-1',
    vault_id: 'v1',
    vault_name: 'Personal',
    category: 'login',
    title: 'Alpha Login',
    subtitle: 'octocat',
    url: 'https://alpha.example.com/login',
    favorite: false,
    has_totp: false,
    has_attachments: false,
  },
  {
    id: 'v1:item-2',
    vault_id: 'v1',
    vault_name: 'Personal',
    category: 'api_credential',
    title: 'Beta API Key',
    subtitle: 'Personal access token',
    url: null,
    favorite: false,
    has_totp: false,
    has_attachments: false,
  },
  {
    id: 'v1:item-3',
    vault_id: 'v1',
    vault_name: 'Personal',
    category: 'secure_note',
    title: 'Gamma Recovery Codes',
    subtitle: 'Note',
    url: null,
    favorite: false,
    has_totp: false,
    has_attachments: false,
  },
];

function setupRelatedPopup(item1Related = null) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];

  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Test User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: TEST_ITEMS });
      case 'match_active': return ok({ items: TEST_ITEMS, origin: 'https://alpha.example.com' });
      case 'get_item': {
        const it = TEST_ITEMS.find((x) => x.id === msg.item_id) || TEST_ITEMS[0];
        let related_items = [];
        if (it.id === 'v1:item-1') {
          related_items = item1Related !== null ? item1Related : [
            { id: 'v1:item-2', title: 'Beta API Key', category: 'api_credential', available: true },
            { id: 'missing-item-id', title: 'Item not available', category: null, available: false },
          ];
        }
        return ok({
          item: {
            ...it,
            fields: [
              { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
              { name: 'password', label: 'password', kind: 'password', concealed: true },
            ],
            related_items,
            notes: 'Test notes',
            tags: [],
          },
        });
      }
      default: return ok({});
    }
  };

  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent };
}

test('popup item detail: renders Related items section with available and unavailable items', async () => {
  const { dom, w } = setupRelatedPopup();
  await new Promise((r) => setTimeout(r, 120));

  // Detail is loaded for Row 0 (item-1, which has related items)
  const relSec = dom.window.document.querySelector('.d-related');
  assert.ok(relSec, '.d-related section must be rendered');
  assert.strictEqual(relSec.getAttribute('role'), 'region');
  assert.strictEqual(relSec.getAttribute('aria-label'), 'Related items');

  const title = relSec.querySelector('.d-related-title');
  assert.strictEqual(title.textContent, 'Related items');

  const rows = relSec.querySelectorAll('.related-row');
  assert.strictEqual(rows.length, 2, 'Should render 2 related item rows');

  // Row 0 is available
  const row0 = rows[0];
  assert.strictEqual(row0.classList.contains('unavailable'), false);
  assert.strictEqual(row0.getAttribute('tabindex'), '0');
  assert.strictEqual(row0.querySelector('.related-title').textContent, 'Beta API Key');
  assert.ok(row0.querySelector('.related-ic svg'), 'Available row should render category icon SVG');

  // Row 1 is unavailable
  const row1 = rows[1];
  assert.strictEqual(row1.classList.contains('unavailable'), true);
  assert.strictEqual(row1.getAttribute('tabindex'), '-1');
  assert.strictEqual(row1.querySelector('.related-title').textContent, 'Item not available');
});

test('popup item detail: hidden when empty (no related items)', async () => {
  const { dom, w } = setupRelatedPopup([]);
  await new Promise((r) => setTimeout(r, 120));

  const relSec = dom.window.document.querySelector('.d-related');
  assert.strictEqual(relSec, null, '.d-related section must be hidden when empty');
});

test('popup item detail: clicking a related item opens that item, Back returns to previous item', async () => {
  const { dom, w } = setupRelatedPopup();
  await new Promise((r) => setTimeout(r, 120));

  let h2 = dom.window.document.querySelector('#detail h2');
  assert.strictEqual(h2.textContent, 'Alpha Login');

  // Back button should not be present initially
  assert.strictEqual(dom.window.document.querySelector('.back-item-btn'), null);

  // Click available related item (Beta API Key)
  const relRow = dom.window.document.querySelector('.related-row');
  assert.ok(relRow);
  relRow.click();
  await new Promise((r) => setTimeout(r, 120));

  // Now detail shows Beta API Key
  h2 = dom.window.document.querySelector('#detail h2');
  assert.strictEqual(h2.textContent, 'Beta API Key');

  // Back button should now be present in header
  const backBtn = dom.window.document.querySelector('.back-item-btn');
  assert.ok(backBtn, 'Back button must be rendered after navigating to related item');
  assert.strictEqual(backBtn.getAttribute('aria-label'), 'Back');

  // Click Back button
  backBtn.click();
  await new Promise((r) => setTimeout(r, 120));

  // Should return to Alpha Login
  h2 = dom.window.document.querySelector('#detail h2');
  assert.strictEqual(h2.textContent, 'Alpha Login');

  // Back button should be gone
  assert.strictEqual(dom.window.document.querySelector('.back-item-btn'), null);
});

test('popup item detail: Enter key on available related item opens that item', async () => {
  const { dom, w } = setupRelatedPopup();
  await new Promise((r) => setTimeout(r, 120));

  const relRow = dom.window.document.querySelector('.related-row');
  assert.ok(relRow);

  const enterEvent = new dom.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true });
  relRow.dispatchEvent(enterEvent);
  await new Promise((r) => setTimeout(r, 120));

  const h2 = dom.window.document.querySelector('#detail h2');
  assert.strictEqual(h2.textContent, 'Beta API Key');
});

test('popup item detail: arrow keys navigate between related item rows', async () => {
  const { dom, w } = setupRelatedPopup();
  await new Promise((r) => setTimeout(r, 120));

  const rows = dom.window.document.querySelectorAll('.related-row');
  assert.strictEqual(rows.length, 2);

  let focused = null;
  rows[1].focus = () => { focused = rows[1]; };
  rows[0].focus = () => { focused = rows[0]; };

  const downEvent = new dom.window.KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true });
  rows[0].dispatchEvent(downEvent);
  assert.strictEqual(focused, rows[1], 'ArrowDown should move focus to next row');

  const upEvent = new dom.window.KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true });
  rows[1].dispatchEvent(upEvent);
  assert.strictEqual(focused, rows[0], 'ArrowUp should move focus to previous row');
});
