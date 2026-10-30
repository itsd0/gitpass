const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const V = require('../popup/view.js');

const root = path.join(__dirname, '..', '..');
const rustCategories = () => {
  const src = fs.readFileSync(path.join(root, 'crates/gitpass-core/src/item.rs'), 'utf8');
  const body = src.match(/pub fn as_str\(&self\)[\s\S]*?match self \{([\s\S]*?)\n\s*\}/)[1];
  return [...body.matchAll(/=> "([a-z_]+)"/g)].map((m) => m[1]);
};

test('popup knows every item type the core / desktop app has', () => {
  const core = rustCategories();
  assert.ok(core.length >= 9);
  assert.deepStrictEqual([...V.CATEGORIES].sort(), [...core].sort());
  for (const c of core) { assert.ok(V.CAT_KEY[c], 'i18n key for ' + c); assert.ok(V.CAT_LABEL[c], 'label for ' + c); }
});

test('All items shows one row per item of every type', () => {
  const items = rustCategories().map((c, i) => ({ id: 'v:' + i, vault_id: 'v', category: c, title: c + ' item', subtitle: '' }));
  const rows = V.filterItems(items, [], 'all', '', 'title');
  assert.strictEqual(rows.length, items.length);
  const chips = V.buildFilterChips(items, [], { currentFilter: 'all' });
  for (const c of rustCategories()) assert.ok(chips.some((x) => x.id === 'cat:' + c), 'chip for ' + c);
});

test('item list can shrink and scroll inside the left pane', () => {
  const css = fs.readFileSync(path.join(__dirname, '..', 'popup', 'popup.css'), 'utf8');
  const rule = (sel) => (css.match(new RegExp('\\n' + sel.replace('.', '\\.') + ' \\{([^}]*)\\}')) || [])[1] || '';
  assert.match(rule('.list'), /min-height:\s*0/);
  assert.match(rule('.list'), /overflow-y:\s*auto/);
  assert.match(rule('.left'), /min-height:\s*0/);
});
