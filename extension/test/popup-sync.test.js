// Unit tests for real-time sync between host events and extension popup / background (#42)
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');

test('background.js forwards native host changed/lock/unlock events as popup:event', async () => {
  let portMsgListener = null;
  const sentMsgs = [];
  let runtimeMsgListener = null;

  const mockChrome = {
    runtime: {
      connectNative: () => {
        const port = {
          onMessage: { addListener: (fn) => { portMsgListener = fn; } },
          onDisconnect: { addListener: () => {} },
          postMessage: (msg) => {
            // Immediately resolve host requests
            setTimeout(() => {
              if (portMsgListener) portMsgListener({ id: msg.id, ok: true, result: {} });
            }, 0);
          },
        };
        return port;
      },
      sendMessage: async (msg) => { sentMsgs.push(msg); },
      id: 'mock-ext-id',
      onMessage: { addListener: (fn) => { runtimeMsgListener = fn; } },
    },
    idle: { onStateChanged: { addListener: () => {} } },
    tabs: { query: async () => [], sendMessage: async () => {} },
  };

  global.chrome = mockChrome;
  global.GitpassPolicy = { isFillableOrigin: () => false, originOf: () => null };
  global.GitpassPage = { handle: () => {} };
  global.GitpassPasskey = { isUiSender: () => false, fromUi: () => {}, fromPage: () => {} };

  delete require.cache[require.resolve('../background.js')];
  require('../background.js');

  // Trigger connection by sending a popup:status message from an extension page
  assert.ok(runtimeMsgListener, 'runtime onMessage listener must be set');
  await new Promise((r) => runtimeMsgListener({ type: 'popup:status' }, { id: 'mock-ext-id' }, r));

  assert.ok(portMsgListener, 'native port onMessage listener must be set after connect');

  // Trigger changed, lock, unlock events from native host
  portMsgListener({ event: 'changed' });
  portMsgListener({ event: 'lock' });
  portMsgListener({ event: 'unlock' });

  assert.deepStrictEqual(
    sentMsgs,
    [
      { type: 'popup:event', event: 'changed' },
      { type: 'popup:event', event: 'lock' },
      { type: 'popup:event', event: 'unlock' },
    ],
    'unsolicited host events must be forwarded as popup:event messages'
  );
});

function createPopup(initialState = { unlocked: true }) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;

  let messageListener = null;
  const state = { ...initialState };
  let items = [
    { id: 'v1:1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Service Alpha', subtitle: 'alpha@example.com', url: 'http://localhost', favorite: false },
  ];

  w.chrome = {
    runtime: {
      id: 'mock-id',
      lastError: null,
      onMessage: {
        addListener: (fn) => { messageListener = fn; },
      },
      sendMessage: (msg, cb) => {
        if (msg.type === 'popup:status') {
          cb({ ok: true, result: { unlocked: state.unlocked, app_running: true, account: { name: 'Mock User' } } });
        } else if (msg.type === 'popup:list_vaults') {
          cb({ ok: true, result: { vaults: [{ id: 'v1', name: 'Personal', item_count: items.length }] } });
        } else if (msg.type === 'popup:list_items') {
          cb({ ok: true, result: { items } });
        } else if (msg.type === 'popup:match_active') {
          cb({ ok: true, result: { items, origin: 'http://localhost' } });
        } else if (msg.type === 'popup:get_item') {
          const item = items.find((x) => x.id === msg.item_id) || items[0];
          cb({ ok: true, result: { item: { ...item, fields: [], tags: [] } } });
        } else {
          cb({ ok: true, result: {} });
        }
      },
    },
    storage: {
      local: {
        get: (k, cb) => cb({ settings: { autolockFollowsApp: true } }),
      },
    },
  };

  for (const f of ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/generator.js', 'popup/popup.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }

  return {
    dom,
    w,
    state,
    setItems: (newItems) => { items = newItems; },
    emit: (event) => messageListener && messageListener({ type: 'popup:event', event }),
  };
}

test('popup refreshes list on popup:event changed', async () => {
  const { dom, w, setItems, emit } = createPopup({ unlocked: true });

  // Initial load tick
  await new Promise((r) => setTimeout(r, 60));
  assert.ok(w.document.body.textContent.includes('Service Alpha'), 'initial item must be visible');

  // Desktop adds an item and edits Service Alpha
  setItems([
    { id: 'v1:1', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Service Alpha (Renamed)', subtitle: 'alpha@example.com', url: 'http://localhost', favorite: false },
    { id: 'v1:2', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'Service Beta', subtitle: 'beta@example.com', url: 'http://localhost', favorite: false },
  ]);

  emit('changed');

  // liveReload debounces 120ms
  await new Promise((r) => setTimeout(r, 220));

  const text = w.document.body.textContent;
  assert.ok(text.includes('Service Alpha (Renamed)'), 'renamed item title must be reflected');
  assert.ok(text.includes('Service Beta'), 'newly added item must be reflected');

  dom.window.close();
});

test('popup switches to locked on popup:event lock, and restores on unlock', async () => {
  const { dom, w, state, emit } = createPopup({ unlocked: true });

  await new Promise((r) => setTimeout(r, 60));
  assert.ok(!w.document.getElementById('unlocked').classList.contains('hidden'), 'unlocked view initially visible');

  // Desktop locks
  state.unlocked = false;
  emit('lock');
  await new Promise((r) => setTimeout(r, 60));

  assert.ok(w.document.getElementById('unlocked').classList.contains('hidden'), 'unlocked view must be hidden on lock');
  assert.ok(!w.document.getElementById('locked').classList.contains('hidden'), 'locked view must be shown on lock');

  // Desktop unlocks
  state.unlocked = true;
  emit('unlock');
  await new Promise((r) => setTimeout(r, 60));

  assert.ok(!w.document.getElementById('unlocked').classList.contains('hidden'), 'unlocked view must be restored on unlock');
  assert.ok(w.document.getElementById('locked').classList.contains('hidden'), 'locked view must be hidden on unlock');
  assert.ok(w.document.body.textContent.includes('Service Alpha'), 'item list must be restored');

  dom.window.close();
});
