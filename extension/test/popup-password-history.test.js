// Unit tests for password history on login items in the extension popup (issue #354).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = [
  'lib/i18n.js',
  'lib/icons.js',
  'popup/icons.js',
  'popup/view.js',
  'popup/newitem.js',
  'popup/generator.js',
  'popup/skeletons.js',
  'popup/totp-list.js',
  'popup/detail.js',
  'popup/edit.js',
  'popup/largetype.js',
  'popup/quick-actions.js',
  'popup/panels.js',
  'popup/settings.js',
  'popup/popup.js',
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(50); await sleep(50); };
const q = (w, sel) => w.document.querySelector(sel);

const ITEMS = [
  {
    id: 'v1:i1',
    vault_id: 'v1',
    vault_name: 'Personal',
    category: 'login',
    title: 'GitHub',
    subtitle: 'octo@example.invalid',
    url: 'https://github.com',
    favorite: false,
    has_totp: false,
  },
];

function popup({ passwordHistory = [] } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  let copiedText = null;
  w.navigator.clipboard = {
    writeText: (txt) => {
      copiedText = txt;
      return Promise.resolve();
    },
  };

  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status':
        return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults':
        return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items':
        return ok({ items: ITEMS });
      case 'match_active':
        return ok({ items: ITEMS, origin: 'https://github.com' });
      case 'get_item': {
        const it = ITEMS[0];
        const fields = [
          { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
          { name: 'password', label: 'password', kind: 'password', concealed: true },
        ];
        return ok({
          item: {
            ...it,
            fields,
            password_history: passwordHistory,
            notes: '',
            tags: [],
          },
        });
      }
      case 'reveal': {
        if (msg.field === 'password_history:0') {
          return ok({ value: 'FAKE-OLD-PW-0' });
        }
        return ok({ value: 'FAKE-CURRENT-PW' });
      }
      default:
        return ok({});
    }
  };

  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent, getCopied: () => copiedText };
}

test('login detail without password history does not show disclosure', async (t) => {
  const { dom, w } = popup({ passwordHistory: [] });
  t.after(() => dom.window.close());
  await settle();

  const toggle = q(w, '[data-action="password-history-toggle"]');
  assert.strictEqual(toggle, null, 'no toggle when history is empty');
});

test('login detail with password history shows disclosure, reveal and copy', async (t) => {
  const history = [
    { replaced_at: '2026-10-25T12:00:00Z' },
  ];
  const { dom, w, sent, getCopied } = popup({ passwordHistory: history });
  t.after(() => dom.window.close());
  await settle();

  const toggle = q(w, '[data-action="password-history-toggle"]');
  assert.ok(toggle, 'toggle exists');
  assert.strictEqual(toggle.getAttribute('aria-expanded'), 'false');
  assert.ok(toggle.textContent.includes('Password history (1)'));

  const list = q(w, '.pw-history-list');
  assert.ok(list);
  assert.strictEqual(list.style.display, 'none');

  // Click toggle to open disclosure
  toggle.click();
  assert.strictEqual(toggle.getAttribute('aria-expanded'), 'true');
  assert.strictEqual(list.style.display, 'flex');

  // Verify row rendered with masked text
  const valEl = q(w, '.pw-history-val');
  assert.ok(valEl);
  assert.strictEqual(valEl.textContent, '••••••••••');

  // Click reveal button
  const revealBtn = q(w, '[data-action="reveal-history"]');
  assert.ok(revealBtn);
  revealBtn.click();
  await settle();

  assert.ok(sent.some((m) => m.type === 'popup:reveal' && m.field === 'password_history:0'));
  assert.strictEqual(valEl.textContent, 'FAKE-OLD-PW-0');

  // Click copy button
  const copyBtn = q(w, '[data-action="copy-history"]');
  assert.ok(copyBtn);
  copyBtn.click();
  await settle();

  assert.strictEqual(w.__gitpassMockClipboard, 'FAKE-OLD-PW-0');
});
