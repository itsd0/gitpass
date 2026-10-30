// Popup toast: concealed copies count down and Clear now only wipes our value.
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];

function createPopupDom() {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  w.__gitpassMock = true;
  w.__gitpassMockClipboard = '';
  w.chrome = {
    runtime: {
      onMessage: { addListener: () => {} },
      sendMessage: (msg, cb) => {
        if (msg.type === 'popup:status') cb({ ok: true, result: { unlocked: true } });
        else cb({ ok: true, result: {} });
      },
    },
    storage: { local: { get: (k, cb) => cb({ settings: {} }) } },
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return dom;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

test('concealed copy shows clears in Ns, a polite live region, and an orange progress bar', async () => {
  const dom = createPopupDom();
  const w = dom.window;
  await w.copy('FAKE-secret', 'Password copied', 30, { concealed: true });
  const toast = w.document.getElementById('toast');
  assert.equal(toast.getAttribute('role'), 'status');
  assert.equal(toast.getAttribute('aria-live'), 'polite');
  assert.equal(toast.querySelector('.sr-only').textContent, 'Password copied · clears in 30s');
  assert.equal(toast.querySelector('.toast-visual').getAttribute('aria-hidden'), 'true');
  assert.equal(toast.querySelector('.toast-visual').textContent, 'Password copied · clears in 30s');
  assert.equal(toast.querySelector('button.toast-action').textContent, 'Clear now');
  assert.equal(toast.querySelector('button.toast-action').type, 'button');
  const fill = toast.querySelector('.toast-bar-fill');
  assert.equal(fill.style.width, '100%');
  assert.equal(fill.style.transition, 'width 1s linear');
  assert.equal(w.__gitpassMockClipboard, 'FAKE-secret');
  dom.window.close();
});

test('the countdown ticks once a second without changing the live text', async () => {
  const dom = createPopupDom();
  const w = dom.window;
  await w.copy('FAKE-secret', 'Password copied', 30, { concealed: true });
  await sleep(1100);
  const toast = w.document.getElementById('toast');
  assert.equal(toast.querySelector('.sr-only').textContent, 'Password copied · clears in 30s');
  assert.equal(toast.querySelector('.toast-visual').textContent, 'Password copied · clears in 29s');
  const fill = toast.querySelector('.toast-bar-fill');
  assert.ok(fill.style.width.startsWith('96'), `bar should step down, got ${fill.style.width}`);
  dom.window.close();
});

test('Clear now clears the clipboard only when it still holds our value', async () => {
  const dom = createPopupDom();
  const w = dom.window;
  await w.copy('FAKE-secret', 'Password copied', 30, { concealed: true });
  w.__gitpassMockClipboard = 'user-copied-other';
  w.document.querySelector('#toast button.toast-action').click();
  await sleep(20);
  assert.equal(w.__gitpassMockClipboard, 'user-copied-other');
  assert.equal(w.document.getElementById('toast').classList.contains('hidden'), true);

  await w.copy('FAKE-secret', 'Password copied', 30, { concealed: true });
  w.document.querySelector('#toast button.toast-action').click();
  await sleep(20);
  assert.equal(w.__gitpassMockClipboard, '');
  assert.match(w.document.getElementById('toast').textContent, /Cleared/);
  dom.window.close();
});

test('a username copy keeps the plain toast with no countdown', async () => {
  const dom = createPopupDom();
  const w = dom.window;
  await w.copy('fake@example.invalid', 'Username copied', 30);
  const toast = w.document.getElementById('toast');
  assert.equal(toast.textContent, 'Username copied');
  assert.equal(toast.querySelector('button'), null);
  assert.equal(toast.querySelector('.toast-bar'), null);
  dom.window.close();
});

test('reduced motion does not animate the progress bar', async () => {
  const dom = createPopupDom();
  const w = dom.window;
  w.matchMedia = (q) => ({ matches: String(q).includes('reduce'), addEventListener() {}, removeEventListener() {} });
  await w.copy('FAKE-secret', 'Secret copied', 30, { concealed: true });
  assert.equal(w.document.querySelector('.toast-bar-fill').style.transition, 'none');
  dom.window.close();
});

test('clearing disabled says it will not be cleared automatically', async () => {
  const dom = createPopupDom();
  const w = dom.window;
  await w.copy('FAKE-secret', 'Password copied', 0, { concealed: true });
  const toast = w.document.getElementById('toast');
  assert.match(toast.textContent, /won't be cleared automatically/);
  assert.equal(toast.querySelector('button'), null);
  assert.equal(toast.querySelector('.toast-bar'), null);
  dom.window.close();
});
