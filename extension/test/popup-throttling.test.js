// Unit test for extension popup unlock throttling and live countdown (#271)
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');

function createLockedPopup(initialStatus = { unlocked: false, app_running: false, throttled_secs: null }) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;

  let unlockResponse = null;

  w.chrome = {
    runtime: {
      id: 'mock-id',
      lastError: null,
      onMessage: { addListener: () => {} },
      sendMessage: (msg, cb) => {
        if (msg.type === 'popup:status') {
          cb({ ok: true, result: initialStatus });
        } else if (msg.type === 'popup:unlock') {
          if (unlockResponse) cb(unlockResponse);
          else cb({ ok: true, result: { unlocked: true } });
        } else {
          cb({ ok: true, result: {} });
        }
      },
    },
    storage: { local: { get: (k, cb) => cb({}) } },
  };

  for (const f of ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/generator.js', 'popup/popup.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }

  return {
    dom,
    w,
    setUnlockResponse: (resp) => { unlockResponse = resp; },
  };
}

test('popup shows countdown and disables unlock button on throttled error', async () => {
  const { w, setUnlockResponse } = createLockedPopup();
  await new Promise((r) => setTimeout(r, 10));

  // Switch to password mode
  w.document.getElementById('usePw').click();
  assert.ok(!w.document.getElementById('unlockForm').classList.contains('hidden'));

  // Enter password and submit with throttled error response
  w.document.getElementById('pw').value = 'wrong-password';
  setUnlockResponse({
    ok: false,
    error: 'throttled',
    message: 'Too many attempts. Try again in 2 s',
    seconds_remaining: 2,
  });

  w.document.getElementById('unlockForm').requestSubmit();
  await new Promise((r) => setTimeout(r, 20));

  const errEl = w.document.getElementById('unlockErr');
  assert.ok(errEl.textContent.includes('Too many attempts. Try again in 2 s'), `Expected countdown text, got: ${errEl.textContent}`);
  const btn = w.document.getElementById('unlockBtn');
  assert.strictEqual(btn.disabled, true, 'Unlock button should be disabled while throttled');
});

test('popup shows countdown when initial status reports throttled_secs', async () => {
  const { w } = createLockedPopup({ unlocked: false, app_running: false, throttled_secs: 3 });
  await new Promise((r) => setTimeout(r, 20));

  const errEl = w.document.getElementById('unlockErr');
  assert.ok(errEl.textContent.includes('Too many attempts. Try again in 3 s'), `Expected countdown text, got: ${errEl.textContent}`);
  const btn = w.document.getElementById('unlockBtn');
  assert.strictEqual(btn.disabled, true, 'Unlock button should be disabled');
});
