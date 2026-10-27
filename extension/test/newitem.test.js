'use strict';
const test = require('node:test');
const assert = require('node:assert');
const N = require('../popup/newitem.js');

test('types mirror the desktop templates (category + field ids)', () => {
  const ids = N.TYPES.map((t) => t.id);
  assert.deepStrictEqual(ids, ['login', 'api_credential', 'secure_note', 'credit_card', 'identity', 'wifi', 'ssh_key']);
  const fid = (t) => N.sectionsOf(N.typeById(t)).flatMap((s) => s.fields.map((f) => f.id)).sort();
  assert.deepStrictEqual(fid('api_credential'), ['credential', 'expires', 'hostname', 'type', 'username']);
  assert.deepStrictEqual(fid('credit_card'), ['cardholder', 'cvv', 'expiry', 'number', 'pin', 'type']);
  assert.deepStrictEqual(N.sectionsOf(N.typeById('identity')).map((s) => s.id), ['name', 'address', 'contact']);
});

test('title is required for every type', () => {
  for (const t of N.TYPES) assert.strictEqual(N.validate(t.id, {}).field, 'title');
});

test('api credential: key required, url optional but must be http(s)', () => {
  assert.strictEqual(N.validate('api_credential', { title: 'OpenAI' }).field, 'credential');
  assert.strictEqual(N.validate('api_credential', { title: 'OpenAI', credential: 'sk-1' }), null);
  assert.strictEqual(N.validate('api_credential', { title: 'OpenAI', credential: 'sk-1', hostname: 'ftp://x' }).field, 'hostname');
  const it = N.buildItem('api_credential', { title: ' OpenAI ', credential: ' sk-1 ', hostname: 'api.openai.com', notes: 'prod' }, 'v1');
  assert.strictEqual(it.category, 'api_credential'); assert.strictEqual(it.title, 'OpenAI'); assert.strictEqual(it.vaultId, 'v1'); assert.strictEqual(it.notes, 'prod');
  const f = Object.fromEntries(it.sections[0].fields.map((x) => [x.id, x]));
  assert.strictEqual(f.credential.value, ' sk-1 '); // secrets are stored verbatim
  assert.strictEqual(f.credential.kind, 'concealed');
  assert.strictEqual(f.hostname.value, 'https://api.openai.com');
  assert.ok(!('username' in f), 'empty fields are omitted');
});

test('credit card: Luhn, expiry MM/YY, cvv digits', () => {
  assert.strictEqual(N.validate('credit_card', { title: 'Visa', number: '4242 4242 4242 4241' }).field, 'number');
  assert.strictEqual(N.validate('credit_card', { title: 'Visa', number: '4242424242424242', expiry: '13/30' }).field, 'expiry');
  assert.strictEqual(N.validate('credit_card', { title: 'Visa', number: '4242424242424242', cvv: '12' }).field, 'cvv');
  assert.strictEqual(N.validate('credit_card', { title: 'Visa', number: '4242-4242-4242-4242', expiry: '8/2029', cvv: '123' }), null);
  const f = Object.fromEntries(N.buildItem('credit_card', { title: 'Visa', number: '4242 4242 4242 4242', expiry: '8/2029' }).sections[0].fields.map((x) => [x.id, x.value]));
  assert.strictEqual(f.number, '4242424242424242'); assert.strictEqual(f.expiry, '08/29');
});

test('login uses section "login" + top-level url; secure note / wifi / ssh / identity rules', () => {
  const l = N.buildItem('login', { title: 'x', username: 'me', password: 'p', url: 'example.com' });
  assert.strictEqual(l.url, 'https://example.com'); assert.strictEqual(l.sections[0].id, 'login');
  assert.strictEqual(N.validate('login', { title: 'x' }).field, 'password');
  assert.strictEqual(N.validate('secure_note', { title: 'n' }).field, 'note');
  assert.strictEqual(N.buildItem('secure_note', { title: 'n', note: 'a\nb', notes: 'ignored' }).notes, undefined);
  assert.strictEqual(N.validate('wifi', { title: 'w' }).field, 'ssid');
  assert.strictEqual(N.validate('ssh_key', { title: 's', private_key: 'nope' }).field, 'private_key');
  assert.strictEqual(N.validate('identity', { title: 'i' }).field, 'first');
  assert.strictEqual(N.validate('identity', { title: 'i', email: 'bad' }).field, 'email');
  const id = N.buildItem('identity', { title: 'i', first: 'A', email: 'a@b.co' });
  assert.deepStrictEqual(id.sections.map((s) => s.id), ['name', 'contact']);
});
