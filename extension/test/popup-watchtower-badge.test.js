// Issue #390: the Watchtower shield shows a severity dot and an accessible count
// when items need attention; locked popups never ask for the audit.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/policy.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/recent.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/sitesettings.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const item = (id, title) => ({ id, vault_id: 'v1', vault_name: 'Personal', category: 'login', title, subtitle: 'u', favorite: false, url: 'https://alpha.example' });

function popup({ unlocked = true, audit, auditFails = false } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok(unlocked ? { unlocked: true, app_running: true, account: { name: 'F', email: 'f@x' } } : { unlocked: false, app_running: true });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: [item('v1:a', 'Alpha'), item('v1:b', 'Beta')] });
      case 'match_active': return ok({ items: [], origin: null });
      case 'get_item': return ok({ item: { ...item('v1:a', 'Alpha'), fields: [], tags: [] } });
      case 'audit_summary': return auditFails ? Promise.reject(new Error('audit unavailable')) : ok(audit);
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); await sleep(40); };
const btn = (w) => w.document.getElementById('watchtowerBtn');

test('danger findings paint the red dot and set the accessible label', async (t) => {
  const { dom, w } = popup({ audit: { score: 40, items_checked: 2, findings: [
    { kind: 'pwned', count: 1, items: [{ id: 'v1:a' }] },
    { kind: 'old', count: 1, items: [{ id: 'v1:b' }] },
  ] } });
  t.after(() => dom.window.close());
  await settle();
  assert.ok(btn(w).classList.contains('wt-dot-danger'));
  assert.ok(!btn(w).classList.contains('wt-dot-warn'));
  assert.strictEqual(btn(w).getAttribute('aria-label'), 'Watchtower: 2 items need attention');
  assert.strictEqual(btn(w).title, 'Watchtower: 2 items need attention');
});

test('warning-only findings paint the amber dot', async (t) => {
  const { dom, w } = popup({ audit: { score: 85, items_checked: 2, findings: [
    { kind: 'rotation_due', count: 1, items: [{ id: 'v1:a' }] },
  ] } });
  t.after(() => dom.window.close());
  await settle();
  assert.ok(btn(w).classList.contains('wt-dot-warn'));
  assert.ok(!btn(w).classList.contains('wt-dot-danger'));
  assert.strictEqual(btn(w).getAttribute('aria-label'), 'Watchtower: 1 items need attention');
});

test('a clean vault shows no dot and the plain Watchtower label', async (t) => {
  const { dom, w } = popup({ audit: { score: 100, items_checked: 2, findings: [] } });
  t.after(() => dom.window.close());
  await settle();
  assert.ok(!btn(w).classList.contains('wt-dot-warn'));
  assert.ok(!btn(w).classList.contains('wt-dot-danger'));
  assert.strictEqual(btn(w).getAttribute('aria-label'), 'Watchtower');
});

test('locked popup: no dot and no audit_summary request', async (t) => {
  const { dom, w, sent } = popup({ unlocked: false });
  t.after(() => dom.window.close());
  await settle();
  assert.ok(!btn(w) || (!btn(w).classList.contains('wt-dot-warn') && !btn(w).classList.contains('wt-dot-danger')));
  assert.ok(!sent.some((m) => m.type === 'popup:audit_summary'), 'locked popup must not request the audit');
});

test('a failed audit_summary leaves a plain shield and no error banner', async (t) => {
  const { dom, w } = popup({ auditFails: true });
  t.after(() => dom.window.close());
  await settle();
  assert.ok(!btn(w).classList.contains('wt-dot-warn'));
  assert.ok(!btn(w).classList.contains('wt-dot-danger'));
  assert.strictEqual(btn(w).getAttribute('aria-label'), 'Watchtower');
  assert.ok(!w.document.querySelector('.inline-err'), 'no error surface in the list');
});
