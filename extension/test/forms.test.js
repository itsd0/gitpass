// Form detection + fill tests against the local test pages (jsdom).
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const F = require('../lib/forms.js');
const O = { ignoreLayout: true }; // jsdom has no layout

function page(name) {
  const html = fs.readFileSync(path.join(__dirname, 'pages', name), 'utf8');
  return new JSDOM(html, { url: 'https://example.com/' }).window.document; // scripts not run
}
const byScope = (forms, doc, id) => forms.find((f) => (id ? f.scope.id === id : f.scope === doc));

test('login page: form with autocomplete + formless heuristics; search ignored', () => {
  const doc = page('login.html');
  const forms = F.detectForms(doc, O);
  const f = byScope(forms, doc, 'login');
  assert.strictEqual(f.kind, 'login');
  assert.strictEqual(f.fields.username.id, 'em');
  assert.strictEqual(f.fields.password.id, 'pw');
  assert.ok(!forms.some((x) => x.scope.id === 'search'));
  const fl = byScope(forms, doc, null);
  assert.strictEqual(fl.kind, 'login');
  assert.strictEqual(fl.fields.username.id, 'u2');
  assert.strictEqual(fl.fields.password.id, 'p2');
});

test('signup page: sign-up and change-password forms', () => {
  const doc = page('signup.html');
  const forms = F.detectForms(doc, O);
  const s = byScope(forms, doc, 'signup');
  assert.strictEqual(s.kind, 'signup');
  assert.strictEqual(s.fields.newPassword.id, 'np');
  assert.strictEqual(s.fields.confirm.id, 'cp');
  assert.strictEqual(s.fields.username.getAttribute('name'), 'username'); // explicit username beats email
  const c = byScope(forms, doc, 'change');
  assert.strictEqual(c.kind, 'change_password');
  assert.strictEqual(c.fields.current.id, 'cur');
  assert.strictEqual(c.fields.newPassword.id, 'new1');
  assert.strictEqual(c.fields.confirm.id, 'new2');
});

test('multi-step: each step detected as it becomes visible', () => {
  const doc = page('multi-step.html');
  let forms = F.detectForms(doc, O);
  assert.deepStrictEqual(forms.map((f) => f.kind), ['login_username']);
  assert.strictEqual(forms[0].fields.username.id, 'ident');
  doc.getElementById('step1').style.display = 'none';
  doc.getElementById('step2').style.display = '';
  forms = F.detectForms(doc, O);
  assert.deepStrictEqual(forms.map((f) => f.kind), ['login']);
  assert.strictEqual(forms[0].fields.username, null);
  assert.strictEqual(forms[0].fields.password.id, 'pw');
  doc.getElementById('step2').style.display = 'none';
  doc.getElementById('step3').style.display = '';
  forms = F.detectForms(doc, O);
  assert.deepStrictEqual(forms.map((f) => f.kind), ['totp']);
  assert.strictEqual(F.fillTotp(forms[0], '123456'), true);
  assert.strictEqual(doc.getElementById('otp').value, '123456');
});

test('split one-time-code boxes', () => {
  const doc = page('multi-step.html');
  doc.getElementById('step1').style.display = 'none';
  doc.getElementById('split').style.display = '';
  const [f] = F.detectForms(doc, O);
  assert.strictEqual(f.kind, 'totp');
  assert.strictEqual(f.fields.totpDigits.length, 6);
  F.fillTotp(f, '987654');
  assert.deepStrictEqual(Array.from(doc.querySelectorAll('.d')).map((e) => e.value).join(''), '987654');
});

test('card page: card forms (selects + combined expiry) and identity form', () => {
  const doc = page('card.html');
  const forms = F.detectForms(doc, O);
  const p = byScope(forms, doc, 'pay');
  assert.strictEqual(p.kind, 'card');
  assert.deepStrictEqual(Object.keys(p.fields.card).sort(), ['cc_csc', 'cc_exp_month', 'cc_exp_year', 'cc_name', 'cc_number']);
  F.fillCard(p, { number: '4111 1111 1111 1111', cardholder: 'Nay Lin', expiry: '07/28', cvv: '123' });
  assert.strictEqual(doc.getElementById('num').value, '4111111111111111');
  assert.strictEqual(doc.getElementById('mm').value, '07');
  assert.strictEqual(doc.getElementById('yy').value, '2028');
  assert.strictEqual(doc.getElementById('cvc').value, '123');
  const p2 = byScope(forms, doc, 'pay2');
  assert.strictEqual(p2.kind, 'card');
  F.fillCard(p2, { card_number: '5555', exp_month: 3, exp_year: 2029, cvc: '999' });
  assert.strictEqual(doc.getElementById('exp2').value, '03 / 29');
  assert.strictEqual(doc.getElementById('c2').value, '999');
  const id = byScope(forms, doc, 'ship');
  assert.strictEqual(id.kind, 'identity');
  F.fillIdentity(id, { first_name: 'Nay', last_name: 'Lin', address: '1 Sukhumvit', city: 'Bangkok', zip: '10110', country: 'TH', phone: '+66', email: 'a@b.c' });
  for (const [k, v] of Object.entries({ fn: 'Nay', ln: 'Lin', addr: '1 Sukhumvit', city: 'Bangkok', zip: '10110', country: 'TH', tel: '+66', mail: 'a@b.c' }))
    assert.strictEqual(doc.getElementById(k).value, v, k);
});

test('fillLogin dispatches input events and capture reads values', () => {
  const doc = page('login.html');
  const f = byScope(F.detectForms(doc, O), doc, 'login');
  let events = 0; doc.getElementById('pw').addEventListener('input', () => events++);
  assert.strictEqual(F.fillLogin(f, { username: 'me@x.io', password: 'hunter2' }), 2);
  assert.strictEqual(events, 1);
  assert.deepStrictEqual(F.captureCredentials(f), { kind: 'login', username: 'me@x.io', password: 'hunter2' });
});

test('signup capture requires matching confirmation; fillNewPassword fills both', () => {
  const doc = page('signup.html');
  const s = byScope(F.detectForms(doc, O), doc, 'signup');
  doc.getElementById('np').value = 'a'; doc.getElementById('cp').value = 'b';
  assert.strictEqual(F.captureCredentials(s), null);
  assert.strictEqual(F.fillNewPassword(s, 'Str0ng!pw'), 2);
  assert.strictEqual(F.captureCredentials(s).password, 'Str0ng!pw');
});

test('locate maps a focused element to its form and role', () => {
  const doc = page('card.html');
  const forms = F.detectForms(doc, O);
  assert.strictEqual(F.locate(forms, doc.getElementById('num')).role, 'cc_number');
  assert.strictEqual(F.locate(forms, doc.getElementById('city')).form.kind, 'identity');
  assert.strictEqual(F.locate(forms, doc.body), null);
});

test('classifyField edge cases', () => {
  const { document: d } = new JSDOM('<input id=a type=password autocomplete=new-password><input id=b name=q><input id=c type=text autocomplete="shipping postal-code"><input id=d type=password name=pin_code placeholder="SMS code">').window;
  assert.strictEqual(F.classifyField(d.getElementById('a')), 'new_password');
  assert.strictEqual(F.classifyField(d.getElementById('b')), null);
  assert.strictEqual(F.classifyField(d.getElementById('c')), 'postal');
  assert.strictEqual(F.classifyField(d.getElementById('d')), 'totp');
});
