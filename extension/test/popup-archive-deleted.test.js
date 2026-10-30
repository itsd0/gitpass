// Archive and Recently Deleted views in the extension popup (issue #320).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');

const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const vaults = [{ id: 'v1', name: 'Personal' }];
const activeLogin = { id: 'v1:active', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Active Login', subtitle: 'user@example.com', url: 'https://example.com', favorite: false };
const archLogin = { id: 'v1:arch', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Archived Login', subtitle: 'arch@example.com', url: 'https://arch.example.com', favorite: false, archived: true };
const delLogin = { id: 'v1:del', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Deleted Login', subtitle: 'del@example.com', url: 'https://del.example.com', favorite: false, deleted_at: new Date(Date.now() - 2 * 864e5).toISOString() };

test('buildFilters includes Archive and Recently Deleted when counts are non-zero', () => {
  const empty = V.buildFilters(vaults, { archiveCount: 0, deletedCount: 0 });
  assert.ok(!empty.some((f) => f.id === 'archive'));
  assert.ok(!empty.some((f) => f.id === 'deleted'));

  const populated = V.buildFilters(vaults, { archiveCount: 2, deletedCount: 1 });
  const a = populated.find((f) => f.id === 'archive');
  const d = populated.find((f) => f.id === 'deleted');
  assert.ok(a && a.count === 2 && a.key === 'filterArchive');
  assert.ok(d && d.count === 1 && d.key === 'filterDeleted');
});

test('buildFilterChips places Archive and Recently Deleted after vaults and hides them when empty', () => {
  const all = [activeLogin];
  const chipsEmpty = V.buildFilterChips(all, vaults, { currentFilter: 'all', archiveCount: 0, deletedCount: 0 });
  assert.ok(!chipsEmpty.some((c) => c.id === 'archive'));
  assert.ok(!chipsEmpty.some((c) => c.id === 'deleted'));

  const chipsPopulated = V.buildFilterChips(all, vaults, { currentFilter: 'all', archiveCount: 3, deletedCount: 5 });
  const a = chipsPopulated.find((c) => c.id === 'archive');
  const d = chipsPopulated.find((c) => c.id === 'deleted');
  assert.ok(a && a.count === 3 && a.key === 'filterArchive');
  assert.ok(d && d.count === 5 && d.key === 'filterDeleted');

  // When active filter is archive or deleted, chip is preserved even if count is 0
  const activeArch = V.buildFilterChips(all, vaults, { currentFilter: 'archive', archiveCount: 0, deletedCount: 0 });
  assert.ok(activeArch.some((c) => c.id === 'archive'));
  const activeDel = V.buildFilterChips(all, vaults, { currentFilter: 'deleted', archiveCount: 0, deletedCount: 0 });
  assert.ok(activeDel.some((c) => c.id === 'deleted'));
});

test('filterItems partitions active, archived, and deleted items correctly', () => {
  const all = [activeLogin, archLogin, delLogin];
  const suggested = [activeLogin, archLogin, delLogin];

  // Suggestions never includes archived or deleted items
  const suggRes = V.filterItems(all, suggested, 'suggested', '');
  assert.deepStrictEqual(suggRes.map((x) => x.id), ['v1:active']);

  // All items filter excludes archived and deleted
  const allRes = V.filterItems(all, suggested, 'all', '');
  assert.deepStrictEqual(allRes.map((x) => x.id), ['v1:active']);

  // Archive filter returns archived non-deleted items
  const archRes = V.filterItems(all, suggested, 'archive', '', 'title', { archived: [archLogin], deleted: [delLogin] });
  assert.deepStrictEqual(archRes.map((x) => x.id), ['v1:arch']);

  // Deleted filter returns deleted items
  const delRes = V.filterItems(all, suggested, 'deleted', '', 'title', { archived: [archLogin], deleted: [delLogin] });
  assert.deepStrictEqual(delRes.map((x) => x.id), ['v1:del']);

  // Searching from suggestions searches all non-archived non-deleted items
  const qRes = V.filterItems(all, suggested, 'suggested', 'Login');
  assert.deepStrictEqual(qRes.map((x) => x.id), ['v1:active']);
});

test('formatTrashSubtitle computes relative days ago and days until purge', () => {
  const now = 1730000000000;
  const twoDaysAgo = now - 2 * 24 * 60 * 60 * 1000;
  const sub = V.formatTrashSubtitle(twoDaysAgo, now);
  assert.ok(sub);
  assert.strictEqual(sub.daysAgo, 2);
  assert.strictEqual(sub.daysLeft, 28);
  assert.strictEqual(sub.danger, false);
  assert.strictEqual(sub.text, 'Deleted 2 days ago · Deletes in 28 days');

  // Days left <= 3 flags danger: true
  const twentyEightDaysAgo = now - 28 * 24 * 60 * 60 * 1000;
  const subDanger = V.formatTrashSubtitle(twentyEightDaysAgo, now);
  assert.strictEqual(subDanger.danger, true);
  assert.strictEqual(subDanger.daysLeft, 2);
});

test('keyAction recognizes Cmd+Alt+R / Ctrl+Alt+R as restore', () => {
  const macEv = { key: 'r', altKey: true, metaKey: true, ctrlKey: false, shiftKey: false };
  assert.strictEqual(V.keyAction(macEv, { hasSelection: false }), 'restore');

  const winEv = { key: 'R', altKey: true, metaKey: false, ctrlKey: true, shiftKey: false };
  assert.strictEqual(V.keyAction(winEv, { hasSelection: false }), 'restore');

  const shiftEv = { key: 'r', altKey: false, metaKey: true, ctrlKey: false, shiftKey: true };
  assert.strictEqual(V.keyAction(shiftEv, { hasSelection: false }), 'restore');
});

function createPopup(initialState = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;

  const mockVaults = [{ id: 'v1', name: 'Personal' }];
  let mockAll = initialState.all ? [...initialState.all] : [activeLogin];
  let mockArchived = initialState.archived ? [...initialState.archived] : [archLogin];
  let mockDeleted = initialState.deleted ? [...initialState.deleted] : [delLogin];

  const calls = [];
  w.__gitpassMock = (msg) => {
    const type = String(msg.type).replace(/^popup:/, '');
    calls.push({ type, msg });
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status':
        return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults':
        return ok({ vaults: mockVaults });
      case 'list_items':
        return ok({ items: mockAll });
      case 'list_archived':
        return ok({ items: mockArchived });
      case 'list_deleted':
        return ok({ items: mockDeleted });
      case 'match_active':
        return ok({ items: mockAll, origin: 'https://example.com' });
      case 'get_item': {
        const id = msg.item_id;
        const it = mockAll.find((x) => x.id === id) || mockArchived.find((x) => x.id === id) || (msg.include_deleted ? mockDeleted.find((x) => x.id === id) : null);
        if (!it) return Promise.reject(new Error('not_found'));
        const fields = [
          { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
          { name: 'password', label: 'password', kind: 'password', concealed: true },
        ];
        return ok({ item: { ...it, fields, notes: '' } });
      }
      case 'archive_item': {
        const id = msg.item_id;
        mockAll = mockAll.filter((x) => x.id !== id);
        if (!mockArchived.some((x) => x.id === id)) mockArchived.push({ ...activeLogin, id, archived: true });
        return ok({ ok: true, archived: true });
      }
      case 'unarchive_item': {
        const id = msg.item_id;
        mockArchived = mockArchived.filter((x) => x.id !== id);
        if (!mockAll.some((x) => x.id === id)) mockAll.push({ ...archLogin, id, archived: false });
        return ok({ ok: true, archived: false });
      }
      case 'trash_item': {
        const id = msg.item_id;
        mockAll = mockAll.filter((x) => x.id !== id);
        mockArchived = mockArchived.filter((x) => x.id !== id);
        if (!mockDeleted.some((x) => x.id === id)) mockDeleted.push({ ...activeLogin, id, deleted_at: new Date().toISOString() });
        return ok({ ok: true });
      }
      case 'restore_item': {
        const id = msg.item_id;
        mockDeleted = mockDeleted.filter((x) => x.id !== id);
        if (!mockAll.some((x) => x.id === id)) mockAll.push({ ...delLogin, id, deleted_at: undefined });
        return ok({ ok: true });
      }
      default:
        return ok({});
    }
  };

  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, calls };
}

test('popup DOM: renders Archive and Recently Deleted chips with counts', async () => {
  const { dom, w } = createPopup();
  await new Promise((r) => setTimeout(r, 60));

  const archChip = dom.window.document.querySelector('#filterChips [data-filter-id="archive"]');
  const delChip = dom.window.document.querySelector('#filterChips [data-filter-id="deleted"]');
  assert.ok(archChip, 'Archive chip should be rendered');
  assert.ok(delChip, 'Recently Deleted chip should be rendered');
  assert.ok(archChip.textContent.includes('Archive (1)'), `expected Archive (1) but got ${archChip.textContent}`);
  assert.ok(delChip.textContent.includes('Recently Deleted (1)'), `expected Recently Deleted (1) but got ${delChip.textContent}`);
});

test('popup DOM: Archive view shows archived item, banner, and Unarchive action with undo', async () => {
  const { dom, w, calls } = createPopup();
  await new Promise((r) => setTimeout(r, 60));

  const archChip = dom.window.document.querySelector('#filterChips [data-filter-id="archive"]');
  archChip.click();
  await new Promise((r) => setTimeout(r, 60));

  assert.strictEqual(w.__gitpassPopup.S.filter, 'archive');
  const rows = dom.window.document.querySelectorAll('#list .row');
  assert.strictEqual(rows.length, 1);
  assert.ok(rows[0].textContent.includes('Archived Login'));

  // Detail view should show archived banner and Unarchive button
  const banner = dom.window.document.querySelector('#detail .trash-banner');
  assert.ok(banner, 'Archive banner should be present in detail view');
  assert.ok(banner.textContent.includes('This item is archived.'));

  const unarchBtn = dom.window.document.querySelector('#detail .d-actions button.btn-primary');
  assert.ok(unarchBtn, 'Unarchive button should be present in detail actions');
  assert.strictEqual(unarchBtn.textContent.trim(), 'Unarchive');

  // Clicking Unarchive sends unarchive_item and shows toast with Undo
  unarchBtn.click();
  await new Promise((r) => setTimeout(r, 80));

  assert.ok(calls.some((c) => c.type === 'unarchive_item' && c.msg.item_id === 'v1:arch'));
  const toast = dom.window.document.getElementById('toast');
  assert.ok(toast.textContent.includes('Item unarchived'));
  const undoBtn = toast.querySelector('button');
  assert.ok(undoBtn, 'Undo button should exist in toast');

  // Clicking Undo re-archives the item
  undoBtn.click();
  await new Promise((r) => setTimeout(r, 80));
  assert.ok(calls.some((c) => c.type === 'archive_item' && c.msg.item_id === 'v1:arch'));
});

test('popup DOM: Recently Deleted view shows deleted item subtitle, banner, Restore action, and disabled copy', async () => {
  const { dom, w, calls } = createPopup();
  await new Promise((r) => setTimeout(r, 60));

  const delChip = dom.window.document.querySelector('#filterChips [data-filter-id="deleted"]');
  delChip.click();
  await new Promise((r) => setTimeout(r, 60));

  assert.strictEqual(w.__gitpassPopup.S.filter, 'deleted');
  const rows = dom.window.document.querySelectorAll('#list .row');
  assert.strictEqual(rows.length, 1);
  assert.ok(rows[0].textContent.includes('Deleted Login'));

  // Subtitle shows deleted days ago
  const sub = rows[0].querySelector('.meta .u');
  assert.ok(sub.textContent.includes('Deleted 2 days ago'));

  // Detail view should show Recently Deleted banner
  const banner = dom.window.document.querySelector('#detail .trash-banner');
  assert.ok(banner, 'Recently Deleted banner should be present in detail view');
  assert.ok(banner.textContent.includes('This item is in Recently Deleted.'));
  assert.ok(banner.textContent.includes('Deletes in 28 days'));

  // Detail actions should show Restore button, no Fill, no Edit
  const restoreBtn = dom.window.document.querySelector('#detail .d-actions button.btn-primary');
  assert.ok(restoreBtn, 'Restore button should be present in detail actions');
  assert.strictEqual(restoreBtn.textContent.trim(), 'Restore');
  assert.ok(!dom.window.document.querySelector('#detail [data-action="edit"]'), 'Edit button must be omitted for deleted item');

  // Copy button on field rows should be disabled
  const copyBtn = dom.window.document.querySelector('#detail .f [title*="Recently Deleted"], #detail .f button[disabled]');
  assert.ok(copyBtn, 'Copy button must be disabled for deleted item');
  assert.strictEqual(copyBtn.disabled, true);

  // Clicking Restore sends restore_item and shows toast with Undo
  restoreBtn.click();
  await new Promise((r) => setTimeout(r, 80));

  assert.ok(calls.some((c) => c.type === 'restore_item' && c.msg.item_id === 'v1:del'));
  const toast = dom.window.document.getElementById('toast');
  assert.ok(toast.textContent.includes('Item restored'));
  const undoBtn = toast.querySelector('button');
  assert.ok(undoBtn, 'Undo button should exist in toast');

  // Clicking Undo re-trashes the item
  undoBtn.click();
  await new Promise((r) => setTimeout(r, 80));
  assert.ok(calls.some((c) => c.type === 'trash_item' && c.msg.item_id === 'v1:del'));
});

test('popup DOM: empty states for Archive and Recently Deleted', async () => {
  const { dom, w } = createPopup({ all: [activeLogin], archived: [], deleted: [] });
  await new Promise((r) => setTimeout(r, 60));

  w.__gitpassPopup.S.filter = 'archive';
  w.__gitpassPopup.renderFilter();
  w.__gitpassPopup.S.rows = [];
  const listInner = dom.window.document.getElementById('listInner');
  w.__gitpassPopup.renderEmpty(listInner);
  assert.ok(listInner.textContent.includes('Nothing archived'));
  assert.ok(listInner.textContent.includes('Items you archive will appear here.'));

  w.__gitpassPopup.S.filter = 'deleted';
  w.__gitpassPopup.renderFilter();
  w.__gitpassPopup.S.rows = [];
  listInner.textContent = '';
  w.__gitpassPopup.renderEmpty(listInner);
  assert.ok(listInner.textContent.includes('Recently Deleted is empty'));
  assert.ok(listInner.textContent.includes('Items you delete stay here for 30 days.'));
});
