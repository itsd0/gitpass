// Unit and DOM tests for attachment metadata, paperclip indicator, and Open in gitpass in extension popup (issue #290).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/capslock.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];
const V = require('../popup/view.js');
const Icons = require('../popup/icons.js');

test('view.js: formatSize formats bytes, KB, and MB accurately', () => {
  assert.strictEqual(V.formatSize(0), '0 B');
  assert.strictEqual(V.formatSize(512), '512 B');
  assert.strictEqual(V.formatSize(1023), '1023 B');
  assert.strictEqual(V.formatSize(1024), '1.0 KB');
  assert.strictEqual(V.formatSize(1536), '1.5 KB');
  assert.strictEqual(V.formatSize(1048576), '1.0 MB');
  assert.strictEqual(V.formatSize(2621440), '2.5 MB');
});

test('icons.js: fileIcon maps file extensions and mime types to appropriate glyphs', () => {
  assert.strictEqual(Icons.fileIcon('photo.png', 'image/png'), Icons.UI.fileImage);
  assert.strictEqual(Icons.fileIcon('avatar.jpg', ''), Icons.UI.fileImage);
  assert.strictEqual(Icons.fileIcon('script.ts', 'text/typescript'), Icons.UI.fileCode);
  assert.strictEqual(Icons.fileIcon('config.json', 'application/json'), Icons.UI.fileCode);
  assert.strictEqual(Icons.fileIcon('document.pdf', 'application/pdf'), Icons.UI.fileText);
  assert.strictEqual(Icons.fileIcon('notes.txt', 'text/plain'), Icons.UI.fileText);
  assert.strictEqual(Icons.fileIcon('blob.bin', 'application/octet-stream'), Icons.UI.file);
});

const TEST_ITEMS = [
  {
    id: 'v1:item-with-att',
    vault_id: 'v1',
    vault_name: 'Personal',
    category: 'login',
    title: 'Acme Corp',
    subtitle: 'alice',
    url: 'https://acme.example.com',
    favorite: false,
    has_totp: false,
    has_attachments: true,
  },
  {
    id: 'v1:item-no-att',
    vault_id: 'v1',
    vault_name: 'Personal',
    category: 'login',
    title: 'Beta Services',
    subtitle: 'bob',
    url: 'https://beta.example.com',
    favorite: false,
    has_totp: false,
    has_attachments: false,
  },
];

function setupAttachmentsPopup() {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];

  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Test User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: TEST_ITEMS });
      case 'match_active': return ok({ items: TEST_ITEMS, origin: 'https://acme.example.com' });
      case 'get_item': {
        const it = TEST_ITEMS.find((x) => x.id === msg.item_id) || TEST_ITEMS[0];
        const attachments = it.has_attachments ? [
          { id: 'att-1', name: 'employment-contract.pdf', size: 1048576, mime: 'application/pdf' },
          { id: 'att-2', name: 'badge-photo.png', size: 204800, mime: 'image/png' },
        ] : [];
        return ok({
          item: {
            ...it,
            fields: [
              { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
              { name: 'password', label: 'password', kind: 'password', concealed: true },
            ],
            attachments,
            notes: 'Test notes',
            tags: [],
          },
        });
      }
      case 'open_in_app': return ok({ opened: true });
      default: return ok({});
    }
  };

  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent };
}

test('popup list rows show paperclip indicator for items with attachments only', async () => {
  const { dom, w } = setupAttachmentsPopup();
  await new Promise((r) => setTimeout(r, 100));

  const rows = dom.window.document.querySelectorAll('#listInner .row');
  assert.strictEqual(rows.length, 2);

  // Row 0 has attachments
  const attIcon0 = rows[0].querySelector('.att');
  assert.ok(attIcon0, 'Row 0 must render .att indicator');
  assert.strictEqual(attIcon0.getAttribute('aria-label'), 'Has attachments');
  assert.ok(attIcon0.innerHTML.includes('svg'), 'Indicator contains paperclip SVG');

  // Row 1 has no attachments
  const attIcon1 = rows[1].querySelector('.att');
  assert.strictEqual(attIcon1, null, 'Row 1 must not render .att indicator');
});

test('popup item detail renders Attachments section with files and Open in gitpass action', async () => {
  const { dom, w, sent } = setupAttachmentsPopup();
  await new Promise((r) => setTimeout(r, 120));

  // Detail is loaded for Row 0 (item with attachments)
  const sec = dom.window.document.querySelector('.d-attachments');
  assert.ok(sec, '.d-attachments section must exist');
  assert.strictEqual(sec.getAttribute('role'), 'region');

  const title = sec.querySelector('.d-attachments-title');
  assert.strictEqual(title.textContent, 'Attachments');

  const attRows = sec.querySelectorAll('.att-row');
  assert.strictEqual(attRows.length, 2);

  // Check first attachment row: name, title tooltip, formatted size, and Open in gitpass button
  const name0 = attRows[0].querySelector('.att-name');
  assert.strictEqual(name0.textContent, 'employment-contract.pdf');
  assert.strictEqual(name0.title, 'employment-contract.pdf');
  const size0 = attRows[0].querySelector('.att-size');
  assert.strictEqual(size0.textContent, '1.0 MB');

  const openBtn = attRows[0].querySelector('.att-open-btn');
  assert.ok(openBtn, 'Open in gitpass button must be present');
  assert.ok(openBtn.textContent.includes('Open in gitpass'));

  // Click Open in gitpass
  openBtn.click();
  assert.ok(sent.some((m) => m.type === 'popup:open_in_app' && m.item_id === 'v1:item-with-att'),
    'Clicking Open in gitpass must send popup:open_in_app with item_id');
});

test('popup item detail: Enter key on attachment row triggers open_in_app', async () => {
  const { dom, w, sent } = setupAttachmentsPopup();
  await new Promise((r) => setTimeout(r, 120));

  const sec = dom.window.document.querySelector('.d-attachments');
  assert.ok(sec);
  const attRows = sec.querySelectorAll('.att-row');
  assert.ok(attRows.length > 0);

  const initialCalls = sent.filter((m) => m.type === 'popup:open_in_app').length;
  const enterEvent = new dom.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true });
  attRows[0].dispatchEvent(enterEvent);
  const afterCalls = sent.filter((m) => m.type === 'popup:open_in_app').length;
  assert.strictEqual(afterCalls, initialCalls + 1, 'Enter key on attachment row must trigger open_in_app');
});

test('popup item detail empty case: no Attachments section when item has no attachments', async () => {
  const { dom, w } = setupAttachmentsPopup();
  await new Promise((r) => setTimeout(r, 120));

  const row1 = dom.window.document.querySelectorAll('#listInner .row')[1];
  assert.ok(row1);
  row1.click();
  await new Promise((r) => setTimeout(r, 120));

  const detailTitle = dom.window.document.querySelector('#detail h2');
  assert.strictEqual(detailTitle.textContent, 'Beta Services');

  const sec = dom.window.document.querySelector('.d-attachments');
  assert.strictEqual(sec, null, 'Items without attachments must not render .d-attachments section');
});
