// Integration tests for extension popup shortcuts (issue #138).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/popup.js'];

const item = (id, title, user, url, category = 'login') => ({
  id, vault_id: 'v1', vault_name: 'Personal', category, title, subtitle: user,
  url, favorite: false, has_totp: false, username: user,
});
const ITEMS = [
  item('v1:i1', 'A GitHub', 'octocat', 'https://github.com'),
  item('v1:i2', 'B No User Login', '', 'https://example.com'),
  item('v1:i3', 'C Evil Script', 'hacker', 'javascript:alert(1)'),
  item('v1:i4', 'D Visa Card', '•••• 4242', '', 'credit_card'),
];

function popup({ items = ITEMS } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  const createdTabs = [];
  w.chrome = {
    tabs: {
      create: (opts) => {
        createdTabs.push(opts);
        return Promise.resolve(opts);
      },
    },
  };
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items });
      case 'match_active': return ok({ items, origin: 'https://github.com' });
      case 'get_item': {
        const it = items.find((x) => x.id === msg.item_id) || items[0];
        const fields = [];
        if (it.subtitle) fields.push({ name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle });
        fields.push({ name: 'password', label: 'password', kind: 'password', concealed: true });
        if (it.url) fields.push({ name: 'url', label: 'website', kind: 'url', concealed: false, value: it.url });
        return ok({ item: { ...it, fields, tags: [] } });
      }
      case 'reveal': return ok({ value: 'FAKE-PW' });
      default: return ok({});
    }
  };
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w, sent, createdTabs };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = () => sleep(60);

test('popup DOM: ⌘⇧U / Ctrl+Shift+U copies selected item username to clipboard with toast', async () => {
  const { w } = popup();
  await settle();

  // Select the first item (GitHub, user = octocat)
  const rows = w.document.querySelectorAll('#listInner .row');
  assert.ok(rows.length > 0);
  rows[0].dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
  await settle();

  // Press ⌘⇧U
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'U', metaKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  await settle();

  assert.strictEqual(w.__gitpassMockClipboard, 'octocat');
  const toast = w.document.getElementById('toast');
  assert.ok(toast);
  assert.strictEqual(toast.textContent, 'Username copied');
  assert.strictEqual(toast.classList.contains('hidden'), false);
});

test('popup DOM: ⌘⇧U when item has no username shows "No username" toast without copying', async () => {
  const { w } = popup();
  await settle();

  w.__gitpassMockClipboard = 'previous-clip';

  // Select the second item (No User Login, user = '')
  const rows = w.document.querySelectorAll('#listInner .row');
  assert.ok(rows.length >= 2);
  rows[1].dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
  await settle();

  // Press Ctrl+Shift+U
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'u', ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  await settle();

  // Clipboard should NOT be updated with empty string
  assert.strictEqual(w.__gitpassMockClipboard, 'previous-clip');
  const toast = w.document.getElementById('toast');
  assert.ok(toast);
  assert.strictEqual(toast.textContent, 'No username');
  assert.strictEqual(toast.classList.contains('hidden'), false);
});

test('popup DOM: ⌘O / Ctrl+O opens the selected item website in a new tab', async () => {
  const { w, createdTabs } = popup();
  await settle();

  // Select the first item (GitHub, url = https://github.com)
  const rows = w.document.querySelectorAll('#listInner .row');
  rows[0].dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
  await settle();

  // Press ⌘O
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'o', metaKey: true, bubbles: true, cancelable: true }));
  await settle();

  assert.strictEqual(createdTabs.length, 1);
  assert.strictEqual(createdTabs[0].url, 'https://github.com');
  assert.strictEqual(createdTabs[0].active, true);
});

test('popup DOM: ⌘O refuses dangerous schemes like javascript: and data:', async () => {
  const { w, createdTabs } = popup();
  await settle();

  // Select item with javascript: url
  const rows = w.document.querySelectorAll('#listInner .row');
  rows[2].dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
  await settle();

  // Press ⌘O
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'o', metaKey: true, bubbles: true, cancelable: true }));
  await settle();

  assert.strictEqual(createdTabs.length, 0, 'did not open javascript: tab');
});

test('popup DOM: ? does NOT trigger shortcuts dialog when typing in search field', async () => {
  const { w } = popup();
  await settle();

  const q = w.document.getElementById('q');
  q.focus();

  // Press '?'
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: '?', bubbles: true, cancelable: true }));
  await settle();

  const panel = w.document.getElementById('panel');
  assert.ok(panel.classList.contains('hidden'), 'panel stays hidden when typing in search input');
});

test('popup DOM: ? toggles keyboard shortcuts dialog and Esc closes it with focus restored', async () => {
  const { w } = popup();
  await settle();

  const q = w.document.getElementById('q');
  q.blur();
  const rows = w.document.querySelectorAll('#listInner .row');
  rows[0].focus();
  rows[0].dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
  await settle();

  const panel = w.document.getElementById('panel');
  assert.ok(panel.classList.contains('hidden'));

  // Press '?' to open
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: '?', bubbles: true, cancelable: true }));
  await settle();

  assert.strictEqual(panel.classList.contains('hidden'), false);
  assert.strictEqual(panel.getAttribute('role'), 'dialog');
  assert.strictEqual(panel.getAttribute('aria-modal'), 'true');
  assert.strictEqual(panel.getAttribute('aria-label'), 'Keyboard shortcuts');

  // Verify list and kbd chips
  const list = panel.querySelector('.shortcuts-list');
  assert.ok(list);
  const kbds = panel.querySelectorAll('kbd');
  assert.ok(kbds.length >= 8);

  // Focus trap: Tab cycles inside dialog
  const closeBtn = panel.querySelector('button');
  assert.ok(closeBtn);
  closeBtn.focus();
  assert.strictEqual(w.document.activeElement, closeBtn);

  // Tab key should cycle
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
  assert.strictEqual(w.document.activeElement, closeBtn);

  // Press Esc to close
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  await settle();

  assert.strictEqual(panel.classList.contains('hidden'), true);
});

test('popup DOM: ? toggles dialog closed when pressed while shortcuts dialog is open', async () => {
  const { w } = popup();
  await settle();

  // Blur text inputs
  w.document.getElementById('q').blur();

  // Open with ?
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: '?', bubbles: true, cancelable: true }));
  await settle();
  const panel = w.document.getElementById('panel');
  assert.strictEqual(panel.classList.contains('hidden'), false);

  // Press ? again to close
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: '?', bubbles: true, cancelable: true }));
  await settle();
  assert.strictEqual(panel.classList.contains('hidden'), true);
});
