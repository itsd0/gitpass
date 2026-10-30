// Issue #376: Vault-wide Watchtower overview panel in extension popup
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/policy.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/recent.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/sitesettings.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const item = (id, title, extra = {}) => ({
  id, vault_id: 'v1', vault_name: 'Personal', category: 'login', title,
  subtitle: 'user@example.invalid', username: 'user@example.invalid', favorite: false, url: 'https://example.com', ...extra,
});

function setupPopup(opts = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const store = { settings: {} };
  w.chrome = {
    storage: {
      local: {
        get: (k, cb) => setTimeout(() => cb(Array.isArray(k) ? Object.fromEntries(k.map((x) => [x, store[x]])) : { [k]: store[k] }), 0),
        set: (obj, cb) => { Object.assign(store, obj); setTimeout(cb, 0); },
      },
      onChanged: { addListener() {} },
    },
  };

  const auditSummaryData = opts.auditSummary || {
    score: 75,
    items_checked: 5,
    findings: [
      {
        kind: 'weak',
        count: 2,
        items: [
          item('v1:i1', 'GitHub', { username: 'octocat', url: 'https://github.com' }),
          item('v1:i2', 'GitLab', { username: 'gitlabber', url: 'https://gitlab.com' }),
        ],
      },
      {
        kind: 'reused',
        count: 2,
        items: [
          item('v1:i1', 'GitHub', { username: 'octocat', url: 'https://github.com' }),
          item('v1:i2', 'GitLab', { username: 'gitlabber', url: 'https://gitlab.com' }),
        ],
      },
      {
        kind: 'rotation_due',
        count: 1,
        items: [
          item('v1:i3', 'Stripe', { username: 'payuser', url: 'https://stripe.com' }),
        ],
      },
    ],
  };

  let failAudit = Boolean(opts.failAudit);

  w.__gitpassMock = (msg) => {
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Test User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: [item('v1:i1', 'GitHub'), item('v1:i2', 'GitLab'), item('v1:i3', 'Stripe')] });
      case 'match_active': return ok({ items: [item('v1:i1', 'GitHub')], origin: 'https://github.com' });
      case 'get_item': {
        const id = msg.item_id || 'v1:i1';
        return ok({ item: { ...item(id, id === 'v1:i3' ? 'Stripe' : 'GitHub'), fields: [], tags: [] } });
      }
      case 'audit_summary': {
        if (failAudit) return Promise.reject(new Error('connection timeout'));
        return ok(auditSummaryData);
      }
      default: return ok({});
    }
  };

  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return {
    dom,
    w,
    setFailAudit: (f) => { failAudit = f; },
  };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); };

test('Watchtower button exists in filter row and opens the overview panel', async (t) => {
  const { dom, w } = setupPopup();
  t.after(() => dom.window.close());
  await settle();

  const wtBtn = w.document.getElementById('watchtowerBtn');
  assert.ok(wtBtn, 'watchtowerBtn must exist in popup');
  assert.ok((wtBtn.getAttribute('aria-label') || '').startsWith('Watchtower'), 'shield keeps an accessible Watchtower name (with count when items need attention, #390)');

  wtBtn.click();
  await settle();

  const panel = w.document.getElementById('panel');
  assert.ok(!panel.classList.contains('hidden'), 'panel must open');
  assert.ok(panel.classList.contains('wt-panel'), 'panel must have wt-panel class');
  assert.strictEqual(panel.getAttribute('role'), 'dialog');
  assert.strictEqual(panel.getAttribute('aria-modal'), 'true');

  // Overall score card
  const scoreNum = panel.querySelector('.wt-gauge-num');
  assert.ok(scoreNum, 'score gauge num must exist');
  assert.strictEqual(scoreNum.textContent.trim(), '75');
  const scoreLabel = panel.querySelector('.wt-score-label');
  assert.ok(scoreLabel, 'score label must exist');
  assert.strictEqual(scoreLabel.textContent.trim(), 'Good');

  // Finding rows
  const rows = [...panel.querySelectorAll('.wt-finding-row')];
  assert.strictEqual(rows.length, 3, 'should render 3 finding rows');
  assert.strictEqual(rows[0].dataset.kind, 'weak');
  assert.ok(rows[0].textContent.includes('Weak password · 2'));
  assert.ok(rows[0].classList.contains('sev-danger'), 'weak must be danger');

  assert.strictEqual(rows[1].dataset.kind, 'reused');
  assert.ok(rows[1].textContent.includes('Reused password · 2'));
  assert.ok(rows[1].classList.contains('sev-danger'), 'reused must be danger');

  assert.strictEqual(rows[2].dataset.kind, 'rotation_due');
  assert.ok(rows[2].textContent.includes('Due for rotation · 1'));
  assert.ok(rows[2].classList.contains('sev-warn'), 'rotation_due must be warn');
});

test('Watchtower panel renders empty state when no findings need attention', async (t) => {
  const { dom, w } = setupPopup({
    auditSummary: { score: 100, items_checked: 5, findings: [] },
  });
  t.after(() => dom.window.close());
  await settle();

  w.document.getElementById('watchtowerBtn').click();
  await settle();

  const panel = w.document.getElementById('panel');
  assert.ok(!panel.classList.contains('hidden'));

  const empty = panel.querySelector('.wt-empty');
  assert.ok(empty, 'empty state container must exist');
  const msg = panel.querySelector('.wt-empty-msg');
  assert.strictEqual(msg.textContent.trim(), 'Nothing needs attention');
  const icon = panel.querySelector('.wt-empty-icon');
  assert.ok(icon, 'accent icon must be rendered');
});

test('Watchtower error state displays inline error card and retries', async (t) => {
  const ctx = setupPopup({ failAudit: true });
  t.after(() => ctx.dom.window.close());
  await settle();

  ctx.w.document.getElementById('watchtowerBtn').click();
  await settle();

  const panel = ctx.w.document.getElementById('panel');
  assert.ok(!panel.classList.contains('hidden'));

  const errCard = panel.querySelector('.inline-err');
  assert.ok(errCard, 'inline error card must be rendered on failure');
  const retryBtn = errCard.querySelector('button.btn-retry');
  assert.ok(retryBtn, 'retry button must exist');

  // Make audit succeed on retry
  ctx.setFailAudit(false);
  retryBtn.click();
  await settle();

  const rows = [...panel.querySelectorAll('.wt-finding-row')];
  assert.strictEqual(rows.length, 3, 'findings must render after retry');
});

test('Keyboard navigation between finding rows and drill-down into affected items', async (t) => {
  const { dom, w } = setupPopup();
  t.after(() => dom.window.close());
  await settle();

  w.document.getElementById('watchtowerBtn').click();
  await settle();

  const panel = w.document.getElementById('panel');
  const rows = [...panel.querySelectorAll('.wt-finding-row')];

  // Arrow navigation
  rows[0].dispatchEvent(new w.KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
  assert.strictEqual(w.document.activeElement, rows[1]);

  rows[1].dispatchEvent(new w.KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
  assert.strictEqual(w.document.activeElement, rows[0]);

  // Press Enter to drill down into 'weak' finding
  rows[0].dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await settle();

  // In drill-down view
  const drillTitle = panel.querySelector('.wt-drill-title');
  assert.ok(drillTitle, 'drill-down title must exist');
  assert.ok(drillTitle.textContent.includes('Weak password (2)'));

  const itemRows = [...panel.querySelectorAll('.wt-item-row')];
  assert.strictEqual(itemRows.length, 2, '2 affected items should be listed');
  assert.ok(itemRows[0].textContent.includes('GitHub'));
  assert.ok(itemRows[0].textContent.includes('octocat'));
  assert.ok(itemRows[1].textContent.includes('GitLab'));
  assert.ok(itemRows[1].textContent.includes('gitlabber'));

  // Esc goes back to overview
  itemRows[0].dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await settle();

  const overviewRows = [...panel.querySelectorAll('.wt-finding-row')];
  assert.strictEqual(overviewRows.length, 3, 'should return to overview rows');
});

test('Drill-down back button returns to overview', async (t) => {
  const { dom, w } = setupPopup();
  t.after(() => dom.window.close());
  await settle();

  w.document.getElementById('watchtowerBtn').click();
  await settle();

  const panel = w.document.getElementById('panel');
  const rows = [...panel.querySelectorAll('.wt-finding-row')];

  // Click first row to drill down
  rows[0].click();
  await settle();

  const backBtn = panel.querySelector('.wt-back-btn');
  assert.ok(backBtn, 'back button must exist');
  backBtn.click();
  await settle();

  const overviewRows = [...panel.querySelectorAll('.wt-finding-row')];
  assert.strictEqual(overviewRows.length, 3, 'back button returned to overview');
});

test('Choosing an affected item opens item detail and closes panel', async (t) => {
  const { dom, w } = setupPopup();
  t.after(() => dom.window.close());
  await settle();

  w.document.getElementById('watchtowerBtn').click();
  await settle();

  const panel = w.document.getElementById('panel');
  // Click first finding row (weak)
  panel.querySelectorAll('.wt-finding-row')[0].click();
  await settle();

  // Click first item row (GitHub, v1:i1)
  const itemRow = panel.querySelector('.wt-item-row');
  itemRow.click();
  await settle();

  // Panel must be closed
  assert.ok(panel.classList.contains('hidden'), 'panel must close when item is selected');

  // Detail pane must show selected item
  const detail = w.document.getElementById('detail');
  assert.ok(detail.textContent.includes('GitHub'), 'detail must display the chosen item');
});
