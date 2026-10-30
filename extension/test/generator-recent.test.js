// Popup generator "Recent" section (issue #226): masking, reveal, copy, fill gating, empty, locked and setting-off states.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const GEN = require('../popup/generator.js');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];
const ORIGIN = 'https://example.invalid/login';
const NOW = Math.floor(Date.now() / 1000);
const ENTRIES = [
  { id: 'h1', value: 'FAKE-Recent-Pass-1234', created_at: NOW - 120, source: 'extension', host: 'example.invalid' },
  { id: 'h2', value: 'FAKE-Recent-Pass-5678', created_at: NOW - 3 * 3600, source: 'extension', host: 'other.example.invalid' },
  { id: 'h3', value: 'short', created_at: NOW - 2 * 86400, source: 'desktop' },
];

function popup({ history = { enabled: true, entries: ENTRIES }, historyError, canFill = true } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  w.__gitpassMockClipboard = '';
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: [] });
      case 'match_active': return ok({ items: [], origin: ORIGIN });
      case 'can_fill_generated': return ok({ canFill });
      case 'fill_generated': return ok({ filled: true });
      case 'generate': return ok({ password: 'FAKE-Gen-Abc-123' });
      case 'generator_history':
        if (historyError) return Promise.reject(Object.assign(new Error(historyError), { code: historyError }));
        return ok(history);
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { w, sent };
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(30); await sleep(30); };
const q = (w, sel) => w.document.querySelector(sel);
const qa = (w, sel) => [...w.document.querySelectorAll(sel)];
async function openRecent(w) {
  q(w, '#genBtn').click(); await settle(); await settle();
  q(w, '.gen-recent-toggle').click(); await settle(); await settle();
}
const key = (el, k) => el.dispatchEvent(new el.ownerDocument.defaultView.KeyboardEvent('keydown', { key: k, bubbles: true }));

test('helpers: masking keeps the last 4 characters of long values only', () => {
  assert.strictEqual(GEN.maskValue('FAKE-Recent-Pass-1234'), '\u2022'.repeat(8) + '1234');
  assert.strictEqual(GEN.maskValue('short'), '\u2022'.repeat(8));
  assert.strictEqual(GEN.maskValue(''), '\u2022'.repeat(8));
});

test('helpers: relative age buckets and clock skew', () => {
  assert.deepStrictEqual(GEN.ageParts(100, 130), { key: 'genAgoNow', n: 0 });
  assert.deepStrictEqual(GEN.ageParts(0, 125), { key: 'genAgoMin', n: 2 });
  assert.deepStrictEqual(GEN.ageParts(0, 3 * 3600 + 5), { key: 'genAgoHour', n: 3 });
  assert.deepStrictEqual(GEN.ageParts(0, 2 * 86400), { key: 'genAgoDay', n: 2 });
  assert.deepStrictEqual(GEN.ageParts(500, 100), { key: 'genAgoNow', n: 0 });
});

test('helpers: fill only when the entry host is the tab host (www ignored)', () => {
  assert.ok(GEN.hostMatches('example.invalid', 'https://www.example.invalid/login'));
  assert.ok(!GEN.hostMatches('other.example.invalid', 'https://example.invalid/'));
  assert.ok(!GEN.hostMatches('', 'https://example.invalid/'));
  assert.ok(!GEN.hostMatches('example.invalid', null));
  assert.ok(!GEN.hostMatches('example.invalid', 'not a url'));
});

test('helpers: cleanHistory sorts newest first, caps and drops bad rows', () => {
  const r = GEN.cleanHistory({ entries: [{ id: 'a', value: 'x', created_at: 1 }, null, { id: 'b', value: 'y', created_at: 9 }, { id: 'c', value: 5 }] }, 10);
  assert.deepStrictEqual(r.entries.map((e) => e.id), ['b', 'a']);
  assert.strictEqual(GEN.cleanHistory({ entries: ENTRIES }, 2).entries.length, 2);
  assert.deepStrictEqual(GEN.cleanHistory({ enabled: false, entries: ENTRIES }, 10), { enabled: false, entries: [] });
});

test('collapsed by default and history is not fetched until expanded', async () => {
  const { w, sent } = popup();
  await settle();
  q(w, '#genBtn').click(); await settle(); await settle();
  const toggle = q(w, '.gen-recent-toggle');
  assert.strictEqual(toggle.textContent.trim(), 'Recent');
  assert.strictEqual(toggle.getAttribute('aria-expanded'), 'false');
  assert.ok(q(w, '.gen-recent-body').classList.contains('hidden'));
  assert.ok(!sent.some((m) => m.type === 'popup:generator_history'));
  toggle.click(); await settle(); await settle();
  assert.strictEqual(toggle.getAttribute('aria-expanded'), 'true');
  assert.strictEqual(sent.filter((m) => m.type === 'popup:generator_history').length, 1);
});

test('rows are masked with host and relative time; reveal is per row', async () => {
  const { w } = popup();
  await settle();
  await openRecent(w);
  const rows = qa(w, '.gen-recent-row');
  assert.strictEqual(rows.length, 3);
  const text = rows.map((r) => r.textContent);
  assert.match(text[0], /^\u2022{8}1234example\.invalid · 2 min ago/);
  assert.match(text[1], /other\.example\.invalid · 3 h ago/);
  assert.match(text[2], /2 d ago/);
  assert.ok(!w.document.body.textContent.includes('FAKE-Recent-Pass-1234'), 'no full value while masked');
  rows[0].querySelector('button[title="Reveal"]').click(); await settle();
  assert.strictEqual(rows[0].querySelector('.gen-recent-val').textContent, 'FAKE-Recent-Pass-1234');
  assert.match(rows[1].querySelector('.gen-recent-val').textContent, /^\u2022{8}5678$/, 'other rows stay masked');
  rows[0].querySelector('button[title="Hide"]').click(); await settle();
  assert.match(rows[0].querySelector('.gen-recent-val').textContent, /^\u2022{8}1234$/);
});

test('copy uses the popup clipboard path with the full value', async () => {
  const { w } = popup();
  await settle();
  await openRecent(w);
  qa(w, '.gen-recent-row')[1].querySelector('button[title="Copy"]').click(); await settle();
  assert.strictEqual(w.__gitpassMockClipboard, 'FAKE-Recent-Pass-5678');
});

test('fill appears only for entries from the current site and fills that value', async () => {
  const { w, sent } = popup();
  await settle();
  await openRecent(w);
  const rows = qa(w, '.gen-recent-row');
  assert.ok(rows[0].querySelector('button[title="Fill on this page"]'));
  assert.ok(!rows[1].querySelector('button[title="Fill on this page"]'), 'different host');
  assert.ok(!rows[2].querySelector('button[title="Fill on this page"]'), 'no host');
  w.close = () => {};
  rows[0].querySelector('button[title="Fill on this page"]').click(); await settle();
  const f = sent.find((m) => m.type === 'popup:fill_generated' && m.password === 'FAKE-Recent-Pass-1234');
  assert.ok(f, 'fill_generated carries the entry value');
});

test('no fill buttons when the page has no fillable password field', async () => {
  const { w } = popup({ canFill: false });
  await settle();
  await openRecent(w);
  assert.strictEqual(qa(w, '.gen-recent-row button[title="Fill on this page"]').length, 0);
});

test('keyboard: arrows move between rows, Enter copies, R reveals', async () => {
  const { w } = popup();
  await settle();
  await openRecent(w);
  const rows = qa(w, '.gen-recent-row');
  assert.deepStrictEqual(rows.map((r) => r.tabIndex), [0, -1, -1]);
  rows[0].focus();
  key(rows[0], 'ArrowDown');
  assert.strictEqual(w.document.activeElement, rows[1]);
  assert.deepStrictEqual(rows.map((r) => r.tabIndex), [-1, 0, -1]);
  key(rows[1], 'ArrowUp');
  assert.strictEqual(w.document.activeElement, rows[0]);
  key(rows[0], 'ArrowUp');
  assert.strictEqual(w.document.activeElement, rows[0], 'stops at the first row');
  key(rows[0], 'r'); await settle();
  assert.strictEqual(rows[0].querySelector('.gen-recent-val').textContent, 'FAKE-Recent-Pass-1234');
  key(rows[0], 'Enter'); await settle();
  assert.strictEqual(w.__gitpassMockClipboard, 'FAKE-Recent-Pass-1234');
});

test('View all opens the app', async () => {
  const { w, sent } = popup();
  await settle();
  await openRecent(w);
  const link = q(w, '.gen-recent-all');
  assert.strictEqual(link.textContent, 'View all in gitpass');
  link.click(); await settle();
  assert.ok(sent.some((m) => m.type === 'popup:open_in_app'));
});

test('empty state', async () => {
  const { w } = popup({ history: { enabled: true, entries: [] } });
  await settle();
  await openRecent(w);
  assert.strictEqual(q(w, '.gen-recent-body').textContent.trim(), 'No generated passwords yet');
  assert.strictEqual(qa(w, '.gen-recent-row').length, 0);
});

test('history setting off shows a muted line and no values', async () => {
  const { w } = popup({ history: { enabled: false, entries: ENTRIES } });
  await settle();
  await openRecent(w);
  assert.strictEqual(q(w, '.gen-recent-body').textContent.trim(), 'Generator history is off');
  assert.strictEqual(qa(w, '.gen-recent-row').length, 0);
  assert.ok(!w.document.body.textContent.includes('1234'));
});

test('locked host hides the section', async () => {
  const { w } = popup({ historyError: 'locked' });
  await settle();
  await openRecent(w);
  assert.ok(q(w, '.gen-recent').classList.contains('hidden'));
});
