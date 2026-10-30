// List density setting (issue #397): Comfortable (default) and Compact popup rows.
// The choice is stored in extension settings, applies a class on the list container,
// updates live on storage changes, and does not change keyboard navigation markup.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/recent.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const mk = (id, title, extra = {}) => ({ id: `v1:${id}`, vault_id: 'v1', vault_name: 'Personal', category: 'login', title, subtitle: 'user@example.com', favorite: false, updated_at: '2026-01-01T00:00:00Z', created_at: 100, ...extra });

function popup(items, opts = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  let storedSettings = { ...(opts.settings || {}) };
  const storageListeners = [];
  w.chrome = {
    storage: {
      local: {
        get: (k, cb) => setTimeout(() => cb(k === 'settings' ? { settings: storedSettings } : {}), 0),
        set: (d, cb) => { if (d.settings) storedSettings = d.settings; if (cb) setTimeout(cb, 0); },
      },
      onChanged: {
        addListener: (fn) => storageListeners.push(fn),
        fire: (changes, area) => storageListeners.forEach((fn) => fn(changes, area)),
      },
    },
  };
  w.__gitpassMock = (msg) => {
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items });
      case 'match_active': return ok({ items: [], origin: null });
      case 'get_item': return ok({ item: { ...items.find((i) => i.id === msg.item_id), fields: [], tags: [] } });
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { w, storageListeners, fireStorage: (changes, area) => storageListeners.forEach((fn) => fn(changes, area)) };
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

test('popupSettings defaults density to comfortable', async () => {
  const { w } = popup([mk('a', 'Alpha')]);
  await sleep(60);
  const list = w.document.getElementById('list');
  assert.ok(list, 'list container exists');
  assert.strictEqual(list.classList.contains('compact'), false, 'comfortable default has no compact class');
});

test('compact setting applies the compact class on the list', async () => {
  const { w } = popup([mk('a', 'Alpha')], { settings: { density: 'compact' } });
  await sleep(60);
  const list = w.document.getElementById('list');
  assert.strictEqual(list.classList.contains('compact'), true, 'compact class applied');
});

test('unknown density values fall back to comfortable', async () => {
  const { w } = popup([mk('a', 'Alpha')], { settings: { density: 'huge' } });
  await sleep(60);
  assert.strictEqual(w.document.getElementById('list').classList.contains('compact'), false);
});

test('storage change updates density instantly without reopening', async () => {
  const { w, storageListeners, fireStorage } = popup([mk('a', 'Alpha')], { settings: { density: 'comfortable' } });
  await sleep(60);
  const list = w.document.getElementById('list');
  assert.strictEqual(list.classList.contains('compact'), false);
  assert.ok(storageListeners.length > 0, 'onChanged listener registered');
  fireStorage({ settings: { newValue: { density: 'compact' } } }, 'local');
  assert.strictEqual(list.classList.contains('compact'), true, 'compact applied live');
  fireStorage({ settings: { newValue: { density: 'comfortable' } } }, 'local');
  assert.strictEqual(list.classList.contains('compact'), false, 'back to comfortable live');
  fireStorage({ settings: { newValue: { density: 'compact' } } }, 'sync');
  assert.strictEqual(list.classList.contains('compact'), false, 'other storage areas ignored');
});

test('row markup and roles are identical in compact density (keyboard nav unchanged)', async () => {
  const items = [mk('a', 'Alpha'), mk('b', 'Bravo')];
  const { w } = popup(items, { settings: { density: 'compact' } });
  await sleep(60);
  const rows = [...w.document.querySelectorAll('#listInner .row')];
  assert.strictEqual(rows.length >= 2, true, 'rows rendered');
  const first = rows[0];
  assert.strictEqual(first.getAttribute('role'), 'option');
  assert.strictEqual(first.getAttribute('tabindex'), '-1');
  assert.ok(first.querySelector('.t'), 'title element present');
  assert.ok(first.querySelector('.u'), 'subtitle element present');
  assert.ok(first.querySelector('.ic'), 'icon element present');
});

test('virtualized list uses 36px units in compact mode after scrolling', async () => {
  const items = [];
  for (let i = 0; i < 200; i++) items.push(mk('r' + i, 'Row ' + i));
  const { w } = popup(items, { settings: { density: 'compact' } });
  await sleep(80);
  const list = w.document.getElementById('list');
  const inner = w.document.getElementById('listInner');
  assert.strictEqual(list.classList.contains('compact'), true);
  // jsdom reports clientHeight 0; the popup falls back to a 380px viewport.
  list.scrollTop = 1800; // 50 compact rows down
  list.dispatchEvent(new w.Event('scroll'));
  await sleep(30);
  const padTop = parseInt(inner.style.paddingTop, 10);
  const padBottom = parseInt(inner.style.paddingBottom, 10);
  assert.strictEqual(padTop % 36, 0, 'padTop in 36px units, got ' + padTop);
  assert.strictEqual(padBottom % 36, 0, 'padBottom in 36px units, got ' + padBottom);
  // Window covers the scrolled position: row 50 (offset 1800) must be rendered.
  const rendered = [...inner.querySelectorAll('.row')].map((r) => parseInt(r.dataset.idx, 10));
  assert.ok(rendered.includes(50), 'row at the scrolled offset is rendered, got range ' + rendered[0] + '-' + rendered[rendered.length - 1]);
  assert.ok(padTop <= 50 * 36, 'padTop does not overshoot the first rendered row');
  const total = padTop + rendered.length * 36 + padBottom;
  assert.strictEqual(total, 200 * 36, 'padding + rows add up to the full compact list height');
});

test('switching density re-renders the virtualized list in the new units', async () => {
  const items = [];
  for (let i = 0; i < 200; i++) items.push(mk('r' + i, 'Row ' + i));
  const { w, fireStorage } = popup(items, { settings: { density: 'comfortable' } });
  await sleep(80);
  const inner = w.document.getElementById('listInner');
  inner.ownerDocument.getElementById('list').scrollTop = 2600; // 50 comfortable rows
  inner.ownerDocument.getElementById('list').dispatchEvent(new w.Event('scroll'));
  await sleep(30);
  const padComfort = parseInt(inner.style.paddingTop, 10);
  assert.strictEqual(padComfort % 52, 0, 'comfortable padTop in 52px units');
  fireStorage({ settings: { newValue: { density: 'compact' } } }, 'local');
  await sleep(30);
  const padCompact = parseInt(inner.style.paddingTop, 10);
  assert.strictEqual(padCompact % 36, 0, 'padTop recomputed in 36px units after live switch, got ' + padCompact);
});
