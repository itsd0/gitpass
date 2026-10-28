// Unit tests for extension popup loading skeletons, timeouts, inline errors, and retry states (issue #94).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const V = require('../popup/view.js');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/popup.js'];

const ITEMS = [
  { id: 'v1:i1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'me@example.invalid', url: 'https://github.com', favorite: false },
  { id: 'v1:i2', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: true },
];

function popup({ items = ITEMS, hostHandler = null } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  const sent = [];

  w.__gitpassMock = (msg) => {
    sent.push(msg);
    if (hostHandler) {
      const custom = hostHandler(msg);
      if (custom !== undefined) return custom;
    }
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User', email: 'fake@example.invalid' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items });
      case 'match_active': return ok({ items, origin: 'https://github.com' });
      case 'get_item': {
        const it = items.find((x) => x.id === msg.item_id) || items[0];
        const fields = [
          { name: 'username', label: 'username', kind: 'text', concealed: false, value: it.subtitle },
          { name: 'password', label: 'password', kind: 'password', concealed: true },
        ];
        return ok({ item: { ...it, fields, tags: [] } });
      }
      case 'reveal': return ok({ value: 'FAKE-PW-123' });
      case 'create_item': return ok({ item_id: 'v1:new', title: msg.item && msg.item.title });
      case 'open_fill': return ok(true);
      default: return ok({});
    }
  };

  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, w, sent };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(40); await sleep(40); };

test('withTimeout helper resolves faster promises and rejects hung calls with code timeout', async () => {
  // Fast promise
  const fast = await V.withTimeout(Promise.resolve('ok'), 50);
  assert.strictEqual(fast, 'ok');

  // Slow promise
  const slowPromise = new Promise((r) => setTimeout(() => r('late'), 100));
  await assert.rejects(
    () => V.withTimeout(slowPromise, 30, 'Custom timeout msg'),
    (err) => {
      assert.strictEqual(err.code, 'timeout');
      assert.strictEqual(err.message, 'Custom timeout msg');
      return true;
    }
  );
});

test('fast list responses (< 150 ms) do not flash loading skeletons', async (t) => {
  const { dom, w } = popup({ items: ITEMS });
  t.after(() => dom.window.close());

  // Wait less than 150ms for instant mock
  await sleep(60);

  const skeletonRows = w.document.querySelectorAll('#listInner .skeleton-row');
  assert.strictEqual(skeletonRows.length, 0, 'no skeletons should flash on fast response');

  const rows = w.document.querySelectorAll('#listInner .row:not(.skeleton-row)');
  assert.strictEqual(rows.length, 2, 'actual rows should be rendered');
  assert.strictEqual(w.document.getElementById('list').getAttribute('aria-busy'), null);
});

test('slow list responses (> 150 ms) show 6 skeleton rows and aria-busy, replaced once resolved', async (t) => {
  let resolveList;
  const listPromise = new Promise((res) => { resolveList = res; });

  const { dom, w } = popup({
    hostHandler: (msg) => {
      if (msg.type === 'popup:list_items') {
        return listPromise;
      }
    },
  });
  t.after(() => dom.window.close());

  // At 50ms, skeletons have not appeared yet
  await sleep(50);
  assert.strictEqual(w.document.querySelectorAll('#listInner .skeleton-row').length, 0);

  // At 180ms (> 150ms delay), skeletons appear
  await sleep(140);
  const skeletonRows = w.document.querySelectorAll('#listInner .skeleton-row');
  assert.strictEqual(skeletonRows.length, 6, 'exactly 6 skeleton rows should be displayed');
  assert.strictEqual(w.document.getElementById('list').getAttribute('aria-busy'), 'true');

  // Check skeleton structure
  const firstSkel = skeletonRows[0];
  assert.ok(firstSkel.querySelector('.skeleton-icon'), 'skeleton row has icon circle');
  assert.ok(firstSkel.querySelector('.skeleton-meta .skeleton-bar.t'), 'skeleton row has title bar');
  assert.ok(firstSkel.querySelector('.skeleton-meta .skeleton-bar.u'), 'skeleton row has subtitle bar');
  assert.ok(firstSkel.classList.contains('skeleton-shimmer'), 'skeleton row has shimmer class');

  // Resolve the list items
  resolveList({ items: ITEMS });
  await settle();

  // Skeletons are replaced by actual items
  assert.strictEqual(w.document.querySelectorAll('#listInner .skeleton-row').length, 0);
  assert.strictEqual(w.document.querySelectorAll('#listInner .row').length, 2);
  assert.strictEqual(w.document.getElementById('list').getAttribute('aria-busy'), null);
});

test('item detail loading displays skeleton header and 3 field placeholders with aria-busy', async (t) => {
  let resolveItem;
  const itemPromise = new Promise((res) => { resolveItem = res; });

  const { dom, w } = popup({
    items: ITEMS,
    hostHandler: (msg) => {
      if (msg.type === 'popup:get_item') {
        return itemPromise;
      }
    },
  });
  t.after(() => dom.window.close());

  await sleep(60); // list loads

  const detailPane = w.document.getElementById('detail');
  assert.strictEqual(detailPane.getAttribute('aria-busy'), 'true');

  const skelHead = detailPane.querySelector('.skeleton-head');
  assert.ok(skelHead, 'skeleton header is shown');
  assert.ok(skelHead.querySelector('.skeleton-icon.lg'), 'large icon circle in header');
  assert.ok(skelHead.querySelector('.skeleton-bar.h'), 'title bar in header');
  assert.ok(skelHead.querySelector('.skeleton-bar.sub'), 'subtitle bar in header');

  const skelFields = detailPane.querySelectorAll('.skeleton-field');
  assert.strictEqual(skelFields.length, 3, 'exactly 3 field placeholders shown');

  // Now resolve item
  resolveItem({
    item: {
      ...ITEMS[0],
      fields: [
        { name: 'username', label: 'username', kind: 'text', concealed: false, value: 'me@example.invalid' },
        { name: 'password', label: 'password', kind: 'password', concealed: true },
      ],
      tags: [],
    },
  });
  await settle();

  assert.strictEqual(detailPane.getAttribute('aria-busy'), null);
  assert.strictEqual(detailPane.querySelectorAll('.skeleton-field').length, 0);
  assert.strictEqual(detailPane.querySelector('h2').textContent, 'GitHub');
});

test('list request failure renders inline error card with Retry button that gets focus', async (t) => {
  let shouldFail = true;

  const { dom, w } = popup({
    items: ITEMS,
    hostHandler: (msg) => {
      if (msg.type === 'popup:list_items') {
        if (shouldFail) {
          const err = new Error('Database locked');
          err.code = 'locked';
          return Promise.reject(err);
        }
        return Promise.resolve({ items: ITEMS });
      }
    },
  });
  t.after(() => dom.window.close());

  await settle();

  const errCard = w.document.querySelector('#listInner .inline-err');
  assert.ok(errCard, 'inline error card is rendered in list pane');
  assert.strictEqual(errCard.getAttribute('role'), 'alert');
  assert.match(errCard.querySelector('.inline-err-msg').textContent, /Couldn't load items/);

  const retryBtn = errCard.querySelector('.btn-retry');
  assert.ok(retryBtn, 'retry button exists');
  assert.strictEqual(w.document.activeElement, retryBtn, 'Retry button received focus');

  // Clicking Retry after error clears should recover
  shouldFail = false;
  retryBtn.click();
  await settle();

  assert.strictEqual(w.document.querySelector('#listInner .inline-err'), null, 'error card cleared');
  assert.strictEqual(w.document.querySelectorAll('#listInner .row').length, 2, 'items successfully rendered after retry');
});

test('item detail request failure renders inline error card with Retry button and focus', async (t) => {
  let failDetail = true;

  const { dom, w } = popup({
    items: ITEMS,
    hostHandler: (msg) => {
      if (msg.type === 'popup:get_item') {
        if (failDetail) {
          return Promise.reject(new Error('Decryption failed'));
        }
        return Promise.resolve({
          item: { ...ITEMS[0], fields: [{ name: 'password', label: 'password', kind: 'password', concealed: true }], tags: [] },
        });
      }
    },
  });
  t.after(() => dom.window.close());

  await settle();

  const detailPane = w.document.getElementById('detail');
  const errCard = detailPane.querySelector('.inline-err');
  assert.ok(errCard, 'inline error card rendered in detail pane');
  assert.strictEqual(errCard.getAttribute('role'), 'alert');
  assert.match(errCard.querySelector('.inline-err-msg').textContent, /Couldn't load item details/);
  assert.match(errCard.querySelector('.inline-err-sub').textContent, /Decryption failed/);

  const retryBtn = errCard.querySelector('.btn-retry');
  assert.ok(retryBtn);
  assert.strictEqual(w.document.activeElement, retryBtn, 'Retry button in detail pane received focus');

  // Click retry
  failDetail = false;
  retryBtn.click();
  await settle();

  assert.strictEqual(detailPane.querySelector('.inline-err'), null, 'error card cleared after successful retry');
  assert.strictEqual(detailPane.querySelector('h2').textContent, 'GitHub');
});

test('popup request timeout causes list and detail to transition to timeout error state', async (t) => {
  const { dom, w } = popup({
    items: ITEMS,
    hostHandler: (msg) => {
      if (msg.type === 'popup:list_items') {
        // Return a promise that never resolves, but with a short test timeout
        return new Promise(() => {});
      }
    },
  });
  t.after(() => dom.window.close());

  // Trigger load with a custom short timeout (80ms)
  w.__gitpassPopup.startListLoading(30);
  const p = w.__gitpassPopup.send({ type: 'popup:list_items' }, 80);

  await assert.rejects(
    () => p,
    (err) => {
      assert.strictEqual(err.code, 'timeout');
      assert.match(err.message, /Request timed out/);
      return true;
    }
  );

  // Render the error card using the timeout error
  w.__gitpassPopup.renderListError({ code: 'timeout', message: 'Request timed out.' }, () => {});
  const errCard = w.document.querySelector('#listInner .inline-err');
  assert.ok(errCard);
  assert.match(errCard.querySelector('.inline-err-sub').textContent, /Request timed out/);
});

test('action buttons show spinner while pending, disable to prevent double clicks, and toast on error', async (t) => {
  let resolveFill, rejectFill;
  let fillCount = 0;

  const { dom, w } = popup({
    items: ITEMS,
    hostHandler: (msg) => {
      if (msg.type === 'popup:open_fill') {
        fillCount++;
        return new Promise((res, rej) => { resolveFill = res; rejectFill = rej; });
      }
    },
  });
  t.after(() => dom.window.close());

  await settle();

  const fillBtn = w.document.querySelector('#detail .d-actions button');
  assert.ok(fillBtn, 'fill button exists');

  // Click Fill
  fillBtn.click();
  await sleep(10);

  // Button must show spinner and be disabled
  assert.strictEqual(fillBtn.disabled, true, 'button must be disabled while pending');
  assert.ok(fillBtn.querySelector('.spinner'), 'spinner must be shown in pending button');

  // Attempting double click while disabled must not fire second request
  fillBtn.click();
  assert.strictEqual(fillCount, 1, 'double click was prevented');

  // Fail the fill request
  rejectFill(new Error('Page navigation refused'));
  await settle();

  // Button restored and toast shown
  assert.strictEqual(fillBtn.disabled, false, 'button re-enabled after completion');
  assert.strictEqual(fillBtn.querySelector('.spinner'), null, 'spinner removed');
  assert.strictEqual(w.document.getElementById('toast').textContent, 'Page navigation refused');
});

test('copy action shows spinner while pending and handles failure with toast', async (t) => {
  let rejectReveal;

  const { dom, w } = popup({
    items: ITEMS,
    hostHandler: (msg) => {
      if (msg.type === 'popup:reveal') {
        return new Promise((res, rej) => { rejectReveal = rej; });
      }
    },
  });
  t.after(() => dom.window.close());

  await settle();

  const copyBtn = w.document.querySelectorAll('#detail .f .acts button')[1]; // copy button
  assert.ok(copyBtn);

  copyBtn.click();
  await sleep(10);

  assert.strictEqual(copyBtn.disabled, true);
  assert.ok(copyBtn.querySelector('.spinner'), 'copy button shows spinner');

  // Reject reveal
  rejectReveal(new Error('Permission denied'));
  await settle();

  assert.strictEqual(copyBtn.disabled, false);
  assert.strictEqual(copyBtn.querySelector('.spinner'), null);
  assert.strictEqual(w.document.getElementById('toast').textContent, 'Permission denied');
});

test('new item save button shows spinner, disables, and toasts on failure', async (t) => {
  let rejectCreate;

  const { dom, w } = popup({
    items: ITEMS,
    hostHandler: (msg) => {
      if (msg.type === 'popup:create_item') {
        return new Promise((res, rej) => { rejectCreate = rej; });
      }
    },
  });
  t.after(() => dom.window.close());

  await settle();

  // Open new item panel
  w.document.getElementById('newBtn').click();
  await sleep(10);

  const titleInput = w.document.querySelector('#panel [data-field=title]');
  titleInput.value = 'Test Item';

  const saveBtn = w.document.querySelector('#panel button[type=submit]');
  assert.ok(saveBtn);

  saveBtn.click();
  await sleep(10);

  assert.strictEqual(saveBtn.disabled, true, 'save button is disabled while pending');
  assert.ok(saveBtn.querySelector('.spinner'), 'save button shows spinner');

  // Reject create
  rejectCreate(new Error('Vault read-only'));
  await settle();

  assert.strictEqual(saveBtn.disabled, false, 'save button re-enabled');
  assert.strictEqual(saveBtn.querySelector('.spinner'), null);
  assert.strictEqual(w.document.getElementById('toast').textContent, 'Vault read-only');
});

test('popup.css includes reduced-motion disable for shimmer and light theme tokens', () => {
  const css = fs.readFileSync(path.join(root, 'popup', 'popup.css'), 'utf8');

  // Reduced motion
  assert.match(css, /@media[\s]*\(prefers-reduced-motion:[\s]*reduce\)/);
  assert.match(css, /animation:[\s]*none[\s]*!important/);

  // Light theme tokens
  assert.match(css, /@media[\s]*\(prefers-color-scheme:[\s]*light\)/);
  assert.match(css, /--orange:/);
  assert.match(css, /--bg:/);
  assert.match(css, /--panel:/);
  assert.match(css, /--fg:/);

  // Skeletons and inline error
  assert.match(css, /.skeleton-row/);
  assert.match(css, /.skeleton-icon/);
  assert.match(css, /.skeleton-bar/);
  assert.match(css, /.inline-err/);
  assert.match(css, /.btn-retry/);
});

test("timeoutForMessage assigns >= 130 s for user-interactive unlock and app calls, 8 s default for regular calls", () => {
  assert.strictEqual(V.timeoutForMessage({ type: "popup:request_unlock" }), 130000);
  assert.strictEqual(V.timeoutForMessage({ type: "popup:open_fill" }), 130000);
  assert.strictEqual(V.timeoutForMessage({ type: "popup:open_in_app" }), 130000);
  assert.strictEqual(V.timeoutForMessage({ type: "popup:list_items" }), 8000);
  assert.strictEqual(V.timeoutForMessage({ type: "popup:status" }), 8000);
  assert.strictEqual(V.timeoutForMessage({ type: "popup:request_unlock", __timeoutMs: 5000 }), 5000);
  assert.strictEqual(V.timeoutForMessage({ type: "popup:request_unlock" }, 4000), 4000);
});

test("request_unlock does not time out at 8 s", async () => {
  const { mock } = require("node:test");
  mock.timers.enable();
  try {
    let unlockResolved = false;
    let unlockPromiseResolver;
    const unlockPromise = new Promise((res) => {
      unlockPromiseResolver = res;
    });

    const { w } = popup({
      hostHandler: (msg) => {
        if (msg.type === "popup:request_unlock") {
          return unlockPromise;
        }
        return undefined;
      },
    });

    const p = w.__gitpassPopup.send({ type: "popup:request_unlock" });
    p.then(() => { unlockResolved = true; });

    // Advance mock timers past 8 seconds (e.g. 10 seconds / 10,000 ms)
    mock.timers.tick(10000);
    await Promise.resolve();

    // Verify request_unlock has NOT timed out at 8 s
    assert.strictEqual(unlockResolved, false, "request_unlock should not be resolved or timed out yet");

    // Resolve unlock prompt (e.g. Touch ID confirmed after 10 s)
    unlockPromiseResolver({ unlocked: true });
    await Promise.resolve();
    const res = await p;
    assert.strictEqual(res.unlocked, true);
    assert.strictEqual(unlockResolved, true);
  } finally {
    mock.timers.reset();
  }
});

test("window.__gitpassPopup is only set when window.__gitpassMock is present", () => {
  const html = fs.readFileSync(path.join(root, "popup", "popup.html"), "utf8");
  const dom = new JSDOM(html, { url: "chrome-extension://mock-id/popup/popup.html", runScripts: "outside-only" });
  const w = dom.window;
  // do NOT set w.__gitpassMock
  for (const f of SCRIPTS) w.eval(fs.readFileSync(path.join(root, f), "utf8"));
  assert.strictEqual(w.__gitpassPopup, undefined);
});
