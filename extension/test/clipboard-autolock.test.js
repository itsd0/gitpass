// Unit tests for extension clipboard auto-clear and auto-lock on idle/locked
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');

function createPopupDom() {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  w.__gitpassMock = true;
  w.__gitpassMockClipboard = '';
  w.toast = () => {};
  w.chrome = {
    runtime: {
      onMessage: { addListener: () => {} },
      sendMessage: (msg, cb) => {
        if (msg.type === 'popup:status') cb({ ok: true, result: { unlocked: true } });
        else if (msg.type === 'popup:lock') cb({ ok: true, result: {} });
        else cb({ ok: true, result: {} });
      },
    },
    storage: { local: { get: (k, cb) => cb({ settings: {} }) } },
  };
  for (const f of ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/popup.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }
  return dom;
}

test('clipboard auto-clears after timer only if unchanged', async () => {
  const dom = createPopupDom();
  const w = dom.window;

  // Copy with a short 50ms clearSecs (0.05s)
  await w.copy('secret-pw-123', 'Copied', 0.05);
  assert.equal(w.__gitpassMockClipboard, 'secret-pw-123');

  // Wait for timer to expire
  await new Promise((r) => setTimeout(r, 80));
  assert.equal(w.__gitpassMockClipboard, '', 'clipboard should be cleared after timeout');
  dom.window.close();
});

test('clipboard is NOT cleared if user copied something else', async () => {
  const dom = createPopupDom();
  const w = dom.window;

  await w.copy('secret-pw-456', 'Copied', 0.05);
  assert.equal(w.__gitpassMockClipboard, 'secret-pw-456');

  // External clipboard change before timer fires
  w.__gitpassMockClipboard = 'user-copied-other-text';

  await new Promise((r) => setTimeout(r, 80));
  assert.equal(w.__gitpassMockClipboard, 'user-copied-other-text', 'external clipboard must remain untouched');
  dom.window.close();
});

test('clearClipboardIfOurs clears immediately on vault lock if unchanged', async () => {
  const dom = createPopupDom();
  const w = dom.window;

  await w.copy('secret-on-lock', 'Copied', 60); // 60s timer
  assert.equal(w.__gitpassMockClipboard, 'secret-on-lock');

  // Vault locks
  const cleared = await w.clearClipboardIfOurs();
  assert.equal(cleared, true);
  assert.equal(w.__gitpassMockClipboard, '');
  dom.window.close();
});

test('chrome.idle listener sends host lock on screen lock', async () => {
  let stateListener = null;
  const sentHostCmds = [];
  const sentRuntimeMsgs = [];

  const mockChrome = {
    runtime: {
      connectNative: () => ({
        onMessage: { addListener: () => {} },
        onDisconnect: { addListener: () => {} },
        postMessage: (msg) => {
          sentHostCmds.push(msg);
          Promise.resolve().then(() => {
            const p = mockChrome.runtime._lastPort;
            if (p && p._msgHandler) p._msgHandler({ id: msg.id, ok: true, result: {} });
          });
        },
      }),
      sendMessage: async (msg) => { sentRuntimeMsgs.push(msg); },
      id: 'mock-ext-id',
      onMessage: { addListener: () => {} },
    },
    idle: {
      onStateChanged: {
        addListener: (fn) => { stateListener = fn; },
      },
    },
    tabs: {
      query: async () => [],
      sendMessage: async () => {},
    },
  };

  global.chrome = mockChrome;
  global.GitpassPolicy = { isFillableOrigin: () => false, originOf: () => null };
  global.GitpassPage = { handle: () => {} };
  global.GitpassPasskey = { isUiSender: () => false, fromUi: () => {}, fromPage: () => {} };

  delete require.cache[require.resolve('../background.js')];
  require('../background.js');

  assert.ok(stateListener, 'chrome.idle.onStateChanged listener must be registered');

  // Trigger locked state (screen lock / screensaver)
  stateListener('locked');

  // Wait a microtask
  await new Promise((r) => setTimeout(r, 20));

  assert.ok(sentHostCmds.some((c) => c.cmd === 'lock'), 'must send lock command to host on screen lock');
  assert.ok(sentRuntimeMsgs.some((m) => m.type === 'popup:event' && m.event === 'lock'), 'must broadcast popup:event lock');
});

test('chrome.idle listener sends host lock on screen lock', async () => {
  let stateListener = null;
  const sentHostCmds = [];
  const sentRuntimeMsgs = [];

  const mockChrome = {
    runtime: {
      connectNative: () => ({
        onMessage: { addListener: () => {} },
        onDisconnect: { addListener: () => {} },
        postMessage: (msg) => {
          sentHostCmds.push(msg);
          // resolve immediately
          Promise.resolve().then(() => {
            const p = mockChrome.runtime._lastPort;
            if (p && p._msgHandler) p._msgHandler({ id: msg.id, ok: true, result: {} });
          });
        },
      }),
      sendMessage: async (msg) => { sentRuntimeMsgs.push(msg); },
      id: 'mock-ext-id',
      onMessage: { addListener: () => {} },
    },
    idle: {
      onStateChanged: {
        addListener: (fn) => { stateListener = fn; },
      },
    },
    tabs: {
      query: async () => [],
      sendMessage: async () => {},
    },
  };

  global.chrome = mockChrome;
  global.GitpassPolicy = { isFillableOrigin: () => false, originOf: () => null };
  global.GitpassPage = { handle: () => {} };
  global.GitpassPasskey = { isUiSender: () => false, fromUi: () => {}, fromPage: () => {} };

  delete require.cache[require.resolve('../background.js')];
  require('../background.js');

  assert.ok(stateListener, 'chrome.idle.onStateChanged listener must be registered');

  // Trigger locked state (screen lock / screensaver)
  stateListener('locked');

  // Wait a microtask
  await new Promise((r) => setTimeout(r, 20));

  assert.ok(sentHostCmds.some((c) => c.cmd === 'lock'), 'must send lock command to host on screen lock');
  assert.ok(sentRuntimeMsgs.some((m) => m.type === 'popup:event' && m.event === 'lock'), 'must broadcast popup:event lock');
});
