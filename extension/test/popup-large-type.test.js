// Unit tests for the popup's Large type view and Move to Recently Deleted with Undo (issue #278).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/popup.js'];

const REVEAL = 'Ab3-xY7!qP';
const base = (id, title, extra = {}) => ({
  id, vault_id: 'v1', vault_name: 'Personal', category: 'login', title,
  subtitle: 'fake@example.invalid', url: 'https://' + title.toLowerCase() + '.example', favorite: false, has_totp: false, ...extra,
});
const ITEMS = () => [base('v1:i1', 'GitHub', { has_totp: true }), base('v1:i2', 'GitLab')];

// Popup DOM with a fake host (no chrome.*, no real vault). `trash_item`/`restore_item` really move
// items in and out of the mock list, so the list/toast assertions exercise the popup's own flow.
function popup() {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const items = ITEMS();
  const trashed = new Set();
  const live = () => items.filter((x) => !trashed.has(x.id));
  let locked = false;
  const sent = [];
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    const notFound = () => Promise.reject(Object.assign(new Error('not_found'), { code: 'not_found' }));
    switch (type) {
      case 'status':
        return ok(locked
          ? { unlocked: false, app_running: true }
          : { unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'lock': locked = true; return ok({ unlocked: false });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: live() });
      case 'match_active': return ok({ items: live(), origin: 'https://github.example' });
      case 'get_item': {
        const it = live().find((x) => x.id === msg.item_id);
        if (!it) return notFound();
        const fields = [
          { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
          { name: 'password', label: 'password', kind: 'password', concealed: true },
        ];
        if (it.has_totp) fields.push({ name: 'totp', label: 'one-time password', kind: 'totp', concealed: true });
        return ok({ item: { ...it, fields, tags: [] } });
      }
      case 'reveal': return live().some((x) => x.id === msg.item_id) ? ok({ value: REVEAL }) : notFound();
      case 'totp': return ok({ code: '123456', remaining: 20, period: 30 });
      case 'trash_item': {
        if (!live().some((x) => x.id === msg.item_id)) return notFound();
        trashed.add(msg.item_id);
        return ok({ ok: true });
      }
      case 'restore_item': {
        if (!trashed.has(msg.item_id)) return notFound();
        trashed.delete(msg.item_id);
        return ok({ ok: true });
      }
      default: return ok({});
    }
  };
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w, sent, trashed };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(50); await sleep(50); };
const q = (w, sel) => w.document.querySelector(sel);
const key = (w, target, k, mods = {}) => target.dispatchEvent(new w.KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...mods }));
const ltBtn = (w, field) => q(w, `#detail [data-action="large-type"][data-field="${field}"]`);
const overlay = (w) => q(w, '#largetype');
const cells = (w) => [...w.document.querySelectorAll('#largetype [data-large-char]')];
const isOpen = (w) => !overlay(w).classList.contains('hidden');

test('a password opens Large type with one cell per character, kinds and index numbers; Esc clears it and restores focus', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();

  const btn = ltBtn(w, 'password');
  assert.ok(btn, 'the password row has a Large type button');
  assert.strictEqual(btn.getAttribute('aria-label'), 'Show password in large type');
  btn.click();
  await settle();

  assert.ok(isOpen(w), 'the overlay is visible');
  assert.strictEqual(overlay(w).getAttribute('aria-label'), 'Large type view');
  assert.deepStrictEqual(cells(w).map((c) => c.dataset.position), ['1', '2', '3', '4', '5', '6', '7', '8', '9', '10']);
  assert.deepStrictEqual(cells(w).map((c) => c.dataset.kind), ['letter', 'letter', 'digit', 'symbol', 'letter', 'letter', 'digit', 'symbol', 'letter', 'letter']);
  assert.deepStrictEqual(cells(w).map((c) => c.querySelector('.largetype-glyph').textContent), [...REVEAL]);
  assert.deepStrictEqual([...w.document.querySelectorAll('#largetype [data-large-group]')].map((g) => g.children.length), [8, 2], 'groups of 8 like the desktop');
  assert.deepStrictEqual(cells(w).map((c) => c.querySelector('.largetype-pos').textContent), ['1', '2', '3', '4', '5', '6', '7', '8', '9', '10']);
  assert.strictEqual(w.document.activeElement, q(w, '#largetype [data-action="largetype-close"]'), 'the close button takes focus');

  // The value came from the normal reveal path, not from storage.
  const reveals = sent.filter((m) => m.type === 'popup:reveal');
  assert.deepStrictEqual(reveals.map((m) => [m.item_id, m.field]), [['v1:i1', 'password']]);

  key(w, w.document, 'Escape');
  assert.ok(!isOpen(w), 'Esc closes the overlay');
  assert.strictEqual(overlay(w).textContent, '', 'the secret is removed from the DOM');
  assert.strictEqual(w.document.activeElement, btn, 'focus returns to the button that opened it');
});

test('the one-time code row has Large type with the six digits', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();

  const btn = ltBtn(w, 'totp');
  assert.ok(btn, 'the TOTP row has a Large type button');
  btn.click();
  await settle();

  assert.ok(isOpen(w));
  assert.deepStrictEqual(cells(w).map((c) => c.querySelector('.largetype-glyph').textContent), ['1', '2', '3', '4', '5', '6']);
  assert.deepStrictEqual(cells(w).map((c) => c.dataset.kind), ['digit', 'digit', 'digit', 'digit', 'digit', 'digit']);
  assert.strictEqual(w.document.activeElement, q(w, '#largetype [data-action="largetype-close"]'));

  key(w, w.document, 'Tab');
  assert.strictEqual(w.document.activeElement, q(w, '#largetype [data-action="largetype-close"]'), 'Tab stays inside the modal');
  key(w, w.document, 'Tab', { shiftKey: true });
  assert.strictEqual(w.document.activeElement, q(w, '#largetype [data-action="largetype-close"]'));

  q(w, '#largetype [data-action="largetype-close"]').click();
  assert.ok(!isOpen(w));
  assert.strictEqual(overlay(w).textContent, '');
  assert.strictEqual(w.document.activeElement, btn, 'the close button returns focus too');
});

test('locking the popup closes Large type and removes the value', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();

  ltBtn(w, 'password').click();
  await settle();
  assert.ok(isOpen(w));

  q(w, '#lock').click();
  await settle();

  assert.ok(!isOpen(w), 'lock closes the overlay');
  assert.strictEqual(overlay(w).textContent, '', 'lock clears the value from the DOM');
  assert.ok(!q(w, '#locked').classList.contains('hidden'), 'the popup shows the unlock screen');
});

test('Cmd/Ctrl+Alt+L opens Large type for the selected item', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();

  key(w, w.document, 'l', { metaKey: true, altKey: true });
  await settle();
  assert.ok(isOpen(w), 'the shortcut opens the primary secret in Large type');
  assert.deepStrictEqual(cells(w).map((c) => c.querySelector('.largetype-glyph').textContent), [...REVEAL]);

  key(w, w.document, 'Escape');
  assert.strictEqual(w.document.activeElement, ltBtn(w, 'password'), 'focus lands on the field button');
});

test('the detail menu offers Move to Recently Deleted and Undo restores the item', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();

  const trigger = q(w, '#detail .menu-wrap > button');
  assert.ok(trigger, 'the detail header has a More menu');
  trigger.click();
  const menu = q(w, '#detail .menu');
  assert.ok(!menu.classList.contains('hidden'));
  assert.strictEqual(trigger.getAttribute('aria-expanded'), 'true');
  assert.strictEqual(w.document.activeElement, menu.querySelector('button'), 'the first menu item takes focus');

  // Keyboard: arrows move inside the menu; Esc closes it and returns to the trigger.
  key(w, w.document.activeElement, 'ArrowDown');
  assert.strictEqual(w.document.activeElement, menu.querySelectorAll('button')[1]);
  key(w, w.document.activeElement, 'Escape');
  assert.ok(menu.classList.contains('hidden'));
  assert.strictEqual(trigger.getAttribute('aria-expanded'), 'false');
  assert.strictEqual(w.document.activeElement, trigger);

  trigger.click();
  const move = [...menu.querySelectorAll('button')].find((b) => b.textContent.includes('Move to Recently Deleted'));
  assert.ok(move, 'the menu has Move to Recently Deleted');
  move.click();
  await settle();

  assert.ok(sent.some((m) => m.type === 'popup:trash_item' && m.item_id === 'v1:i1'), 'the popup asks the host to trash the item');
  const titles = [...w.document.querySelectorAll('#listInner .row .t')].map((e) => e.textContent);
  assert.ok(!titles.includes('GitHub'), 'the trashed item leaves the list');
  assert.ok(titles.includes('GitLab'), 'the next item stays');

  const toastEl = q(w, '#toast');
  assert.ok(!toastEl.classList.contains('hidden'), 'the undo toast is visible');
  assert.match(toastEl.textContent, /Moved to Recently Deleted/);
  const undo = toastEl.querySelector('button.toast-action');
  assert.strictEqual(undo.textContent, 'Undo');

  undo.click();
  await settle();
  assert.ok(sent.some((m) => m.type === 'popup:restore_item' && m.item_id === 'v1:i1'), 'Undo asks the host to restore');
  assert.strictEqual(toastEl.textContent, 'Item restored');
  const back = [...w.document.querySelectorAll('#listInner .row .t')].map((e) => e.textContent);
  assert.ok(back.includes('GitHub'), 'the restored item is back in the list');
  assert.strictEqual(q(w, '#detail h2').textContent, 'GitHub', 'the restored item is selected again');
});
