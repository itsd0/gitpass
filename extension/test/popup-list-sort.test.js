// Popup list (#165): sort menu (newest edit first by default), API secret preview, no stale spacer padding.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/popup.js'];

const mk = (id, title, updated, created, extra = {}) => ({ id: `v1:${id}`, vault_id: 'v1', vault_name: 'Personal', category: 'login', title, subtitle: 'u', favorite: false, updated_at: updated, created_at: created, ...extra });

function popup(items) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
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
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return w;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const q = (w, sel) => w.document.querySelector(sel);
const change = (w, sel, value) => { const s = q(w, sel); s.value = value; s.dispatchEvent(new w.Event('change', { bubbles: true })); };
const titles = (w) => [...w.document.querySelectorAll('#listInner .row .t')].map((e) => e.textContent);

test('filterItems sorts by last edited, date created or title (favorites stay first)', () => {
  const items = [
    mk('a', 'Alpha', '2026-01-01T00:00:00Z', 300),
    mk('b', 'Bravo', '2026-03-01T00:00:00Z', 100),
    mk('c', 'Charlie', '2026-02-01T00:00:00Z', 200),
    mk('d', 'Delta', '2025-12-01T00:00:00Z', 400, { favorite: true }),
  ];
  const ids = (sort) => V.filterItems(items, [], 'all', '', sort).map((i) => i.title);
  assert.deepStrictEqual(ids('updated'), ['Delta', 'Bravo', 'Charlie', 'Alpha']);
  assert.deepStrictEqual(ids('created'), ['Delta', 'Alpha', 'Charlie', 'Bravo']);
  assert.deepStrictEqual(ids('title'), ['Delta', 'Alpha', 'Bravo', 'Charlie']);
  assert.deepStrictEqual(ids('bogus'), ids('title'), 'unknown sort falls back to title');
  assert.deepStrictEqual(V.SORTS, ['updated', 'created', 'title']);
});

test('popup defaults to newest edit first and the sort menu re-sorts the list', async () => {
  const w = popup([mk('a', 'Alpha', '2026-01-01T00:00:00Z', 1), mk('b', 'Bravo', '2026-03-01T00:00:00Z', 2), mk('c', 'Charlie', '2026-02-01T00:00:00Z', 3)]);
  await sleep(60);
  change(w, '#filter', 'all');
  await sleep(30);
  assert.strictEqual(q(w, '#sort').value, 'updated');
  assert.deepStrictEqual(titles(w), ['Bravo', 'Charlie', 'Alpha']);
  change(w, '#sort', 'title');
  assert.deepStrictEqual(titles(w), ['Alpha', 'Bravo', 'Charlie']);
  change(w, '#sort', 'created');
  assert.deepStrictEqual(titles(w), ['Charlie', 'Bravo', 'Alpha']);
});

test('an empty list clears the spacer padding left by the previous list', async () => {
  const many = Array.from({ length: 300 }, (_, i) => mk(`i${i}`, `Fake ${String(i).padStart(3, '0')}`, i, i));
  const w = popup(many);
  await sleep(60);
  change(w, '#filter', 'all');
  await sleep(30);
  const inner = q(w, '#listInner');
  assert.ok(parseInt(inner.style.paddingBottom, 10) > 1000, 'virtual list pads the rows below the window');
  const input = q(w, '#q');
  input.value = 'no-such-item-zzz';
  input.dispatchEvent(new w.Event('input', { bubbles: true }));
  await sleep(150);
  assert.ok(q(w, '#listInner .empty'), 'empty state shown');
  assert.strictEqual(parseInt(inner.style.paddingTop, 10) || 0, 0);
  assert.strictEqual(parseInt(inner.style.paddingBottom, 10) || 0, 0);
});
