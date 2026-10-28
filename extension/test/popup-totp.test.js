// Unit tests for one-time codes in the popup list and detail pane (issue #62).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/popup.js'];

const item = (id, title, hasTotp) => ({
  id, vault_id: 'v1', vault_name: 'Personal', category: 'login', title, subtitle: 'me@example.invalid',
  url: 'https://github.com', favorite: false, has_totp: hasTotp,
});
const ITEMS = [item('v1:i1', 'GitHub', true), item('v1:i2', 'Plain login', false)];

// Popup DOM with a fake host (no chrome.*, no real vault). `totp` decides the host's `totp` reply per item id.
function popup({ items = ITEMS, totp = () => ({ code: '123456', remaining: 20, period: 30 }) } = {}) {
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
        const it = items.find((x) => x.id === msg.item_id);
        const fields = [
          { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
          { name: 'password', label: 'password', kind: 'password', concealed: true },
        ];
        if (it.has_totp) fields.push({ name: 'totp', label: 'one-time password', kind: 'totp', concealed: true });
        return ok({ item: { ...it, fields, tags: [] } });
      }
      case 'reveal': return ok({ value: 'FAKE-PW' });
      case 'totp': return ok(totp(msg.item_id));
      default: return ok({});
    }
  };
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w, sent, setTotp: (fn) => { totp = fn; } };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); };
const rowCode = (w, id) => w.document.querySelector(`#listInner [data-totp-id="${id}"] .totp-code`);
const rowRing = (w, id) => w.document.querySelector(`#listInner [data-totp-id="${id}"] .ring`);

test('list rows show the formatted code + ring; the popup only asks for the code, never a secret', async (t) => {
  const { dom, w, sent } = popup();
  t.after(() => dom.window.close());
  await settle();

  assert.strictEqual(rowCode(w, 'v1:i1').textContent, '123 456', 'row shows the split code');
  assert.ok(rowRing(w, 'v1:i1'), 'row has a countdown ring');
  assert.strictEqual(w.document.querySelector('#detail .totp .totp-code').textContent, '123 456', 'detail shows the same code');
  assert.strictEqual(rowCode(w, 'v1:i2'), null, 'a login without a TOTP secret shows no code');

  const totpCalls = sent.filter((m) => m.type === 'popup:totp');
  assert.deepStrictEqual([...new Set(totpCalls.map((m) => m.item_id))], ['v1:i1'], 'only the TOTP login is asked for a code');
  assert.ok(!sent.some((m) => m.type === 'popup:reveal'), 'rendering a code must not reveal a secret');
  assert.ok(!sent.some((m) => /secret|field/i.test(JSON.stringify(m))), 'no host request may carry a secret or field name');
  for (const m of totpCalls) assert.deepStrictEqual(Object.keys(m).sort(), ['item_id', 'type'], 'the code request carries only the item id');
});

test('the ring turns danger in the last 5 seconds only', async (t) => {
  const items = [item('v1:i1', 'Almost rolled over', true), item('v1:i2', 'Fresh code', true)];
  const { dom, w } = popup({ items, totp: (id) => ({ code: '123456', remaining: id === 'v1:i1' ? 5 : 20, period: 30 }) });
  t.after(() => dom.window.close());
  await settle();

  assert.ok(rowRing(w, 'v1:i1').classList.contains('low'), '5 s remaining is the danger state');
  assert.ok(!rowRing(w, 'v1:i2').classList.contains('low'), '20 s remaining is the normal state');
  assert.ok(Number(rowRing(w, 'v1:i1').querySelector('.fg').getAttribute('stroke-dashoffset')) > 0, 'the ring is partly drained');
});

test('clicking a list code copies it without changing the selection', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();
  w.document.querySelectorAll('#listInner .row')[1].click(); // select Plain login
  await settle();
  assert.strictEqual(w.document.querySelector('#detail h2').textContent, 'Plain login');

  w.document.querySelector('#listInner [data-totp-id="v1:i1"]').dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
  await settle();

  assert.strictEqual(w.__gitpassMockClipboard, '123456');
  assert.strictEqual(w.document.getElementById('toast').textContent, 'Copied');
  assert.ok(!w.document.getElementById('toast').classList.contains('hidden'));
  assert.strictEqual(w.document.querySelector('#detail h2').textContent, 'Plain login', 'clicking the code does not select the row');
});

test('Cmd+Shift+C / Ctrl+Shift+C copies the selected row code; Cmd+C still copies the password', async (t) => {
  const { dom, w } = popup();
  t.after(() => dom.window.close());
  await settle();

  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'C', metaKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  await settle();
  assert.strictEqual(w.__gitpassMockClipboard, '123456', 'Cmd+Shift+C copies the TOTP code');

  w.__gitpassMockClipboard = '';
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'C', ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  await settle();
  assert.strictEqual(w.__gitpassMockClipboard, '123456', 'Ctrl+Shift+C copies the TOTP code');

  w.__gitpassMockClipboard = '';
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'c', metaKey: true, bubbles: true, cancelable: true }));
  await settle();
  assert.strictEqual(w.__gitpassMockClipboard, 'FAKE-PW', 'plain Cmd+C keeps copying the password');
});

test('the list code refreshes when the period rolls over without reopening the popup', async (t) => {
  const t0 = Date.now();
  const { dom, w } = popup({
    totp: () => (Date.now() - t0 < 900
      ? { code: '111222', remaining: 1, period: 30 }
      : { code: '333444', remaining: 30, period: 30 }),
  });
  t.after(() => dom.window.close());
  await settle();
  assert.strictEqual(rowCode(w, 'v1:i1').textContent, '111 222');

  await sleep(2000);
  assert.strictEqual(rowCode(w, 'v1:i1').textContent, '333 444', 'the code refreshes at rollover');
});

test('a host error shows a short inline error, not a broken row', async (t) => {
  const { dom, w } = popup({ totp: () => { throw Object.assign(new Error('vault is locked'), { code: 'locked' }); } });
  t.after(() => dom.window.close());
  await settle();
  assert.strictEqual(rowCode(w, 'v1:i1').textContent, '—');
  assert.strictEqual(rowRing(w, 'v1:i1').classList.contains('hidden'), true);
});
