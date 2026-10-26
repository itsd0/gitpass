const test = require('node:test');
const assert = require('node:assert');
const P = require('../lib/policy.js');

test('fillable origins', () => {
  assert.ok(P.isFillableOrigin('https://paypal.com'));
  assert.ok(P.isFillableOrigin('http://localhost:3000'));
  assert.ok(P.isFillableOrigin('http://127.0.0.1:8080'));
  assert.ok(!P.isFillableOrigin('http://paypal.com'));
  assert.ok(!P.isFillableOrigin('file:///etc/passwd'));
  assert.ok(!P.isFillableOrigin('data:text/html,hi'));
  assert.ok(!P.isFillableOrigin('javascript:alert(1)'));
  assert.ok(!P.isFillableOrigin('not a url'));
});

test('originOf', () => {
  assert.strictEqual(P.originOf('https://www.paypal.com/signin?x=1'), 'https://www.paypal.com');
  assert.strictEqual(P.originOf('https://paypal.com.evil.io/'), 'https://paypal.com.evil.io');
  assert.strictEqual(P.originOf('data:text/html,x'), null);
  assert.strictEqual(P.originOf('garbage'), null);
});

// Minimal fake DOM for form detection.
function input(type, form, visible = true) {
  return { type, form, disabled: false, readOnly: false, getBoundingClientRect: () => ({ width: visible ? 100 : 0, height: visible ? 20 : 0 }) };
}
test('findLoginFields pairs username with password', () => {
  const form = { inputs: [] };
  const hidden = input('text', form, false), user = input('email', form), cb = input('checkbox', form), pw = input('password', form);
  form.inputs = [hidden, user, cb, pw];
  form.querySelectorAll = () => form.inputs;
  const doc = { querySelectorAll: (s) => (s.includes('password') ? [pw] : form.inputs) };
  const r = P.findLoginFields(doc);
  assert.strictEqual(r.length, 1);
  assert.strictEqual(r[0].password, pw);
  assert.strictEqual(r[0].username, user);
});
test('invisible password fields are ignored', () => {
  const pw = input('password', null, false);
  const doc = { querySelectorAll: () => [pw] };
  assert.strictEqual(P.findLoginFields(doc).length, 0);
});
