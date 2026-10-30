// Caps Lock hint in the popup unlock form (issue #325).
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

const root = path.join(__dirname, '..');
const capslock = require('../popup/capslock.js');

function page() {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  for (const f of ['lib/i18n.js', 'popup/capslock.js']) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return w;
}

// jsdom does not implement the CapsLock modifier, so stub getModifierState like a real browser.
const key = (w, el, caps, type = 'keydown') => {
  const e = new w.KeyboardEvent(type, { bubbles: true });
  Object.defineProperty(e, 'getModifierState', { value: (k) => k === 'CapsLock' && caps });
  el.dispatchEvent(e);
};

test('popup unlock form has the hint wired with aria-describedby', () => {
  const w = page();
  const pw = w.document.getElementById('pw');
  const hint = w.document.getElementById('capsHint');
  assert.ok(hint, 'hint element exists in popup.html');
  assert.strictEqual(pw.getAttribute('aria-describedby'), 'capsHint');
  assert.strictEqual(hint.hidden, true, 'hidden initially');
});

test('hint shows on keydown with CapsLock on, hides on off and on blur', () => {
  const w = page();
  const pw = w.document.getElementById('pw');
  const hint = w.document.getElementById('capsHint');
  capslock.attachCapsLockHint(pw, hint);
  pw.dispatchEvent(new w.FocusEvent('focus'));
  key(w, pw, true);
  assert.strictEqual(hint.hidden, false, 'shown while focused with CapsLock on');
  key(w, pw, false);
  assert.strictEqual(hint.hidden, true, 'hidden when CapsLock goes off');
  key(w, pw, true);
  assert.strictEqual(hint.hidden, false);
  pw.dispatchEvent(new w.FocusEvent('blur'));
  assert.strictEqual(hint.hidden, true, 'hidden on blur');
});

test('hint line is reserved (stays laid out) when hidden', () => {
  const w = page();
  const hint = w.document.getElementById('capsHint');
  assert.strictEqual(hint.className, 'caps-hint');
  const css = fs.readFileSync(path.join(root, 'popup', 'popup.css'), 'utf8');
  assert.match(css, /\.caps-hint\[hidden\]\s*\{[^}]*visibility:\s*hidden/, 'hidden keeps layout via visibility');
});
