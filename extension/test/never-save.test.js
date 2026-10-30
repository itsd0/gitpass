// Unit tests for "Never for this site" save banner decision, exceptions management in options,
// domain normalisation, and popup note. (Issue #56)
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

globalThis.GitpassPolicy = require('../lib/policy.js');
const B = require('../bg-page.js');
const root = path.join(__dirname, '..');

const tab = (url, frameId = 0, id = 1) => ({ tab: { id, url }, frameId, url });
function fakeHost(items = [], pw = 'old') {
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'match') return { items };
    if (cmd === 'fill') return { username: 'user@example.com', password: pw };
    if (cmd === 'save_login') return { item_id: 'v:new' };
    return {};
  };
  h.calls = calls;
  return h;
}

test('domain normalisation: subdomains map to same registrable domain', () => {
  const P = GitpassPolicy;
  assert.strictEqual(P.registrableDomain('https://login.example.com'), 'example.com');
  assert.strictEqual(P.registrableDomain('https://sub.auth.example.com/path?q=1'), 'example.com');
  assert.strictEqual(P.registrableDomain('https://example.com'), 'example.com');
  assert.strictEqual(P.registrableDomain('example.com'), 'example.com');
  assert.strictEqual(P.registrableDomain('login.example.com'), 'example.com');
  assert.strictEqual(P.registrableDomain('https://sub.example.co.uk'), 'example.co.uk');
  assert.strictEqual(P.registrableDomain('https://auth.service.com.au'), 'service.com.au');
  assert.strictEqual(P.registrableDomain('http://localhost:3000'), 'localhost');
  assert.strictEqual(P.registrableDomain('http://127.0.0.1:8080'), '127.0.0.1');

  // isNeverSave checks
  const list = ['example.com', 'service.co.uk'];
  assert.strictEqual(P.isNeverSave('https://login.example.com', list), true);
  assert.strictEqual(P.isNeverSave('https://sub.auth.example.com', list), true);
  assert.strictEqual(P.isNeverSave('https://other.com', list), false);
  assert.strictEqual(P.isNeverSave('https://my.service.co.uk', list), true);
});

test('banner suppressed for excepted site; subdomains match; autofill unaffected; removing exception re-enables banner', async () => {
  await B._setNeverSave([]);
  const item = { id: 'v:1', category: 'login', title: 'Example', subtitle: 'user@example.com' };
  const h = fakeHost([item]);
  const s = tab('https://login.example.com/login', 0, 10);

  // 1. Initially not excepted -> banner offered
  const r1 = await B.handle({ type: 'page:capture', kind: 'login', username: 'user@example.com', password: 'newpass' }, s, h);
  assert.strictEqual(r1.prompt, 'update');

  // 2. Click "Never for this site"
  // Untrusted click rejected
  await assert.rejects(
    B.handle({ type: 'page:save_decision', action: 'never', user_click: false }, s, h),
    /needs a click/
  );
  // Trusted click succeeds and stores registrable domain
  const dec = await B.handle({ type: 'page:save_decision', action: 'never', user_click: true }, s, h);
  assert.deepStrictEqual(dec, { done: true, action: 'never' });
  const exceptions = await B._getNeverSave();
  assert.ok(exceptions.includes('example.com'), `expected example.com in ${JSON.stringify(exceptions)}`);

  // 3. Next capture on the same domain is suppressed
  const r2 = await B.handle({ type: 'page:capture', kind: 'login', username: 'user@example.com', password: 'newpass' }, s, h);
  assert.deepStrictEqual(r2, { prompt: null });

  // 4. Subdomains also suppressed
  const subTab = tab('https://auth.sub.example.com/login', 0, 11);
  const r3 = await B.handle({ type: 'page:capture', kind: 'login', username: 'user@example.com', password: 'newpass' }, subTab, h);
  assert.deepStrictEqual(r3, { prompt: null });

  // 5. page:pending also suppressed
  const p = await B.handle({ type: 'page:pending' }, s, h);
  assert.deepStrictEqual(p, { prompt: null });

  // 6. Autofill and inline menu are completely unaffected
  const match = await B.handle({ type: 'page:match' }, s, h);
  assert.strictEqual(match.items.length, 1);
  assert.strictEqual(match.items[0].id, 'v:1');
  const filled = await B.handle({ type: 'page:fill', item_id: 'v:1' }, s, h);
  assert.strictEqual(filled.password, 'old');

  // 7. Removing exception re-enables banner
  await B._removeNeverSave('example.com');
  assert.ok(!(await B._getNeverSave()).includes('example.com'));
  const r4 = await B.handle({ type: 'page:capture', kind: 'login', username: 'user@example.com', password: 'newpass' }, s, h);
  assert.strictEqual(r4.prompt, 'update');
});

test('options page: displays excluded domains with Remove button and empty state', async () => {
  const html = fs.readFileSync(path.join(root, 'options', 'options.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/options/options.html', runScripts: 'outside-only' });
  const w = dom.window;

  let storedExceptions = ['site-a.com', 'site-b.org'];
  w.chrome = {
    storage: {
      local: {
        get: (keys, cb) => cb({ settings: {}, neverSave: storedExceptions }),
        set: (data, cb) => {
          if (data.neverSave) storedExceptions = data.neverSave;
          if (cb) cb();
        },
      },
    },
    runtime: {
      connect: () => {
        let listener = null;
        return {
          postMessage(msg) {
            if (listener) setTimeout(() => listener({ rid: msg.rid, ok: true, result: { shortcut: 'Cmd+Shift+X' } }), 0);
          },
          onMessage: { addListener(fn) { listener = fn; } },
          onDisconnect: { addListener() {} },
        };
      },
    },
  };

  w.eval(fs.readFileSync(path.join(root, 'lib', 'i18n.js'), 'utf8'));
  w.eval(fs.readFileSync(path.join(root, 'lib', 'onboard-common.js'), 'utf8'));
  w.eval(fs.readFileSync(path.join(root, 'options', 'options.js'), 'utf8'));

  await new Promise((r) => setTimeout(r, 60));

  const rows = w.document.querySelectorAll('#never-save-list .t');
  assert.strictEqual(rows.length, 2);
  assert.ok(rows[0].textContent.includes('site-a.com'));
  assert.ok(rows[1].textContent.includes('site-b.org'));
  assert.strictEqual(w.document.getElementById('never-save-empty').style.display, 'none');

  // Click Remove on site-a.com
  const removeBtn = rows[0].querySelector('button.sec');
  assert.ok(removeBtn);
  removeBtn.click();

  await new Promise((r) => setTimeout(r, 60));

  assert.strictEqual(storedExceptions.length, 1);
  assert.strictEqual(storedExceptions[0], 'site-b.org');
  assert.strictEqual(w.document.querySelectorAll('#never-save-list .t').length, 1);

  // Click Remove on site-b.org -> empty state shown
  const secondBtn = w.document.querySelector('#never-save-list button.sec');
  secondBtn.click();
  await new Promise((r) => setTimeout(r, 60));

  assert.strictEqual(storedExceptions.length, 0);
  assert.strictEqual(w.document.querySelectorAll('#never-save-list .t').length, 0);
  assert.strictEqual(w.document.getElementById('never-save-empty').style.display, 'block');
  assert.strictEqual(w.document.getElementById('never-save-empty').textContent, 'No sites excluded');
  w.close();
});

test('popup: shows saving is off note with undo for excepted site', async () => {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;

  let storedNeverSave = ['example.com'];
  w.__gitpassMockNeverSave = storedNeverSave;
  const fakeItems = [{ id: 'v1:1', title: 'Other', category: 'login', subtitle: 'u', url: 'https://other.com' }];
  w.__gitpassMock = (msg) => {
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: fakeItems });
      case 'match_active': return ok({ items: [], origin: 'https://login.example.com' });
      case 'get_item': return ok({ item: { ...fakeItems[0], fields: [], tags: [] } });
      default: return ok({});
    }
  };

  w.eval("'use strict';\n" + ['lib/i18n.js', 'lib/icons.js', 'lib/policy.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'].map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));

  await new Promise((r) => setTimeout(r, 80));

  // Switch filter to suggested
  const filter = w.document.querySelector('#filterChips [data-filter-id="suggested"]');
  assert.ok(filter, 'expected Suggested filter chip');
  filter.click();

  await new Promise((r) => setTimeout(r, 60));

  // The site is excepted and empty -> note with Undo must appear
  const note = w.document.querySelector('.empty-excepted-note');
  assert.ok(note, 'expected .empty-excepted-note in popup empty state');
  assert.ok(note.textContent.includes('Saving is off for this site'));
  const undoBtn = note.querySelector('button');
  assert.ok(undoBtn, 'expected Undo button');

  // Click Undo
  undoBtn.click();
  await new Promise((r) => setTimeout(r, 80));

  assert.ok(!w.__gitpassMockNeverSave.includes('example.com'), 'undo should remove example.com from exceptions');
  w.close();
});
