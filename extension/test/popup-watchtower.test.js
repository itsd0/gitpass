// Unit tests for Watchtower warnings in the extension popup (issue #155).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const V = require('../popup/view.js');

test('view.js: findingSeverity and hasWatchtowerFindings', () => {
  assert.strictEqual(V.findingSeverity('weak'), 'danger');
  assert.strictEqual(V.findingSeverity('reused'), 'danger');
  assert.strictEqual(V.findingSeverity('pwned'), 'danger');
  assert.strictEqual(V.findingSeverity('expired'), 'danger');
  assert.strictEqual(V.findingSeverity('old'), 'warn');
  assert.strictEqual(V.findingSeverity('insecure_url'), 'warn');
  assert.strictEqual(V.findingSeverity('missing_totp'), 'warn');
  assert.strictEqual(V.findingSeverity('totp_available'), 'warn');
  assert.strictEqual(V.findingSeverity('passkey_available'), 'warn');
  assert.strictEqual(V.findingSeverity('expiring_soon'), 'warn');
  assert.strictEqual(V.findingSeverity('duplicate'), 'warn');
  assert.strictEqual(V.findingSeverity('unused'), 'warn');
  assert.strictEqual(V.findingSeverity('rotation_due'), 'warn');
  assert.strictEqual(V.findingSeverity('rotation_soon'), 'warn', 'rotation_soon is a heads-up, not danger');

  assert.strictEqual(V.hasWatchtowerFindings(null), false);
  assert.strictEqual(V.hasWatchtowerFindings({}), false);
  assert.strictEqual(V.hasWatchtowerFindings({ kinds: [] }), false);
  assert.strictEqual(V.hasWatchtowerFindings({ kinds: ['weak'] }), true);
});

test('view.js: findingLabel formats chips correctly', () => {
  const fakeT = (key, fallback, subs) => {
    let s = fallback || key;
    if (subs) s = s.replace(/\$(\d)/g, (_, n) => subs[Number(n) - 1]);
    return s;
  };
  assert.strictEqual(V.findingLabel('weak', {}, fakeT), 'Weak password');
  assert.strictEqual(V.findingLabel('reused', { reused_count: 3 }, fakeT), 'Reused on 3 items');
  assert.strictEqual(V.findingLabel('reused', {}, fakeT), 'Reused password');
  assert.strictEqual(V.findingLabel('reused_old_password', {}, fakeT), 'Reused an old password');
  assert.strictEqual(V.findingSeverity('reused_old_password'), 'warn');
  assert.strictEqual(V.findingLabel('pwned', {}, fakeT), 'In a data breach');
  assert.strictEqual(V.findingLabel('old', { old_days: 365 }, fakeT), 'Not changed in 1 year');
  assert.strictEqual(V.findingLabel('old', { old_days: 730 }, fakeT), 'Not changed in 2 years');
  assert.strictEqual(V.findingLabel('old', { old_days: 42 }, fakeT), 'Not changed in 42 days');
  assert.strictEqual(V.findingLabel('old', {}, fakeT), 'Old password');
  assert.strictEqual(V.findingLabel('insecure_url', {}, fakeT), 'Insecure website (HTTP)');
  assert.strictEqual(V.findingLabel('missing_totp', {}, fakeT), '2FA not configured');
  assert.strictEqual(V.findingLabel('totp_available', {}, fakeT), '2FA available on site');
  assert.strictEqual(V.findingLabel('passkey_available', {}, fakeT), 'Passkey available');
  assert.strictEqual(V.findingLabel('expired', {}, fakeT), 'Expired');
  assert.strictEqual(V.findingLabel('expiring_soon', {}, fakeT), 'Expiring soon');
  assert.strictEqual(V.findingLabel('duplicate', {}, fakeT), 'Duplicate login');
  assert.strictEqual(V.findingLabel('unused', {}, fakeT), 'Not used in over a year');
  assert.strictEqual(V.findingLabel('rotation_due', {}, fakeT), 'Due for rotation');
  assert.strictEqual(V.findingLabel('rotation_soon', {}, fakeT), 'Rotation due soon');
});

test('view.js: every Finding kind the core serializes has a localized chip label (issue #369)', () => {
  // Parse the kinds straight from the core enum so a future finding can't fall back
  // to a raw snake_case chip without this test noticing.
  const auditSrc = fs.readFileSync(path.join(root, '..', 'crates', 'gitpass-core', 'src', 'audit', 'mod.rs'), 'utf8');
  const m = auditSrc.match(/FINDING_KINDS: &\[&str\] = &\[([^\]]+)\]/);
  assert.ok(m, 'FINDING_KINDS not found in core audit/mod.rs');
  const kinds = [...m[1].matchAll(/"([a-z_]+)"/g)].map((x) => x[1]);
  assert.ok(kinds.length >= 15, 'expected the full finding list, got ' + kinds.length);
  const fakeT = (key, fallback, subs) => {
    let s = fallback || key;
    if (subs) s = s.replace(/\$(\d)/g, (_, n) => subs[Number(n) - 1]);
    return s;
  };
  for (const kind of kinds) {
    const label = V.findingLabel(kind, {}, fakeT);
    assert.notStrictEqual(label, kind, `findingLabel('${kind}') returned the raw kind`);
    assert.ok(label.length > 0, `findingLabel('${kind}') returned an empty label`);
    assert.ok(!label.includes('_'), `findingLabel('${kind}') still looks like a raw kind: ${label}`);
  }
});

const ITEMS = [
  { id: 'v1:i1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'octo', url: 'https://github.com', favorite: false, has_totp: false },
  { id: 'v1:i2', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitLab', subtitle: 'git', url: 'https://gitlab.com', favorite: false, has_totp: false },
  { id: 'v1:i3', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: false, has_totp: false },
];

function setupPopup({ itemFindings, updateLogin } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];
  const createdTabs = [];

  w.chrome = {
    tabs: {
      create: (opts) => {
        createdTabs.push(opts);
        return Promise.resolve({ id: 99 });
      },
    },
  };

  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Test User', email: 'test@example.com' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: ITEMS });
      case 'match_active': return ok({ items: ITEMS, origin: 'https://github.com' });
      case 'get_item': {
        const it = ITEMS.find((x) => x.id === msg.item_id) || ITEMS[0];
        const fields = [];
        if (it.category === 'login') {
          fields.push({ name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle });
          fields.push({ name: 'password', label: 'password', kind: 'password', concealed: true });
          if (it.url) fields.push({ name: 'url', label: 'website', kind: 'url', concealed: false, value: it.url });
        }
        return ok({ item: { ...it, fields, notes: '', tags: [] } });
      }
      case 'item_findings': {
        if (itemFindings) return itemFindings(msg);
        if (msg.item_id === 'v1:i1') {
          return ok({
            kinds: ['weak', 'reused'],
            reused_count: 3,
            change_password_url: 'https://github.com/.well-known/change-password',
          });
        }
        return ok({ kinds: [] });
      }
      case 'generate': return ok({ password: 'FAKE-GENERATED-PW-456' });
      case 'update_login': return updateLogin ? updateLogin(msg) : ok({ ok: true });
      case 'open_in_app': return ok({ opened: true });
      default: return ok({});
    }
  };

  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent, createdTabs };
}

test('popup detail: renders warning strip with chips and action buttons for login with findings', async () => {
  const { dom, w, sent, createdTabs } = setupPopup();
  await new Promise((r) => setTimeout(r, 100));

  // Select item 1 (GitHub)
  dom.window.document.querySelectorAll('#listInner .row')[0].click();
  await new Promise((r) => setTimeout(r, 80));

  const strip = dom.window.document.querySelector('.warning-strip');
  assert.ok(strip, 'warning strip must be rendered');
  assert.strictEqual(strip.getAttribute('role'), 'status');
  assert.ok(strip.getAttribute('aria-label'), 'must have aria-label');

  const chips = strip.querySelectorAll('.warning-chip');
  assert.strictEqual(chips.length, 2, 'must render 2 chips');
  assert.ok(chips[0].classList.contains('danger'));
  assert.strictEqual(chips[0].textContent, 'Weak password');
  assert.strictEqual(chips[0].getAttribute('tabindex'), '0');
  assert.strictEqual(chips[0].getAttribute('role'), 'listitem');

  assert.ok(chips[1].classList.contains('danger'));
  assert.strictEqual(chips[1].textContent, 'Reused on 3 items');
  assert.strictEqual(chips[1].getAttribute('tabindex'), '0');

  // Check action buttons
  const actions = strip.querySelector('.warning-actions');
  assert.ok(actions, 'action bar must exist');

  const buttons = actions.querySelectorAll('button');
  assert.strictEqual(buttons.length, 4, 'must render 4 action buttons');

  const [genBtn, siteBtn, wtBtn, appBtn] = buttons;
  assert.strictEqual(genBtn.textContent.trim(), 'Generate & update');
  assert.ok(genBtn.classList.contains('btn-primary'), 'primary button has orange accent');

  assert.strictEqual(siteBtn.textContent.trim(), 'Change on site');
  assert.strictEqual(wtBtn.textContent.trim(), 'Open Watchtower');
  assert.strictEqual(appBtn.textContent.trim(), 'Open in gitpass');

  // Test Change on site click
  siteBtn.click();
  assert.strictEqual(createdTabs.length, 1);
  assert.strictEqual(createdTabs[0].url, 'https://github.com/.well-known/change-password');
  assert.strictEqual(createdTabs[0].active, true);

  // Test Open Watchtower click
  wtBtn.click();
  assert.ok(sent.some((m) => m.type === 'popup:open_in_app' && m.view === 'watchtower'));

  // Test Open in gitpass click
  appBtn.click();
  assert.ok(sent.some((m) => m.type === 'popup:open_in_app' && m.item_id === 'v1:i1'));
});

test('popup detail: hidden when item has no findings', async () => {
  const { dom, w } = setupPopup();
  await new Promise((r) => setTimeout(r, 100));

  // Select item 2 (GitLab, no findings)
  dom.window.document.querySelectorAll('#listInner .row')[1].click();
  await new Promise((r) => setTimeout(r, 80));

  const strip = dom.window.document.querySelector('.warning-strip');
  assert.strictEqual(strip, null, 'warning strip must NOT be rendered when no findings');
});

test('popup detail: Generate & update action saves new password and updates item', async () => {
  let updatedPayload = null;
  const { dom, w } = setupPopup({
    updateLogin: (msg) => {
      updatedPayload = msg;
      return Promise.resolve({ ok: true });
    },
  });
  await new Promise((r) => setTimeout(r, 100));

  dom.window.document.querySelectorAll('#listInner .row')[0].click();
  await new Promise((r) => setTimeout(r, 80));

  const genBtn = dom.window.document.querySelector('.warning-actions .btn-primary');
  assert.ok(genBtn, 'Generate & update button must exist');
  genBtn.click();
  await new Promise((r) => setTimeout(r, 50));

  // Panel should now be generator with update button
  const panel = dom.window.document.getElementById('panel');
  assert.ok(!panel.classList.contains('hidden'), 'generator panel must open');

  const updateBtn = panel.querySelector('button.btn-primary');
  assert.ok(updateBtn, 'update password button must exist');
  assert.strictEqual(updateBtn.textContent.trim(), 'Update password');

  // Wait for password to generate
  await new Promise((r) => setTimeout(r, 50));
  updateBtn.click();
  await new Promise((r) => setTimeout(r, 50));

  assert.ok(updatedPayload, 'update_login must have been sent');
  assert.strictEqual(updatedPayload.item_id, 'v1:i1');
  assert.strictEqual(updatedPayload.password, 'FAKE-GENERATED-PW-456');
});

test('popup list: displays warning dot only on items with findings', async () => {
  const { dom, w } = setupPopup();
  await new Promise((r) => setTimeout(r, 100));

  const rows = dom.window.document.querySelectorAll('#listInner .row');
  assert.strictEqual(rows.length, 3);

  // Row 0 (GitHub) has findings
  const dot0 = rows[0].querySelector('.warn-dot');
  assert.ok(dot0, 'row 0 must have warning dot');
  assert.ok(dot0.classList.contains('danger'));
  assert.strictEqual(dot0.getAttribute('role'), 'img');
  assert.strictEqual(dot0.getAttribute('aria-label'), 'Security warning');

  // Row 1 (GitLab) has no findings
  const dot1 = rows[1].querySelector('.warn-dot');
  assert.strictEqual(dot1, null, 'row 1 must not have warning dot');

  // Row 2 (Visa credit card) is not a login
  const dot2 = rows[2].querySelector('.warn-dot');
  assert.strictEqual(dot2, null, 'credit card row must not have warning dot');
});

test('view.js: mapLimit runs the first call alone, then bounds concurrency', async () => {
  let inFlight = 0; let peak = 0; const firstAlone = [];
  const out = await V.mapLimit([1, 2, 3, 4, 5, 6, 7], 3, async (x, i) => {
    inFlight++; peak = Math.max(peak, inFlight);
    if (i === 0) firstAlone.push(inFlight);
    await new Promise((r) => setTimeout(r, 5));
    inFlight--;
    return x * 2;
  });
  assert.deepStrictEqual(out, [2, 4, 6, 8, 10, 12, 14]);
  assert.deepStrictEqual(firstAlone, [1]);
  assert.ok(peak <= 3, `peak concurrency ${peak} must be <= 3`);
  assert.deepStrictEqual(await V.mapLimit([], 3, async () => 1), []);
});

test('view.js: cacheableFindings rejects failed lookups', () => {
  assert.strictEqual(V.cacheableFindings({ kinds: [] }), true);
  assert.strictEqual(V.cacheableFindings({ kinds: ['weak'] }), true);
  assert.strictEqual(V.cacheableFindings(null), false);
  assert.strictEqual(V.cacheableFindings(undefined), false);
});

test('popup list: a failed findings lookup is not cached as "no findings" and is retried', async () => {
  let failing = true;
  const { dom, w } = setupPopup({
    itemFindings: (msg) => {
      if (failing) return Promise.reject(new Error('timeout'));
      return Promise.resolve(msg.item_id === 'v1:i1' ? { kinds: ['weak'] } : { kinds: [] });
    },
  });
  await new Promise((r) => setTimeout(r, 100));
  let rows = dom.window.document.querySelectorAll('#listInner .row');
  assert.strictEqual(rows[0].querySelector('.warn-dot'), null, 'no dot while the audit is failing');

  failing = false;
  // Re-filtering the list re-runs the findings fetch for uncached rows.
  w.document.getElementById('q').dispatchEvent(new w.Event('input', { bubbles: true }));
  await new Promise((r) => setTimeout(r, 200));
  rows = dom.window.document.querySelectorAll('#listInner .row');
  assert.ok(rows[0].querySelector('.warn-dot'), 'dot appears once the audit succeeds');
});
