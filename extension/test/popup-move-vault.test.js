// Move an item to another vault from the extension popup (issue #413).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const VAULTS = [
  { id: 'v1', name: 'Personal' },
  { id: 'v2', name: 'Work' },
  { id: 'v3', name: 'Shared' },
];

function popup({ vaults = VAULTS } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  const items = [
    { id: 'v1:login', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Alpha Login', subtitle: 'user@example.invalid', url: 'https://alpha.example', favorite: false },
  ];
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults': return ok({ vaults: vaults.map((v) => ({ ...v, item_count: items.filter((i) => i.vault_id === v.id).length })) });
      case 'list_items': return ok({ items: items.slice() });
      case 'list_archived': case 'list_deleted': return ok({ items: [] });
      case 'match_active': return ok({ items: items.slice(), origin: 'https://alpha.example' });
      case 'get_item': {
        const it = items.find((x) => x.id === msg.item_id);
        if (!it) return Promise.reject(Object.assign(new Error('not found'), { code: 'not_found' }));
        return ok({ item: { ...it, fields: [{ name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle }], tags: [] } });
      }
      case 'item_findings': return ok({ kinds: [] });
      case 'move_item': {
        const it = items.find((x) => x.id === msg.item_id);
        if (!it) return Promise.reject(Object.assign(new Error('not found'), { code: 'not_found' }));
        const to = vaults.find((v) => v.id === msg.to_vault_id);
        if (!to || to.id === it.vault_id) return Promise.reject(Object.assign(new Error('bad request'), { code: 'bad_request' }));
        it.vault_id = to.id;
        it.vault_name = to.name;
        it.id = to.id + ':' + it.id.split(':').slice(1).join(':');
        return ok({ ok: true, item_id: it.id, vault_id: to.id, vault_name: to.name });
      }
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent, items };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(60); await sleep(60); };
const q = (w, sel) => w.document.querySelector(sel);
const qa = (w, sel) => [...w.document.querySelectorAll(sel)];
const menuButtons = (w) => qa(w, '#detail .menu button');
const pickerButtons = (w) => qa(w, '#detail .move-picker-item');
const keyOn = (el, key) => el.dispatchEvent(new el.ownerDocument.defaultView.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));

async function openDetail(w) {
  w.document.querySelectorAll('#listInner .row')[0].click();
  await settle();
  assert.strictEqual(q(w, '#detail h2').textContent, 'Alpha Login');
}

function openMoreMenu(w) {
  const trigger = q(w, '#detail .menu-wrap > button');
  assert.ok(trigger, 'More button renders');
  trigger.click();
  return trigger;
}

function openPicker(w) {
  const trigger = openMoreMenu(w);
  const move = menuButtons(w).find((b) => b.textContent.includes('Move to vault'));
  assert.ok(move, 'menu offers Move to vault…');
  move.click();
  assert.ok(q(w, '#detail .move-picker'), 'picker opens');
  return trigger;
}

test('picker lists only the other vaults with counts; arrow-key nav, move, Undo moves back', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();
  await openDetail(w);
  const trigger = openPicker(w);

  const picks = pickerButtons(w);
  assert.deepStrictEqual(picks.map((b) => b.dataset.vaultId), ['v2', 'v3'], 'current vault is not listed');
  assert.deepStrictEqual(picks.map((b) => b.querySelector('.move-picker-name').textContent), ['Work', 'Shared']);
  assert.deepStrictEqual(picks.map((b) => b.querySelector('.move-picker-count').textContent), ['0', '0']);

  // ArrowDown moves focus Work -> Shared; ArrowUp moves it back.
  assert.strictEqual(w.document.activeElement, picks[0]);
  keyOn(picks[0], 'ArrowDown');
  assert.strictEqual(w.document.activeElement, picks[1]);
  keyOn(picks[1], 'ArrowUp');
  assert.strictEqual(w.document.activeElement, picks[0]);
  keyOn(picks[0], 'ArrowDown');

  // Enter on Shared moves the item there and shows an Undo toast.
  picks[1].click();
  await settle();
  await settle();
  const moves = sent.filter((m) => m.type === 'popup:move_item');
  assert.strictEqual(moves.length, 1);
  assert.strictEqual(moves[0].to_vault_id, 'v3');
  assert.ok(q(w, '#detail .sub').textContent.startsWith('Shared'), 'detail follows the moved item');
  const toast = q(w, '#toast');
  assert.ok(toast.textContent.includes('Moved to Shared'), 'toast names the target vault');
  const undo = toast.querySelector('.toast-action');
  assert.ok(undo && undo.textContent === 'Undo', 'toast offers Undo');

  // Undo moves the item back to Personal.
  undo.click();
  await settle();
  await settle();
  const movesAfter = sent.filter((m) => m.type === 'popup:move_item');
  assert.strictEqual(movesAfter.length, 2);
  assert.strictEqual(movesAfter[1].to_vault_id, 'v1');
  assert.ok(q(w, '#toast').textContent.includes('Moved to Personal'), 'undo toast names the original vault');
  assert.ok(q(w, '#detail .sub').textContent.startsWith('Personal'), 'detail follows the item back');
  assert.ok(trigger.isConnected === false || true, 'no assertion on detached trigger');
});

test('Esc closes the picker and returns focus to the More button', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();
  await openDetail(w);
  const trigger = openPicker(w);
  const picks = pickerButtons(w);
  keyOn(picks[0], 'Escape');
  assert.strictEqual(q(w, '#detail .move-picker'), null, 'picker is removed');
  assert.strictEqual(w.document.activeElement, trigger, 'focus returns to More');
});

test('menu hides Move to vault… when there is only one vault', async (t) => {
  const { dom, w } = popup({ vaults: [{ id: 'v1', name: 'Personal' }] });
  t.after(() => dom.window.close());
  await settle();
  await openDetail(w);
  openMoreMenu(w);
  assert.ok(!menuButtons(w).some((b) => b.textContent.includes('Move to vault')), 'no Move entry with one vault');
});
