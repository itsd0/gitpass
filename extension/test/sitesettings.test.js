'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const SS = require('../popup/sitesettings.js');

test('domainOf normalises origins, URLs and hosts', () => {
  assert.equal(SS.domainOf('https://login.example.com/path'), 'example.com');
  assert.equal(SS.domainOf('https://www.example.com'), 'example.com');
  assert.equal(SS.domainOf('example.com'), 'example.com');
  assert.equal(SS.domainOf(''), '');
});

test('setAutoSubmit manages neverAutoSubmit exceptions', async () => {
  globalThis.__gitpassMockNeverAutoSubmit = [];
  await SS.setAutoSubmit('example.com', false); // opt out -> on the never list
  assert.deepEqual(globalThis.__gitpassMockNeverAutoSubmit, ['example.com']);
  await SS.setAutoSubmit('https://login.example.com', false); // idempotent
  assert.deepEqual(globalThis.__gitpassMockNeverAutoSubmit, ['example.com']);
  await SS.setAutoSubmit('example.com', true); // allow again
  assert.deepEqual(globalThis.__gitpassMockNeverAutoSubmit, []);
  delete globalThis.__gitpassMockNeverAutoSubmit;
});

test('setInlineMenuShown manages hiddenInlineMenu', async () => {
  globalThis.__gitpassMockHiddenInlineMenu = [];
  await SS.setInlineMenuShown('example.com', false);
  assert.deepEqual(await SS.getHiddenInlineMenu(), ['example.com']);
  assert.equal(SS.has(await SS.getHiddenInlineMenu(), 'https://app.example.com/x'), true);
  await SS.setInlineMenuShown('example.com', true);
  assert.deepEqual(await SS.getHiddenInlineMenu(), []);
  delete globalThis.__gitpassMockHiddenInlineMenu;
});

test('setSavePrompts manages neverSave', async () => {
  globalThis.__gitpassMockNeverSave = [];
  await SS.setSavePrompts('example.com', false); // stop offering
  assert.deepEqual(await SS.getNeverSave(), ['example.com']);
  await SS.setSavePrompts('example.com', true);
  assert.deepEqual(await SS.getNeverSave(), []);
  delete globalThis.__gitpassMockNeverSave;
});

test('empty input is a no-op', async () => {
  assert.deepEqual(await SS.setInlineMenuShown('', false), []);
});
