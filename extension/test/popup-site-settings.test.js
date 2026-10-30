// Issue #341: per-site settings panel in the popup - gear button on a web origin,
// three toggles backed by neverSave / neverAutoSubmit / hiddenInlineMenu.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/policy.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/sitesettings.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

const item = (id, title, extra = {}) => ({
  id, vault_id: 'v1', vault_name: 'Personal', category: 'login', title,
  subtitle: 'user@example.invalid', favorite: false, url: 'https://alpha.example', ...extra,
});

function popup(origin) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const store = { settings: { autoSubmitAfterFill: true }, neverSave: [], neverAutoSubmit: [], hiddenInlineMenu: [] };
  w.chrome = {
    storage: {
      local: {
        get: (k, cb) => setTimeout(() => cb(Array.isArray(k) ? Object.fromEntries(k.map((x) => [x, store[x]])) : { [k]: store[k] }), 0),
        set: (obj, cb) => { Object.assign(store, obj); setTimeout(cb, 0); },
      },
      onChanged: { addListener() {} },
    },
  };
  w.__gitpassMock = (msg) => {
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: [item('v1:login', 'Alpha Login')] });
      case 'match_active': return ok({ items: [item('v1:login', 'Alpha Login')], origin });
      case 'get_item': return ok({ item: { ...item('v1:login', 'Alpha Login'), fields: [
        { name: 'username', label: 'username', kind: 'text', concealed: false, value: 'user@example.invalid' },
        { name: 'password', label: 'password', kind: 'password', concealed: true },
      ], tags: [] } });
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, store };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); };

test('gear button appears for a web origin and opens the panel with three toggles', async (t) => {
  const { dom, w } = popup('https://alpha.example');
  t.after(() => dom.window.close());
  await settle();

  const gear = w.document.getElementById('siteSettingsBtn');
  assert.ok(gear && !gear.classList.contains('hidden'), 'gear visible on a fillable origin');
  gear.click();
  await settle();

  const panel = w.document.getElementById('panel');
  assert.ok(!panel.classList.contains('hidden'), 'panel opened');
  const rows = [...panel.querySelectorAll('.site-settings-row')];
  assert.strictEqual(rows.length, 3, 'save / submit-after-fill / inline menu toggles');
  assert.ok(panel.textContent.includes('alpha.example'), 'panel names the current site');
  assert.ok(rows.every((r) => r.querySelector('input').checked), 'everything on by default');
});

test('toggling save-logins off writes the domain to neverSave', async (t) => {
  const { dom, w, store } = popup('https://alpha.example');
  t.after(() => dom.window.close());
  await settle();
  w.document.getElementById('siteSettingsBtn').click();
  await settle();

  const saveRow = [...w.document.querySelectorAll('.site-settings-row')].find((r) => r.textContent.includes('save logins'));
  const cb = saveRow.querySelector('input');
  cb.click();
  await settle();
  assert.deepEqual(store.neverSave, ['alpha.example']);
  assert.ok(!cb.checked);

  cb.click();
  await settle();
  assert.deepEqual(store.neverSave, []);
  assert.ok(cb.checked);
});

test('toggling inline menu off writes the domain to hiddenInlineMenu', async (t) => {
  const { dom, w, store } = popup('https://alpha.example');
  t.after(() => dom.window.close());
  await settle();
  w.document.getElementById('siteSettingsBtn').click();
  await settle();

  const menuRow = [...w.document.querySelectorAll('.site-settings-row')].find((r) => r.textContent.includes('inline menu'));
  menuRow.querySelector('input').click();
  await settle();
  assert.deepEqual(store.hiddenInlineMenu, ['alpha.example']);
});

test('submit-after-fill toggle is hidden when the global opt-in is off', async (t) => {
  const { dom, w, store } = popup('https://alpha.example');
  t.after(() => dom.window.close());
  store.settings.autoSubmitAfterFill = false;
  await settle();
  w.document.getElementById('siteSettingsBtn').click();
  await settle();
  const rows = [...w.document.querySelectorAll('.site-settings-row')];
  assert.strictEqual(rows.length, 2, 'only save + inline menu when autoSubmitAfterFill is off');
});

test('gear hidden without a fillable origin', async (t) => {
  const { dom, w } = popup(null);
  t.after(() => dom.window.close());
  await settle();
  assert.ok(w.document.getElementById('siteSettingsBtn').classList.contains('hidden'));
});
