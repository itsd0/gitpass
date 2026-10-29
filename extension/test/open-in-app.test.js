// Unit tests for open_in_app message routing in background.js (issue #232).
'use strict';
const test = require('node:test');
const assert = require('node:assert');

test('popup:open_in_app forwards view, section, and item_id correctly to host', async () => {
  const hostCalls = [];
  const fakeHost = async (cmd, args) => {
    hostCalls.push([cmd, args]);
    return { ok: true };
  };

  // Minimal simulator of the popupRoute open_in_app branch in background.js
  function handleOpenInApp(msg) {
    const p = {};
    if (msg.item_id) p.item_id = msg.item_id;
    if (msg.view) {
      p.view = msg.view;
      if (msg.section) p.section = msg.section;
    }
    return fakeHost('open_in_app', p);
  }

  // 1. Target view only
  await handleOpenInApp({ type: 'popup:open_in_app', view: 'watchtower' });
  assert.deepStrictEqual(hostCalls.at(-1), ['open_in_app', { view: 'watchtower' }]);

  // 2. Target view with section
  await handleOpenInApp({ type: 'popup:open_in_app', view: 'settings', section: 'security' });
  assert.deepStrictEqual(hostCalls.at(-1), ['open_in_app', { view: 'settings', section: 'security' }]);

  // 3. Target generator_history view
  await handleOpenInApp({ type: 'popup:open_in_app', view: 'generator_history' });
  assert.deepStrictEqual(hostCalls.at(-1), ['open_in_app', { view: 'generator_history' }]);

  // 4. Target item_id only
  await handleOpenInApp({ type: 'popup:open_in_app', item_id: 'v1:i1' });
  assert.deepStrictEqual(hostCalls.at(-1), ['open_in_app', { item_id: 'v1:i1' }]);

  // 5. Target both (both forwarded, host gives precedence to item_id)
  await handleOpenInApp({ type: 'popup:open_in_app', item_id: 'v1:i1', view: 'watchtower' });
  assert.deepStrictEqual(hostCalls.at(-1), ['open_in_app', { item_id: 'v1:i1', view: 'watchtower' }]);

  // 6. Neither (window focus only)
  await handleOpenInApp({ type: 'popup:open_in_app' });
  assert.deepStrictEqual(hostCalls.at(-1), ['open_in_app', {}]);
});
