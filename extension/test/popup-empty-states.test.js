// Unit tests for the popup list empty states (issue #52): which state renders and what its actions do.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/popup.js'];

const ITEMS = [
  { id: 'v1:i1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'me@example.invalid', url: 'https://github.com', favorite: false },
  { id: 'v1:i2', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: true },
];

// Popup DOM with a fake host: no real vault, no chrome.*.
function popup({ items = ITEMS, suggested = [], origin = null } = {}) {
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
      case 'list_items': return ok({ items });
      case 'match_active': return ok({ items: suggested, origin });
      case 'get_item': return ok({ item: { ...items.find((i) => i.id === msg.item_id), fields: [], tags: [] } });
      case 'generate': return ok({ password: 'FAKE-Gen-Pass-123' });
      case 'create_item': return ok({ item_id: 'v1:new', title: msg.item.title });
      default: return ok({});
    }
  };
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w, sent };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const q = (w, sel) => w.document.querySelector(sel);
const settle = async () => { await sleep(30); await sleep(30); };
const emptyText = (w) => { const e = q(w, '#listInner .empty'); return e ? e.textContent : ''; };
const buttons = (w) => [...w.document.querySelectorAll('#listInner .empty button')];
const chooseFilter = (w, id) => { const s = q(w, '#filter'); s.value = id; s.dispatchEvent(new w.Event('change', { bubbles: true })); };
const form = (w) => {
  const sel = q(w, '#panel select');
  const title = q(w, '#panel [data-field=title]');
  if (!sel || !title) return null;
  const url = q(w, '#panel [data-field=url]');
  return { type: sel.value, title: title.value, url: url ? url.value : null };
};

test('emptyState picks the state for each list context', () => {
  assert.deepStrictEqual(V.emptyState({ query: 'x', filter: 'all', origin: null, count: 0 }), { kind: 'search', query: 'x' });
  assert.deepStrictEqual(V.emptyState({ filter: 'suggested', origin: 'https://example.invalid/login', count: 3 }), { kind: 'site', host: 'example.invalid' });
  assert.deepStrictEqual(V.emptyState({ filter: 'suggested', origin: null, count: 3 }), { kind: 'nofill' });
  assert.deepStrictEqual(V.emptyState({ filter: 'all', origin: null, count: 0 }), { kind: 'vault' });
  assert.deepStrictEqual(V.emptyState({ filter: 'favorites', origin: 'https://example.invalid', count: 3 }), { kind: 'none' });
});

test('search with no results offers to create an item named after the query', async () => {
  const { dom, w, sent } = popup({ origin: 'https://example.invalid' });
  await settle();
  const input = q(w, '#q');
  input.value = 'gitlab';
  input.dispatchEvent(new w.Event('input', { bubbles: true }));
  await sleep(120);
  assert.match(emptyText(w), /No items found/);
  const b = buttons(w).find((x) => x.textContent === 'Create item named “gitlab”');
  assert.ok(b, 'create button is shown');
  b.click();
  assert.deepStrictEqual(form(w), { type: 'login', title: 'gitlab', url: 'https://example.invalid' });
  assert.ok(!sent.some((m) => m.type === 'popup:create_item'), 'nothing is saved before submitting');
  await settle();
  dom.window.close();
});

test('Enter on a search with no results opens New item before the debounce fires', async () => {
  const { dom, w } = popup({ origin: 'https://example.invalid' });
  await settle();
  const input = q(w, '#q');
  input.focus();
  input.value = 'gitlab';
  input.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  assert.deepStrictEqual(form(w), { type: 'login', title: 'gitlab', url: 'https://example.invalid' });
  await settle();
  dom.window.close();
});

test('suggested with no logins offers Save a login for the site with the origin prefilled', async () => {
  const { dom, w, sent } = popup({ items: ITEMS, suggested: [], origin: 'https://example.invalid' });
  await settle();
  chooseFilter(w, 'suggested');
  assert.match(emptyText(w), /No saved logins for this site/);
  assert.match(emptyText(w), /example\.invalid/);
  const save = buttons(w).find((x) => x.textContent === 'Save a login for example.invalid');
  assert.ok(save, 'save-login button is shown');
  save.click();
  assert.deepStrictEqual(form(w), { type: 'login', title: 'example.invalid', url: 'https://example.invalid' });
  assert.ok(!sent.some((m) => m.type === 'popup:create_item'), 'nothing is saved before submitting');
  await settle();
  dom.window.close();
});

test('Show all items switches an empty Suggested filter to All Items', async () => {
  const { dom, w } = popup({ items: ITEMS, suggested: [], origin: 'https://example.invalid' });
  await settle();
  chooseFilter(w, 'suggested');
  const all = buttons(w).find((x) => x.textContent === 'Show all items');
  assert.ok(all);
  all.click();
  assert.strictEqual(q(w, '#filter').value, 'all');
  assert.strictEqual(w.document.querySelectorAll('#listInner .row').length, ITEMS.length);
  await settle();
  dom.window.close();
});

test('a non-https page keeps Open an https page to fill and only offers Show all items', async () => {
  const { dom, w } = popup({ items: ITEMS, suggested: [], origin: null });
  await settle();
  chooseFilter(w, 'suggested');
  assert.match(emptyText(w), /Open an https page to fill/);
  assert.deepStrictEqual(buttons(w).map((b) => b.textContent), ['Show all items']);
  await settle();
  dom.window.close();
});

test('an empty vault welcomes the user and opens New item', async () => {
  const { dom, w } = popup({ items: [], suggested: [], origin: null });
  await settle();
  assert.match(emptyText(w), /Your vault is empty/);
  assert.match(emptyText(w), /Items you add will appear here/);
  const b = buttons(w).find((x) => x.textContent === 'New item');
  assert.ok(b);
  b.click();
  assert.deepStrictEqual(form(w), { type: 'login', title: '', url: '' });
  await settle();
  dom.window.close();
});

test('Enter on an empty-state button is not swallowed by the list shortcuts', async () => {
  const { dom, w } = popup({ items: ITEMS, suggested: [], origin: 'https://example.invalid' });
  await settle();
  chooseFilter(w, 'suggested');
  const save = buttons(w).find((x) => x.textContent === 'Save a login for example.invalid');
  save.focus();
  const ev = new w.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
  w.document.dispatchEvent(ev);
  assert.strictEqual(ev.defaultPrevented, false, 'the browser must be able to activate the focused button');
  await settle();
  dom.window.close();
});

test('an empty Favorites filter keeps the plain No items message', async () => {
  const { dom, w } = popup({ items: ITEMS.filter((i) => !i.favorite), suggested: [], origin: null });
  await settle();
  chooseFilter(w, 'favorites');
  assert.strictEqual(emptyText(w), 'No items');
  assert.strictEqual(buttons(w).length, 0);
  await settle();
  dom.window.close();
});
