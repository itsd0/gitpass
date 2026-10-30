// Inline edit for secure notes and API credentials (issue #309).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const ITEMS = [
  { id: 'v1:n1', vault_id: 'v1', vault_name: 'Personal', category: 'secure_note', title: 'Recovery codes', subtitle: '', favorite: false, has_totp: false, notes: 'FAKE-note-1111\nFAKE-note-2222', tags: ['demo', 'home'] },
  { id: 'v1:n2', vault_id: 'v1', vault_name: 'Personal', category: 'secure_note', title: 'Template note', subtitle: '', favorite: false, has_totp: false, notes: '', tags: ['dev'], fields: [
    { name: 'note', label: 'note', kind: 'multiline', concealed: false, value: 'FAKE-body' },
  ] },
  { id: 'v1:a1', vault_id: 'v1', vault_name: 'Work', category: 'api_credential', title: 'Stripe test', subtitle: 'sk-…', favorite: false, has_totp: false, notes: 'FAKE dashboard note', tags: [], fields: [
    { name: 'username', label: 'username', kind: 'text', concealed: false, value: 'demo-user' },
    { name: 'credential', label: 'credential', kind: 'password', concealed: true },
    { name: 'hostname', label: 'hostname', kind: 'url', concealed: false, value: 'https://api.example.invalid' },
    { name: 'expires', label: 'expires', kind: 'date', concealed: false, value: '2026-01-01' },
  ] },
  { id: 'v1:c1', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: false, has_totp: false },
];

function popup({ reveal = 'FAKE-KEY-123' } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    const it = ITEMS.find((x) => x.id === msg.item_id) || ITEMS[0];
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: ITEMS });
      case 'match_active': return ok({ items: ITEMS, origin: 'https://example.invalid' });
      case 'get_item': return ok({ item: { ...it, fields: it.fields || [], notes: it.notes || '', tags: it.tags || [] } });
      case 'reveal': return ok({ value: reveal });
      case 'update_login': return ok({ ok: true });
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
const reveals = (sent) => sent.filter((m) => m.type === 'popup:reveal');

async function selectTitle(w, title) {
  await settle();
  const row = [...w.document.querySelectorAll('#listInner .row')].find((r) => r.querySelector('.t') && r.querySelector('.t').textContent === title);
  assert.ok(row, 'row ' + title);
  row.click();
  await settle();
}

async function openEdit(w, title) {
  await settle();
  if (title) await selectTitle(w, title);
  const b = editBtn(w);
  assert.ok(b, 'Edit button exists');
  b.click();
  await settle();
  assert.ok(form(w), 'inline edit form rendered');
}

test('secure note edit shows title, notes and tags and saves them', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await openEdit(w, 'Recovery codes');

  assert.strictEqual(q(w, '#detail h2').textContent, 'Edit secure note');
  assert.strictEqual(form(w).getAttribute('aria-label'), 'Edit secure note');
  assert.match(q(w, '#detail .sub').textContent, /Personal/);
  assert.match(q(w, '#detail .sub').textContent, /Secure Notes/);
  assert.strictEqual(field(w, 'edit-title').value, 'Recovery codes');
  assert.strictEqual(field(w, 'edit-notes').value, 'FAKE-note-1111\nFAKE-note-2222');
  assert.strictEqual(field(w, 'edit-tags').value, 'demo, home');
  assert.strictEqual(field(w, 'edit-tags').placeholder, 'Comma separated');
  assert.strictEqual(w.document.activeElement, field(w, 'edit-title'));
  assert.ok(q(w, '#detail label[for="edit-title"]'));
  assert.ok(q(w, '#detail label[for="edit-notes"]'));
  assert.ok(q(w, '#detail label[for="edit-tags"]'));
  assert.ok(q(w, '#detail .edit-acts .btn-primary'), 'Save is the primary action');
  assert.strictEqual(q(w, '#detail .edit-acts .btn-link').textContent, 'Cancel');

  field(w, 'edit-title').value = '   ';
  form(w).requestSubmit();
  await settle();
  assert.strictEqual(q(w, '#detail .err').textContent, 'Title is required');
  assert.strictEqual(updates(sent).length, 0);

  field(w, 'edit-title').value = 'Recovery codes (edited)';
  field(w, 'edit-notes').value = 'FAKE edited note';
  field(w, 'edit-tags').value = ' demo, ,home, demo ';
  field(w, 'edit-tags').dispatchEvent(new w.Event('input', { bubbles: true }));
  form(w).requestSubmit();
  await settle();
  await settle();
  const msg = updates(sent).pop();
  assert.ok(msg);
  assert.strictEqual(msg.item_id, 'v1:n1');
  assert.strictEqual(msg.title, 'Recovery codes (edited)');
  assert.strictEqual(msg.notes, 'FAKE edited note');
  assert.deepStrictEqual([...msg.tags], ['demo', 'home']);
  assert.ok(!('note_field' in msg));
  assert.strictEqual(q(w, '#toast').textContent, 'Saved');
  assert.strictEqual(form(w), null);
  assert.strictEqual(w.document.activeElement, editBtn(w));
});

test('a template secure note saves the note body field', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await openEdit(w, 'Template note');
  assert.strictEqual(field(w, 'edit-notes').value, 'FAKE-body');
  field(w, 'edit-notes').value = 'FAKE-body-edited';
  form(w).requestSubmit();
  await settle();
  await settle();
  const msg = updates(sent).pop();
  assert.strictEqual(msg.note_field, 'note');
  assert.strictEqual(msg.notes, 'FAKE-body-edited');
  assert.deepStrictEqual([...msg.tags], ['dev']);
});

test('Esc and navigation use the same discard guard for a secure note', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await openEdit(w, 'Recovery codes');
  key(w, 'Escape');
  await settle();
  assert.strictEqual(form(w), null, 'clean Esc leaves edit mode');
  assert.strictEqual(w.document.activeElement, editBtn(w));

  await openEdit(w, 'Recovery codes');
  field(w, 'edit-notes').value = 'changed';
  field(w, 'edit-notes').dispatchEvent(new w.Event('input', { bubbles: true }));
  let asked = 0;
  w.confirm = () => { asked++; return false; };
  key(w, 'Escape');
  await settle();
  assert.strictEqual(asked, 1);
  assert.ok(form(w), 'declining keeps the form');

  w.confirm = () => false;
  await selectTitle(w, 'Stripe test');
  assert.ok(form(w), 'declining keeps editing the note');
  assert.strictEqual(q(w, '#detail h2').textContent, 'Edit secure note');

  w.confirm = () => true;
  key(w, 'Escape');
  await settle();
  assert.strictEqual(form(w), null);
  assert.strictEqual(q(w, '#detail h2').textContent, 'Recovery codes');

  await selectTitle(w, 'Visa');
  assert.strictEqual(editBtn(w), null, 'credit cards stay read-only');
  key(w, 'e', { metaKey: true });
  await settle();
  assert.strictEqual(form(w), null, '⌘E does not edit a credit card');
});

test('⌘E opens an API credential form and the credential stays masked until Show', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await selectTitle(w, 'Stripe test');
  const before = reveals(sent).length;
  key(w, 'e', { metaKey: true });
  await settle();
  assert.strictEqual(q(w, '#detail h2').textContent, 'Edit API credential');
  assert.strictEqual(form(w).getAttribute('aria-label'), 'Edit API credential');
  assert.match(q(w, '#detail .sub').textContent, /Work/);
  assert.match(q(w, '#detail .sub').textContent, /API Credentials/);
  assert.strictEqual(field(w, 'edit-title').value, 'Stripe test');
  assert.strictEqual(field(w, 'edit-username').value, 'demo-user');
  assert.strictEqual(field(w, 'edit-hostname').value, 'https://api.example.invalid');
  assert.strictEqual(field(w, 'edit-expires').value, '2026-01-01');
  assert.strictEqual(field(w, 'edit-notes').value, 'FAKE dashboard note');
  const cred = field(w, 'edit-credential');
  assert.strictEqual(cred.type, 'password');
  assert.strictEqual(cred.value, '', 'credential is not fetched when the form opens');
  assert.strictEqual(reveals(sent).length, before, 'no reveal until Show or an edit');
  const show = q(w, '#detail [data-action="show-credential"]');
  assert.strictEqual(show.type, 'button');
  assert.strictEqual(show.getAttribute('aria-pressed'), 'false');
  assert.strictEqual(show.getAttribute('aria-label'), 'Show credential');
  assert.ok(q(w, '#detail label[for="edit-credential"]'));
  assert.ok(q(w, '#detail label[for="edit-hostname"]'));
  assert.ok(q(w, '#detail label[for="edit-expires"]'));

  show.click();
  await settle();
  assert.strictEqual(reveals(sent).at(-1).field, 'credential');
  assert.strictEqual(reveals(sent).at(-1).item_id, 'v1:a1');
  assert.strictEqual(cred.type, 'text');
  assert.strictEqual(cred.value, 'FAKE-KEY-123');
  assert.strictEqual(show.getAttribute('aria-pressed'), 'true');
  assert.strictEqual(show.getAttribute('aria-label'), 'Hide credential');

  show.click();
  await settle();
  assert.strictEqual(cred.type, 'password', 'Hide masks the credential again');
  assert.strictEqual(cred.value, 'FAKE-KEY-123');
  assert.strictEqual(reveals(sent).filter((m) => m.field === 'credential').length, 1, 'the secret is fetched once');

  field(w, 'edit-username').value = 'octo@example.invalid';
  key(w, 'Enter', { metaKey: true });
  await settle();
  await settle();
  const msg = updates(sent).pop();
  assert.strictEqual(msg.item_id, 'v1:a1');
  assert.strictEqual(msg.title, 'Stripe test');
  assert.strictEqual(msg.username, 'octo@example.invalid');
  assert.strictEqual(msg.credential, 'FAKE-KEY-123', 'revealed credential is sent back');
  assert.strictEqual(msg.hostname, 'https://api.example.invalid');
  assert.strictEqual(msg.expires, '2026-01-01');
  assert.strictEqual(msg.notes, 'FAKE dashboard note');
  assert.strictEqual(form(w), null);
});

test('an untouched API credential is omitted; typing it sends the new value without a reveal', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await openEdit(w, 'Stripe test');
  form(w).requestSubmit();
  await settle();
  await settle();
  let msg = updates(sent).pop();
  assert.ok(msg);
  assert.ok(!('credential' in msg), 'untouched credential is not sent');
  assert.strictEqual(reveals(sent).filter((m) => m.field === 'credential').length, 0);

  await openEdit(w, 'Stripe test');
  const cred = field(w, 'edit-credential');
  cred.value = 'FAKE-typed-key';
  cred.dispatchEvent(new w.Event('input', { bubbles: true }));
  form(w).requestSubmit();
  await settle();
  await settle();
  msg = updates(sent).pop();
  assert.strictEqual(msg.credential, 'FAKE-typed-key');
  assert.strictEqual(reveals(sent).filter((m) => m.field === 'credential').length, 0);
});

test('API credential cancel and invalid hostname or expiry send nothing', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await openEdit(w, 'Stripe test');
  q(w, '#detail .edit-acts .btn-link').click();
  await settle();
  assert.strictEqual(form(w), null, 'Cancel closes a clean form');
  assert.strictEqual(updates(sent).length, 0);

  await openEdit(w, 'Stripe test');
  field(w, 'edit-hostname').value = 'ftp://example.invalid';
  form(w).requestSubmit();
  await settle();
  assert.strictEqual(q(w, '#detail .err').textContent, 'Website must be an http(s) URL');
  assert.ok(form(w));
  assert.strictEqual(updates(sent).length, 0);

  field(w, 'edit-hostname').value = 'https://api.example.invalid';
  // A date input drops values the browser rejects; the text path is what a non-ISO expiry uses.
  field(w, 'edit-expires').type = 'text';
  field(w, 'edit-expires').value = '2026-13-01';
  form(w).requestSubmit();
  await settle();
  assert.strictEqual(q(w, '#detail .err').textContent, 'Expiry must be YYYY-MM-DD');
  assert.strictEqual(updates(sent).length, 0);

  field(w, 'edit-title').value = 'Stripe renamed';
  field(w, 'edit-title').dispatchEvent(new w.Event('input', { bubbles: true }));
  w.confirm = () => false;
  q(w, '#detail .edit-acts .btn-link').click();
  await settle();
  assert.ok(form(w), 'declining the discard guard keeps the form');
  w.confirm = () => true;
  q(w, '#detail .edit-acts .btn-link').click();
  await settle();
  assert.strictEqual(form(w), null);
});
