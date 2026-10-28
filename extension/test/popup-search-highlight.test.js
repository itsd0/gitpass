// Unit tests for issue #118:
// - pure function match-range and segment logic (view.js)
// - popup DOM highlighting in title and username/subtitle
// - keyboard hint footer bar and localized text
// - aria-selected and aria-activedescendant accessibility pattern
// - Escape key clearing search vs closing popup
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');

const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/popup.js'];

const ITEMS = [
  { id: 'v1:i1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'alice@github.com', url: 'https://github.com', favorite: false },
  { id: 'v1:i2', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitLab Account', subtitle: 'git-admin', url: 'https://gitlab.com', favorite: false },
  { id: 'v1:i3', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Chase Visa', subtitle: '•••• 4242', favorite: true },
];

function popup({ items = ITEMS } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  w.__gitpassMock = (msg) => {
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items });
      case 'match_active': return ok({ items: [], origin: null });
      case 'get_item': return ok({ item: { ...items.find((i) => i.id === msg.item_id), fields: [] } });
      default: return ok({});
    }
  };
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); };

// =========================================================================
// 1. Pure function tests (highlightRanges & highlightSegments)
// =========================================================================

test('highlightRanges: returns sorted non-overlapping ranges', () => {
  // Empty inputs
  assert.deepStrictEqual(V.highlightRanges('', 'test'), []);
  assert.deepStrictEqual(V.highlightRanges('test', ''), []);
  assert.deepStrictEqual(V.highlightRanges(null, 'test'), []);
  assert.deepStrictEqual(V.highlightRanges('test', null), []);

  // Basic prefix and substring
  assert.deepStrictEqual(V.highlightRanges('GitHub', 'git'), [[0, 3]]);
  assert.deepStrictEqual(V.highlightRanges('alice@github.com', 'git'), [[6, 9]]);

  // Multiple occurrences
  assert.deepStrictEqual(V.highlightRanges('banana', 'an'), [[1, 5]]); // [1,3] and [3,5] merged to [1,5]

  // Multiple terms separated by space
  assert.deepStrictEqual(V.highlightRanges('GitHub Account', 'git acc'), [[0, 3], [7, 10]]);

  // Case insensitivity
  assert.deepStrictEqual(V.highlightRanges('GitPASS', 'gitpass'), [[0, 7]]);

  // Special characters in query (no regex crash)
  assert.deepStrictEqual(V.highlightRanges('user@domain.com (Work)', '@domain.com ('), [[4, 15], [16, 17]]);
});

test('highlightSegments: splits text into matched and non-matched chunks', () => {
  // No match
  assert.deepStrictEqual(V.highlightSegments('GitHub', 'nomatch'), [
    { text: 'GitHub', match: false },
  ]);

  // Prefix match
  assert.deepStrictEqual(V.highlightSegments('GitHub', 'git'), [
    { text: 'Git', match: true },
    { text: 'Hub', match: false },
  ]);

  // Middle match
  assert.deepStrictEqual(V.highlightSegments('alice@github.com', 'git'), [
    { text: 'alice@', match: false },
    { text: 'git', match: true },
    { text: 'hub.com', match: false },
  ]);

  // Multiple matched terms
  assert.deepStrictEqual(V.highlightSegments('GitLab Account', 'git acc'), [
    { text: 'Git', match: true },
    { text: 'Lab ', match: false },
    { text: 'Acc', match: true },
    { text: 'ount', match: false },
  ]);
});

test('rowSubtitle: formats secondary line and includes domain when domain matches query', () => {
  const itemWithBoth = { title: 'Personal', subtitle: 'alice', url: 'https://github.com/login' };
  assert.strictEqual(V.rowSubtitle(itemWithBoth, ''), 'alice');
  assert.strictEqual(V.rowSubtitle(itemWithBoth, 'ali'), 'alice');
  assert.strictEqual(V.rowSubtitle(itemWithBoth, 'github'), 'alice · github.com');

  const itemUrlOnly = { title: 'Site', subtitle: '', url: 'https://gitlab.com' };
  assert.strictEqual(V.rowSubtitle(itemUrlOnly, ''), 'gitlab.com');
  assert.strictEqual(V.rowSubtitle(itemUrlOnly, 'git'), 'gitlab.com');

  const itemNeither = { title: 'Secret Note', subtitle: '', url: '' };
  assert.strictEqual(V.rowSubtitle(itemNeither, ''), '—');
});

test('keyAction: Escape clears query when query is active, closes otherwise', () => {
  assert.strictEqual(V.keyAction({ key: 'Escape' }, { query: 'test' }), 'clearSearch');
  assert.strictEqual(V.keyAction({ key: 'Escape' }, { query: '' }), 'close');
  assert.strictEqual(V.keyAction({ key: 'Escape' }, { panel: 'new', query: 'test' }), 'closePanel');
});

// =========================================================================
// 2. DOM integration tests (JSDOM with popup)
// =========================================================================

test('popup DOM: search highlights matched terms in title and subtitle using <mark class="hl">', async () => {
  const { w } = popup({ items: ITEMS });
  await settle();

  const qInput = w.document.getElementById('q');
  qInput.value = 'git';
  qInput.dispatchEvent(new w.Event('input', { bubbles: true }));
  await settle();

  // Rows rendered
  const rows = w.document.querySelectorAll('#listInner .row');
  assert.strictEqual(rows.length, 2, 'two matching rows for "git"');

  // First row: GitHub / alice@github.com
  const firstRow = rows[0];
  const titleMarks = firstRow.querySelectorAll('.t mark.hl');
  assert.strictEqual(titleMarks.length, 1, 'title has 1 highlight mark');
  assert.strictEqual(titleMarks[0].textContent, 'Git', 'matched substring preserves original title case');

  const subMarks = firstRow.querySelectorAll('.u mark.hl');
  assert.strictEqual(subMarks.length, 1, 'subtitle has 1 highlight mark');
  assert.strictEqual(subMarks[0].textContent, 'git', 'matched substring in subtitle');
});

test('popup DOM: ARIA attributes on rows and active-descendant on search input', async () => {
  const { w } = popup({ items: ITEMS });
  await settle();

  const qInput = w.document.getElementById('q');
  assert.strictEqual(qInput.getAttribute('role'), 'combobox');
  assert.strictEqual(qInput.getAttribute('aria-controls'), 'list');

  const rows = w.document.querySelectorAll('#listInner .row');
  assert.ok(rows.length >= 2);

  // Selected row
  const selectedRow = rows[0];
  assert.strictEqual(selectedRow.getAttribute('role'), 'option');
  assert.strictEqual(selectedRow.getAttribute('aria-selected'), 'true');
  assert.strictEqual(selectedRow.id, 'row-0');
  assert.strictEqual(qInput.getAttribute('aria-activedescendant'), 'row-0');

  // Non-selected row
  const secondRow = rows[1];
  assert.strictEqual(secondRow.getAttribute('aria-selected'), 'false');
  assert.strictEqual(secondRow.id, 'row-1');

  // ArrowDown moves selection
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
  await settle();

  const updatedRows = w.document.querySelectorAll('#listInner .row');
  assert.strictEqual(updatedRows[0].getAttribute('aria-selected'), 'false');
  assert.strictEqual(updatedRows[1].getAttribute('aria-selected'), 'true');
  assert.strictEqual(qInput.getAttribute('aria-activedescendant'), 'row-1');
});

test('popup DOM: footer hint bar displays keyboard shortcuts when results are shown and hides on empty', async () => {
  const { w } = popup({ items: ITEMS });
  await settle();

  const hints = w.document.getElementById('searchHints');
  assert.ok(hints, 'searchHints footer element exists');
  assert.strictEqual(hints.classList.contains('hidden'), false, 'hints visible when list results exist');
  assert.strictEqual(hints.textContent, '↑↓ navigate · ↵ fill · ⌘C copy password · ? all shortcuts · Esc clear');

  // Query with no matches
  const qInput = w.document.getElementById('q');
  qInput.value = 'no-matching-item-xyz';
  qInput.dispatchEvent(new w.Event('input', { bubbles: true }));
  await settle();

  assert.strictEqual(hints.classList.contains('hidden'), true, 'hints hidden when empty state is shown');
});

test('popup DOM: Escape key clears active search query and resets list', async () => {
  const { w } = popup({ items: ITEMS });
  await settle();

  const qInput = w.document.getElementById('q');
  qInput.value = 'git';
  qInput.dispatchEvent(new w.Event('input', { bubbles: true }));
  await settle();

  assert.strictEqual(w.document.querySelectorAll('#listInner .row').length, 2);

  // Press Escape
  w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await settle();

  assert.strictEqual(qInput.value, '', 'query input cleared on Escape');
  assert.strictEqual(w.document.querySelectorAll('#listInner .row').length, 3, 'full item list restored');
});

test('popup DOM: search highlights domain in subtitle when URL matches query', async () => {
  const customItems = [
    { id: 'v1:x', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Work Account', subtitle: 'john_doe', url: 'https://jira.atlassian.com', favorite: false },
  ];
  const { w } = popup({ items: customItems });
  await settle();

  const qInput = w.document.getElementById('q');
  qInput.value = 'jira';
  qInput.dispatchEvent(new w.Event('input', { bubbles: true }));
  await settle();

  const rows = w.document.querySelectorAll('#listInner .row');
  assert.strictEqual(rows.length, 1);
  const uEl = rows[0].querySelector('.u');
  assert.ok(uEl.textContent.includes('jira.atlassian.com'), 'shows matched domain');
  const mark = uEl.querySelector('mark.hl');
  assert.ok(mark, 'mark element exists in subtitle');
  assert.strictEqual(mark.textContent, 'jira', 'highlighted domain term');
});

test('popup CSS: mark.hl and row tokens meet WCAG AA contrast in dark and light', () => {
  const css = fs.readFileSync(path.join(root, 'popup', 'popup.css'), 'utf8');
  assert.ok(css.includes('mark.hl'), 'CSS defines mark.hl');
  assert.ok(css.includes('.search-hints'), 'CSS defines .search-hints');

  const lum = (rgb) => {
    const c = rgb.map((v) => v / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
  };
  const contrast = (c1, c2) => {
    const [hi, lo] = [lum(c1), lum(c2)].sort((a, b) => b - a);
    return (hi + 0.05) / (lo + 0.05);
  };

  // Dark palette text on row background
  const darkFg = [236, 237, 240];
  const darkPanel = [36, 36, 38];
  assert.ok(contrast(darkFg, darkPanel) >= 4.5, 'dark fg on panel >= 4.5');

  // Light palette text on row background
  const lightFg = [24, 26, 32];
  const lightPanel = [255, 255, 255];
  assert.ok(contrast(lightFg, lightPanel) >= 4.5, 'light fg on panel >= 4.5');
});
