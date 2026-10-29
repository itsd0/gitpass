// Unit tests for extension options page: connection status card mapping, never-save remove/undo,
// live event updates, accessibility attributes, and light/dark theme CSS tokens. (Issue #99)
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

const root = path.join(__dirname, '..');
const { connectionState, removeDomain, restoreDomain } = require('../options/options.js');

test('connectionState mapping: handles unlocked, locked, not_found and errors', () => {
  // Error or null / undefined probe -> not_found
  assert.deepStrictEqual(connectionState(null), {
    state: 'not_found',
    labelKey: 'statusAppNotFound',
    defaultLabel: 'Desktop app not found',
    canRetry: true,
  });

  assert.deepStrictEqual(connectionState(undefined, new Error('connect failure')), {
    state: 'not_found',
    labelKey: 'statusAppNotFound',
    defaultLabel: 'Desktop app not found',
    canRetry: true,
  });

  // Host missing -> not_found
  assert.deepStrictEqual(connectionState({ host: false, error: 'no helper' }), {
    state: 'not_found',
    labelKey: 'statusAppNotFound',
    defaultLabel: 'Desktop app not found',
    canRetry: true,
  });

  // Host present, but app not running -> not_found
  assert.deepStrictEqual(connectionState({ host: true, app: false, error: 'app_not_running' }), {
    state: 'not_found',
    labelKey: 'statusAppNotFound',
    defaultLabel: 'Desktop app not found',
    canRetry: true,
  });

  // App running, but locked -> locked
  assert.deepStrictEqual(connectionState({ host: true, app: true, unlocked: false }), {
    state: 'locked',
    labelKey: 'statusAppLocked',
    defaultLabel: 'gitpass is locked',
    canRetry: false,
  });

  // App running and unlocked -> unlocked
  assert.deepStrictEqual(connectionState({ host: true, app: true, unlocked: true }), {
    state: 'unlocked',
    labelKey: 'statusConnectedUnlocked',
    defaultLabel: 'Connected to gitpass (unlocked)',
    canRetry: false,
  });

  // Direct status object format
  assert.deepStrictEqual(connectionState({ app_running: false }), {
    state: 'not_found',
    labelKey: 'statusAppNotFound',
    defaultLabel: 'Desktop app not found',
    canRetry: true,
  });

  assert.deepStrictEqual(connectionState({ app_running: true, unlocked: false }), {
    state: 'locked',
    labelKey: 'statusAppLocked',
    defaultLabel: 'gitpass is locked',
    canRetry: false,
  });

  assert.deepStrictEqual(connectionState({ app_running: true, unlocked: true }), {
    state: 'unlocked',
    labelKey: 'statusConnectedUnlocked',
    defaultLabel: 'Connected to gitpass (unlocked)',
    canRetry: false,
  });
});

test('never-save removeDomain: pure function removes domain case-insensitively', () => {
  const initial = ['github.com', 'example.org', 'login.service.com'];

  const r1 = removeDomain(initial, 'example.org');
  assert.deepStrictEqual(r1, ['github.com', 'login.service.com']);
  // Original array is not mutated
  assert.strictEqual(initial.length, 3);

  // Case-insensitive removal
  const r2 = removeDomain(initial, 'GITHUB.COM');
  assert.deepStrictEqual(r2, ['example.org', 'login.service.com']);

  // Non-existent domain returns copy of list
  const r3 = removeDomain(initial, 'notfound.com');
  assert.deepStrictEqual(r3, initial);

  // Empty/falsy inputs handled safely
  assert.deepStrictEqual(removeDomain([], 'example.com'), []);
  assert.deepStrictEqual(removeDomain(initial, ''), initial);
  assert.deepStrictEqual(removeDomain(null, 'example.com'), []);
});

test('never-save restoreDomain: pure function restores domain at original index without duplicates', () => {
  const initial = ['first.com', 'third.com'];

  // Restore at index 1
  const r1 = restoreDomain(initial, 'second.com', 1);
  assert.deepStrictEqual(r1, ['first.com', 'second.com', 'third.com']);

  // Restore at start (index 0)
  const r2 = restoreDomain(initial, 'zero.com', 0);
  assert.deepStrictEqual(r2, ['zero.com', 'first.com', 'third.com']);

  // Restore at end (omitted index or large index)
  const r3 = restoreDomain(initial, 'fourth.com');
  assert.deepStrictEqual(r3, ['first.com', 'third.com', 'fourth.com']);

  // Restore existing domain prevents duplicates and places at requested index
  const r4 = restoreDomain(['a.com', 'b.com', 'c.com'], 'b.com', 0);
  assert.deepStrictEqual(r4, ['b.com', 'a.com', 'c.com']);

  // Empty/falsy inputs handled safely
  assert.deepStrictEqual(restoreDomain(initial, ''), initial);
  assert.deepStrictEqual(restoreDomain(null, 'a.com'), ['a.com']);
});

function bootOptions(options = {}) {
  const html = fs.readFileSync(path.join(root, 'options', 'options.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/options/options.html', runScripts: 'outside-only' });
  const w = dom.window;

  let storedNeverSave = [...(options.neverSave || ['alpha.com', 'beta.org'])];
  let storedNeverAutoSubmit = [...(options.neverAutoSubmit || [])];
  let storedSettings = { ...(options.settings || { autolockFollowsApp: true, autofill: true, savePrompts: true }) };
  let probeResult = options.probeResult || { host: true, app: true, unlocked: true };
  const messageListeners = [];

  w.chrome = {
    storage: {
      local: {
        get: (keys, cb) => {
          if (Array.isArray(keys) && keys.includes('neverSave')) {
            cb({ neverSave: storedNeverSave });
          } else if (Array.isArray(keys) && keys.includes('neverAutoSubmit')) {
            cb({ neverAutoSubmit: storedNeverAutoSubmit });
          } else if (keys === 'settings' || (Array.isArray(keys) && keys.includes('settings'))) {
            cb({ settings: storedSettings });
          } else {
            cb({ settings: storedSettings, neverSave: storedNeverSave, neverAutoSubmit: storedNeverAutoSubmit });
          }
        },
        set: (data, cb) => {
          if (data.neverSave) storedNeverSave = data.neverSave;
          if (data.neverAutoSubmit) storedNeverAutoSubmit = data.neverAutoSubmit;
          if (data.settings) storedSettings = data.settings;
          if (cb) cb();
        },
      },
    },
    runtime: {
      getManifest: () => ({ version: '0.6.3' }),
      onMessage: {
        addListener: (fn) => messageListeners.push(fn),
      },
      connect: () => {
        let listener = null;
        return {
          postMessage(msg) {
            if (!listener) return;
            if (msg.type === 'onboard-probe') {
              setTimeout(() => listener({ rid: msg.rid, ok: true, result: probeResult }), 0);
            } else if (msg.type === 'onboard-shortcut') {
              setTimeout(() => listener({ rid: msg.rid, ok: true, result: { shortcut: 'Cmd+Shift+X' } }), 0);
            }
          },
          onMessage: { addListener(fn) { listener = fn; } },
          onDisconnect: { addListener() {} },
        };
      },
    },
    tabs: {
      create: (params) => { w.__createdTab = params; },
    },
  };

  w.eval(fs.readFileSync(path.join(root, 'lib', 'i18n.js'), 'utf8'));
  w.eval(fs.readFileSync(path.join(root, 'lib', 'onboard-common.js'), 'utf8'));
  w.eval(fs.readFileSync(path.join(root, 'options', 'options.js'), 'utf8'));

  return {
    dom,
    w,
    setProbe: (p) => { probeResult = p; },
    getStoredNeverSave: () => storedNeverSave,
    getStoredNeverAutoSubmit: () => storedNeverAutoSubmit,
    getStoredSettings: () => storedSettings,
    dispatchMessage: (msg) => {
      for (const fn of messageListeners) fn(msg);
    },
  };
}

test('options DOM: connection status card displays unlocked status without item data', async () => {
  const { w } = bootOptions({ probeResult: { host: true, app: true, unlocked: true } });
  await new Promise((r) => setTimeout(r, 80));

  const statusText = w.document.getElementById('status-text');
  assert.ok(statusText);
  assert.ok(statusText.textContent.includes('Connected to gitpass (unlocked)'));

  const statusDot = w.document.getElementById('status-dot');
  assert.ok(statusDot.classList.contains('unlocked'));

  const troubleshootWrap = w.document.getElementById('status-troubleshoot-wrap');
  assert.ok(troubleshootWrap.classList.contains('hidden'), 'retry/troubleshoot hidden when connected');

  // Verify no item data is loaded or exposed
  assert.strictEqual(w.document.querySelector('.item-title'), null);
  assert.strictEqual(w.document.querySelector('.password'), null);
  w.close();
});

test('options DOM: connection status card handles locked and app-not-found states with retry', async () => {
  const { w, setProbe } = bootOptions({ probeResult: { host: true, app: false, error: 'app_not_running' } });
  await new Promise((r) => setTimeout(r, 80));

  const statusText = w.document.getElementById('status-text');
  const statusDot = w.document.getElementById('status-dot');
  const troubleshootWrap = w.document.getElementById('status-troubleshoot-wrap');
  const retryBtn = w.document.getElementById('status-retry-btn');
  const docsLink = w.document.getElementById('status-docs-link');

  assert.ok(statusText.textContent.includes('Desktop app not found'));
  assert.ok(statusDot.classList.contains('not_found'));
  assert.ok(!troubleshootWrap.classList.contains('hidden'), 'troubleshoot wrap visible when app not found');
  assert.ok(docsLink.href.includes('github.com/itsd0/gitpass'));

  // Test Retry button: update probe to unlocked and click retry
  setProbe({ host: true, app: true, unlocked: true });
  retryBtn.click();
  await new Promise((r) => setTimeout(r, 80));

  assert.ok(statusText.textContent.includes('Connected to gitpass (unlocked)'));
  assert.ok(statusDot.classList.contains('unlocked'));
  assert.ok(troubleshootWrap.classList.contains('hidden'));
  w.close();
});

test('options DOM: live lock/unlock events update status card', async () => {
  const { w, setProbe, dispatchMessage } = bootOptions({ probeResult: { host: true, app: true, unlocked: true } });
  await new Promise((r) => setTimeout(r, 80));

  const statusText = w.document.getElementById('status-text');
  assert.ok(statusText.textContent.includes('Connected to gitpass (unlocked)'));

  // Lock event from background
  setProbe({ host: true, app: true, unlocked: false });
  dispatchMessage({ type: 'popup:event', event: 'lock' });
  await new Promise((r) => setTimeout(r, 80));

  assert.ok(statusText.textContent.includes('gitpass is locked'));
  assert.ok(w.document.getElementById('status-dot').classList.contains('locked'));

  // Unlock event from background
  setProbe({ host: true, app: true, unlocked: true });
  dispatchMessage({ type: 'popup:event', event: 'unlock' });
  await new Promise((r) => setTimeout(r, 80));

  assert.ok(statusText.textContent.includes('Connected to gitpass (unlocked)'));
  assert.ok(w.document.getElementById('status-dot').classList.contains('unlocked'));
  w.close();
});

test('options DOM: never-save list Remove button and Undo flow', async () => {
  const { w, getStoredNeverSave } = bootOptions({ neverSave: ['alpha.com', 'beta.org'] });
  await new Promise((r) => setTimeout(r, 80));

  const list = w.document.getElementById('never-save-list');
  const rows = list.querySelectorAll('.never-save-row');
  assert.strictEqual(rows.length, 2);

  const undoBanner = w.document.getElementById('never-save-undo-banner');
  const undoText = w.document.getElementById('never-save-undo-text');
  const undoBtn = w.document.getElementById('never-save-undo-btn');
  assert.ok(undoBanner.classList.contains('hidden'));

  // Click Remove on alpha.com
  const removeBtn = rows[0].querySelector('button.sec');
  removeBtn.click();
  await new Promise((r) => setTimeout(r, 80));

  // alpha.com is removed from storage and row list
  assert.deepStrictEqual(getStoredNeverSave(), ['beta.org']);
  assert.strictEqual(list.querySelectorAll('.never-save-row').length, 1);

  // Undo banner appears
  assert.ok(!undoBanner.classList.contains('hidden'));
  assert.ok(undoText.textContent.includes('alpha.com'));

  // Click Undo
  undoBtn.click();
  await new Promise((r) => setTimeout(r, 80));

  // alpha.com is restored
  assert.strictEqual(getStoredNeverSave().length, 2);
  assert.ok(getStoredNeverSave().includes('alpha.com'));
  assert.strictEqual(list.querySelectorAll('.never-save-row').length, 2);
  assert.ok(undoBanner.classList.contains('hidden'));
  w.close();
});

test('options DOM: toggle settings persist onchange', async () => {
  const { w, getStoredSettings } = bootOptions();
  await new Promise((r) => setTimeout(r, 80));

  const autofillToggle = w.document.getElementById('autofill');
  assert.strictEqual(autofillToggle.checked, true);

  // Toggle autofill off
  autofillToggle.checked = false;
  autofillToggle.dispatchEvent(new w.Event('change'));
  await new Promise((r) => setTimeout(r, 80));

  assert.strictEqual(getStoredSettings().autofill, false);
  w.close();
});

test('options CSS: tokens match #74 and provide light/dark themes with high contrast', () => {
  const css = fs.readFileSync(path.join(root, 'options', 'options.css'), 'utf8');

  // Dark fallback tokens on :root
  assert.ok(/--bg:\s*#1c1c1e/i.test(css), 'dark --bg token');
  assert.ok(/--accent:\s*#f28c28/i.test(css), 'orange accent token');
  assert.ok(/color-scheme:\s*dark/.test(css), 'color-scheme dark');

  // Light theme overrides in @media (prefers-color-scheme: light)
  const lightBlock = /@media\s*\(prefers-color-scheme:\s*light\)\s*\{([\s\S]*?)\}/.exec(css);
  assert.ok(lightBlock, 'light theme media query');
  assert.ok(/--bg:\s*#f5f6f8/i.test(lightBlock[1]), 'light --bg token');
  assert.ok(/color-scheme:\s*light/.test(lightBlock[1]), 'color-scheme light in media query');

  // Focus rings and reduced motion
  assert.ok(/:focus-visible/.test(css), 'accessible focus-visible styles');
  assert.ok(/prefers-reduced-motion/.test(css), 'respects prefers-reduced-motion');
});

test('options DOM: auto-submit toggle defaults off and persists onchange (#231)', async () => {
  const { w, getStoredSettings } = bootOptions();
  await new Promise((r) => setTimeout(r, 80));

  const toggle = w.document.getElementById('autoSubmitAfterFill');
  assert.ok(toggle, 'autoSubmitAfterFill checkbox exists');
  assert.strictEqual(toggle.checked, false, 'off by default');

  toggle.checked = true;
  toggle.dispatchEvent(new w.Event('change'));
  await new Promise((r) => setTimeout(r, 80));

  assert.strictEqual(getStoredSettings().autoSubmitAfterFill, true);
  w.close();
});

test('options DOM: never-auto-submit list add (normalized) and remove (#231)', async () => {
  const { w, getStoredNeverAutoSubmit } = bootOptions({ neverAutoSubmit: ['alpha.com'] });
  await new Promise((r) => setTimeout(r, 80));

  const list = w.document.getElementById('never-auto-submit-list');
  assert.strictEqual(list.querySelectorAll('.never-save-row').length, 1);

  const input = w.document.getElementById('never-auto-submit-add');
  const addBtn = w.document.getElementById('never-auto-submit-add-btn');
  input.value = '  WWW.Beta.ORG ';
  addBtn.click();
  await new Promise((r) => setTimeout(r, 80));
  assert.strictEqual(getStoredNeverAutoSubmit().join(','), 'alpha.com,beta.org');
  assert.strictEqual(list.querySelectorAll('.never-save-row').length, 2);

  // Adding a duplicate does not duplicate the row
  input.value = 'beta.org';
  addBtn.click();
  await new Promise((r) => setTimeout(r, 80));
  assert.strictEqual(getStoredNeverAutoSubmit().join(','), 'alpha.com,beta.org');

  const removeBtn = list.querySelectorAll('.never-save-row')[0].querySelector('button.sec');
  removeBtn.click();
  await new Promise((r) => setTimeout(r, 80));
  assert.strictEqual(getStoredNeverAutoSubmit().join(','), 'beta.org');
  w.close();
});
