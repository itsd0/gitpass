// Browse by tag in the extension popup (issue #314).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/popup.js'];

const items = [
  { id: 'v1:a', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'me@x.com', url: 'https://github.com', favorite: false, tags: ['dev', 'work'] },
  { id: 'v1:b', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitLab', subtitle: 'git@x.com', url: 'https://gitlab.com', favorite: false, tags: ['Dev'] },
  { id: 'v1:c', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: false, tags: ['personal'] },
  { id: 'v1:d', vault_id: 'v1', vault_name: 'Personal', category: 'wifi', title: 'Office', subtitle: 'net', favorite: false, tags: [] },
];

test('collectTags de-duplicates case-insensitively, counts, sorts A-Z', () => {
  assert.deepStrictEqual(V.collectTags(items), [
    { tag: 'dev', count: 2 },
    { tag: 'personal', count: 1 },
    { tag: 'work', count: 1 },
  ]);
  assert.deepStrictEqual(V.collectTags([{ tags: [' b ', 'B', '', null] }, { tags: null }]), [{ tag: 'b', count: 1 }]);
  assert.deepStrictEqual(V.collectTags([]), []);
});

test('buildFilterChips adds a Tags menu chip and keeps the active tag chip', () => {
  const chips = V.buildFilterChips(items, [{ id: 'v1', name: 'Personal' }], { origin: null, currentFilter: 'all' });
  const tags = chips.find((c) => c.id === 'tags');
  assert.ok(tags, 'Tags chip present');
  assert.strictEqual(tags.kind, 'tags');
  assert.deepStrictEqual(tags.tags.map((t) => t.tag), ['dev', 'personal', 'work']);
  assert.ok(!chips.some((c) => c.kind === 'tag'), 'no active-tag chip without a tag filter');
  const active = V.buildFilterChips(items, [{ id: 'v1', name: 'Personal' }], { origin: null, currentFilter: 'tag:dev' });
  assert.ok(active.some((c) => c.id === 'tag:dev' && c.kind === 'tag' && c.label === 'dev'));
  // Items without tags: no Tags chip, existing chip lists unchanged.
  const plain = V.buildFilterChips([{ category: 'login' }], [{ id: 'v1', name: 'Personal' }], { origin: null, currentFilter: 'all' });
  assert.ok(!plain.some((c) => c.id === 'tags'));
});

test('filterItems filters by tag, case-insensitively, favorites first', () => {
  assert.deepStrictEqual(V.filterItems(items, [], 'tag:dev', '').map((i) => i.id), ['v1:a', 'v1:b']);
  assert.deepStrictEqual(V.filterItems(items, [], 'tag:DEV', '').map((i) => i.id), ['v1:a', 'v1:b']);
  assert.deepStrictEqual(V.filterItems(items, [], 'tag:work', '').map((i) => i.id), ['v1:a']);
  assert.deepStrictEqual(V.filterItems(items, [], 'tag:nope', ''), []);
});

function popup() {
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
      case 'match_active': return ok({ items, origin: 'https://github.com' });
      case 'get_item': {
        const it = items.find((x) => x.id === msg.item_id) || items[0];
        return ok({ item: { ...it, fields: [], notes: '', tags: it.tags } });
      }
      default: return ok({});
    }
  };
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w };
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(50); await sleep(50); };
const q = (w, sel) => w.document.querySelector(sel);
const qa = (w, sel) => [...w.document.querySelectorAll(sel)];
const rows = (w) => qa(w, '#listInner .row').map((r) => r.querySelector('.t').textContent);
const chip = (w, id) => q(w, `#filterChips [data-filter-id="${id}"]`);

test('Tags chip menu filters the list, type-to-filter narrows it, active chip clears it', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();
  assert.ok(chip(w, 'tags'), 'Tags chip rendered');
  chip(w, 'tags').click();
  await settle();
  const menu = q(w, '#tagMenu');
  assert.ok(!menu.classList.contains('hidden'), 'tag menu opens');
  assert.deepStrictEqual(qa(w, '#tagMenu .tag-list button').map((b) => b.textContent), ['dev (2)', 'personal (1)', 'work (1)']);
  const input = q(w, '#tagMenu .tag-filter-input');
  input.value = 'per';
  input.dispatchEvent(new w.Event('input', { bubbles: true }));
  assert.deepStrictEqual(qa(w, '#tagMenu .tag-list button').map((b) => b.textContent), ['personal (1)']);
  input.value = '';
  input.dispatchEvent(new w.Event('input', { bubbles: true }));
  qa(w, '#tagMenu .tag-list button').find((b) => b.textContent.startsWith('dev ')).click();
  await settle();
  assert.deepStrictEqual(rows(w), ['GitHub', 'GitLab']);
  const active = chip(w, 'tag:dev');
  assert.ok(active, 'active tag chip rendered');
  assert.ok(active.textContent.includes('dev') && active.textContent.includes('×'), 'active chip shows tag and clear mark');
  active.click();
  await settle();
  assert.strictEqual(rows(w).length, 4, 'clearing the tag filter restores all items');
  assert.ok(!chip(w, 'tag:dev'), 'active tag chip removed');
});

test('detail tag pill applies the tag filter', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();
  qa(w, '#listInner .row')[0].click();
  await settle();
  const pill = qa(w, '#detail .tags button.tag').find((b) => b.textContent === 'work');
  assert.ok(pill, 'detail tag pill rendered as a button');
  pill.click();
  await settle();
  assert.deepStrictEqual(rows(w), ['GitHub']);
  assert.ok(chip(w, 'tag:work'), 'tag filter active after pill click');
});
