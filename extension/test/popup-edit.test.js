// Unit tests for inline login editing in the extension popup (issue #144).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/recent.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const ITEMS = [
  { id: 'v1:i1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'octo@example.invalid', url: 'https://github.com', favorite: false, has_totp: false },
  { id: 'v1:i2', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitLab', subtitle: 'git@example.invalid', url: 'https://gitlab.com', favorite: false, has_totp: false },
  { id: 'v1:i3', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: false, has_totp: false },
];

function popup({ updateLogin, reveal = 'FAKE-PW-123', items = ITEMS } = {}) {
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
      case 'match_active': return ok({ items, origin: 'https://github.com' });
      case 'get_item': {
        const it = items.find((x) => x.id === msg.item_id) || items[0];
        const fields = [];
        if (it.category === 'login') {
          fields.push({ name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle });
          fields.push({ name: 'password', label: 'password', kind: 'password', concealed: true });
          if (it.url) fields.push({ name: 'url', label: 'website', kind: 'url', concealed: false, value: it.url });
        }
        return ok({ item: { ...it, fields, notes: it.notes || '', tags: [] } });
      }
      case 'reveal': return ok({ value: reveal });
      case 'generate': return ok({ password: 'FAKE-GEN-123' });
      case 'update_login': return updateLogin ? updateLogin(msg) : ok({ ok: true });
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(50); await sleep(50); };
const q = (w, sel) => w.document.querySelector(sel);
const editBtn = (w) => q(w, '#detail [data-action="edit"]');
const form = (w) => q(w, '#detail form.inline-edit');
const field = (w, id) => q(w, '#detail #' + id);
const key = (w, k, mods = {}) => w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...mods }));
const updates = (sent) => sent.filter((m) => m.type === 'popup:update_login');

async function selectRow(w, idx) {
  w.document.querySelectorAll('#listInner .row')[idx].click();
  await settle();
}

async function openEdit(w) {
  await settle();
  const b = editBtn(w);
  assert.ok(b, 'Edit button exists for a login');
  b.click();
  await settle();
  assert.ok(form(w), 'inline edit form rendered');
}

test('Edit on a login opens the inline form with the stored values and focuses the title', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await openEdit(w);

  assert.strictEqual(field(w, 'edit-title').value, 'GitHub');
  assert.strictEqual(field(w, 'edit-username').value, 'octo@example.invalid');
  assert.strictEqual(field(w, 'edit-url-0').value, 'https://github.com');
  assert.strictEqual(field(w, 'edit-password').value, 'FAKE-PW-123', 'current password is revealed into the edit field');
  assert.strictEqual(w.document.activeElement, field(w, 'edit-title'), 'focus moves to the first field');
  assert.ok(q(w, '#detail .edit-acts .btn-primary'), 'Save is the primary action');

  // Non-login categories stay read-only.
  await selectRow(w, 2);
  assert.strictEqual(editBtn(w), null);
});

test('title is required and websites must be http(s); invalid input sends nothing', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await openEdit(w);

  field(w, 'edit-title').value = '   ';
  form(w).requestSubmit();
  await settle();
  assert.strictEqual(q(w, '#detail .err').textContent, 'Title is required');
  assert.strictEqual(w.document.activeElement, field(w, 'edit-title'));
  assert.strictEqual(updates(sent).length, 0, 'nothing sent for a missing title');

  field(w, 'edit-title').value = 'GitHub';
  field(w, 'edit-url-0').value = 'ftp://example.invalid';
  form(w).requestSubmit();
  await settle();
  assert.strictEqual(q(w, '#detail .err').textContent, 'Website must be an http(s) URL');
  assert.ok(field(w, 'edit-url-0').classList.contains('bad'));
  assert.strictEqual(updates(sent).length, 0);
});

test('Esc cancels a clean form and returns focus to Edit; a dirty form asks first', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await openEdit(w);
  key(w, 'Escape');
  await settle();
  assert.strictEqual(form(w), null, 'Esc leaves edit mode');
  assert.strictEqual(w.document.activeElement, editBtn(w), 'focus returns to the Edit button');

  await openEdit(w);
  field(w, 'edit-title').value = 'GitHub (work)';
  field(w, 'edit-title').dispatchEvent(new w.Event('input', { bubbles: true }));
  let asked = 0;
  w.confirm = () => { asked++; return false; };
  key(w, 'Escape');
  await settle();
  assert.strictEqual(asked, 1, 'discard confirmation shown');
  assert.ok(form(w), 'declining keeps the form open');

  w.confirm = () => { asked++; return true; };
  key(w, 'Escape');
  await settle();
  assert.strictEqual(asked, 2);
  assert.strictEqual(form(w), null);
});

test('⌘E opens edit and ⌘↩ saves title, username, websites and notes', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();
  key(w, 'e', { metaKey: true });
  await settle();
  assert.ok(form(w), '⌘E opens the form');

  field(w, 'edit-username').value = 'octo-edited@example.invalid';
  field(w, 'edit-username').dispatchEvent(new w.Event('input', { bubbles: true }));
  field(w, 'edit-notes').value = 'edited notes';
  key(w, 'Enter', { metaKey: true });
  await settle();
  await settle();

  const msg = updates(sent).pop();
  assert.ok(msg, 'update_login sent');
  assert.strictEqual(msg.item_id, 'v1:i1');
  assert.strictEqual(msg.title, 'GitHub');
  assert.strictEqual(msg.username, 'octo-edited@example.invalid');
  assert.strictEqual(msg.password, 'FAKE-PW-123', 'unchanged password is sent back');
  assert.deepStrictEqual([...msg.urls], ['https://github.com']);
  assert.strictEqual(msg.notes, 'edited notes');
  assert.strictEqual(q(w, '#toast').textContent, 'Saved');
  assert.strictEqual(form(w), null, 'form closes after save');
  assert.strictEqual(w.document.activeElement, editBtn(w), 'focus returns to Edit');
});

test('saving shows the spinner, host errors stay inline, and Save can retry', async (t) => {
  let calls = 0, rejectFirst;
  const { dom, w, sent } = popup({
    updateLogin: () => {
      calls++;
      if (calls > 1) return Promise.resolve({ ok: true });
      return new Promise((res, rej) => { rejectFirst = rej; });
    },
  });
  t.after(() => dom.window.close());
  await openEdit(w);

  field(w, 'edit-username').value = 'changed';
  form(w).requestSubmit();
  await sleep(10);
  let save = q(w, '#detail .edit-acts .btn-primary');
  assert.strictEqual(save.disabled, true, 'Save is disabled while pending');
  assert.ok(save.querySelector('.spinner'), 'Save shows the existing loading spinner');

  rejectFirst(Object.assign(new Error('vault is read-only'), { code: 'internal' }));
  await settle();
  assert.strictEqual(q(w, '#detail .err').textContent, 'vault is read-only');
  assert.ok(form(w), 'form stays open on error');
  save = q(w, '#detail .edit-acts .btn-primary');
  assert.strictEqual(save.disabled, false, 'Save is enabled again');
  assert.strictEqual(save.querySelector('.spinner'), null);

  form(w).requestSubmit();
  await settle();
  await settle();
  assert.strictEqual(updates(sent).length, 2);
  assert.strictEqual(form(w), null);
});

test('discard confirmation guards navigation to another item', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await openEdit(w);
  field(w, 'edit-title').value = 'Renamed';
  field(w, 'edit-title').dispatchEvent(new w.Event('input', { bubbles: true }));

  w.confirm = () => false;
  await selectRow(w, 1);
  assert.ok(form(w), 'declining keeps editing the same item');
  assert.strictEqual(q(w, '#detail h2').textContent, 'Edit login');
  assert.strictEqual(q(w, '#listInner .row.sel .t').textContent, 'GitHub');

  w.confirm = () => true;
  await selectRow(w, 1);
  assert.strictEqual(form(w), null, 'accepting discards and opens the other item');
  assert.strictEqual(q(w, '#detail h2').textContent, 'GitLab');
});

test('Generate opens the generator view and Use password fills the field', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await openEdit(w);

  const genBtn = [...w.document.querySelectorAll('#detail .edit-pw .icon-btn')].find((b) => b.title === 'Generate');
  genBtn.click();
  await settle();
  const code = q(w, '#panel .gen-code');
  assert.ok(code, 'generator panel opened from the edit form');
  assert.strictEqual(code.textContent, 'FAKE-GEN-123');
  const use = [...w.document.querySelectorAll('#panel button')].find((b) => b.textContent.trim() === 'Use password');
  assert.ok(use, 'Use password action is offered');
  use.click();
  await settle();
  assert.strictEqual(field(w, 'edit-password').value, 'FAKE-GEN-123');
  assert.strictEqual(q(w, '#panel').classList.contains('hidden'), true);
  assert.ok(form(w), 'form remains open after using the generated password');
});

test('password is omitted when reveal fails and the user leaves it untouched', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  // Simulate a host that cannot reveal (e.g. no stored password).
  w.__gitpassMock = ((orig) => (msg) => msg.type === 'popup:reveal'
    ? Promise.reject(Object.assign(new Error('no field'), { code: 'not_found' }))
    : orig(msg))(w.__gitpassMock);
  await openEdit(w);
  assert.strictEqual(field(w, 'edit-password').value, '');
  form(w).requestSubmit();
  await settle();
  const msg = updates(sent).pop();
  assert.ok(msg);
  assert.ok(!('password' in msg), 'untouched password is not sent');
});
