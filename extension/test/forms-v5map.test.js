'use strict';
// v0.6 integration: host fill_card/fill_identity return gitpass-core v5 field ids; lib/forms.js maps them.
const test = require('node:test');
const assert = require('node:assert');
const F = require('../lib/forms.js');

test('cardValues maps v5 card ids (number, cardholder, cvv, expiry)', () => {
  const v = F.cardValues({ number: '4111 1111 1111 1234', cardholder: 'Test Person', cvv: '123', expiry: '12/30' });
  assert.strictEqual(v.cc_number, '4111 1111 1111 1234');
  assert.strictEqual(v.cc_name, 'Test Person');
  assert.strictEqual(v.cc_csc, '123');
  assert.strictEqual(v.cc_exp_month, '12');
  assert.strictEqual(v.cc_exp_year, '2030');
});

test('identityValues maps v5 identity ids (first, last, address multiline, email, phone, name)', () => {
  const v = F.identityValues({ first: 'Test', last: 'Person', address: '1 Main St\nBangkok', email: 't@example.invalid', phone: '+66 1' });
  assert.strictEqual(v.given_name, 'Test');
  assert.strictEqual(v.family_name, 'Person');
  assert.strictEqual(v.full_name, 'Test Person');
  assert.strictEqual(v.address1, '1 Main St');
  assert.strictEqual(v.email, 't@example.invalid');
  assert.strictEqual(v.phone, '+66 1');
  assert.strictEqual(F.identityValues({ name: 'Solo Name' }).full_name, 'Solo Name');
});

test('lib/icons.js has all 9 categories (v5 + passkey)', () => {
  const I = require('../lib/icons.js');
  assert.deepStrictEqual(I.CATEGORIES.sort(), ['api_credential', 'credit_card', 'document', 'identity', 'login', 'passkey', 'secure_note', 'ssh_key', 'wifi']);
  for (const c of I.CATEGORIES) assert.match(I.categoryIcon(c, 28), /^<svg/);
});
