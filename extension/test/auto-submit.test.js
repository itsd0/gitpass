// Auto-submit decision for issue #231: opt-in "submit after fill" for logins.
// The decision itself lives in lib/forms.js (pure DOM, jsdom-loadable); the opt-in setting
// and the per-site exception list are enforced by the content script around it.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const F = require('../lib/forms.js');
const O = { ignoreLayout: true }; // jsdom has no layout

function page(name) {
  const html = fs.readFileSync(path.join(__dirname, 'pages', name), 'utf8');
  return new JSDOM(html, { url: 'https://example.com/' }).window.document;
}
const byScope = (forms, doc, id) => forms.find((f) => (id ? f.scope.id === id : f.scope === doc));
// Filled flags exactly as the content script observes them after a fill.
function filledOf(form) {
  const pw = form.fields.password || form.fields.current;
  return {
    username: !!(form.fields.username && form.fields.username.value),
    password: !!(pw && pw.value),
  };
}
function setValues(form, username, password) {
  if (form.fields.username) form.fields.username.value = username || '';
  const pw = form.fields.password || form.fields.current;
  if (pw) pw.value = password || '';
  return filledOf(form);
}
const ORIGINS = { ignoreLayout: true, pageOrigin: 'https://example.com', fillOrigin: 'https://example.com' };

test('full login: both fields filled + one submit -> submit', () => {
  const doc = page('login.html');
  const f = byScope(F.detectForms(doc, O), doc, 'login');
  assert.strictEqual(f.kind, 'login');
  assert.strictEqual(F.shouldAutoSubmit(f, setValues(f, 'alice@example.com', 'pw-123'), ORIGINS), true);
});

test('full login: only one field filled -> fill and stop', () => {
  const doc = page('login.html');
  const f = byScope(F.detectForms(doc, O), doc, 'login');
  assert.strictEqual(F.shouldAutoSubmit(f, setValues(f, 'alice@example.com', ''), ORIGINS), false);
  assert.strictEqual(F.shouldAutoSubmit(f, setValues(f, '', 'pw-123'), ORIGINS), false);
});

test('full login: two visible submit controls -> no submit', () => {
  const doc = page('login.html');
  const form = doc.getElementById('login');
  const extra = doc.createElement('button'); extra.type = 'submit'; extra.textContent = 'Other'; form.append(extra);
  const f = byScope(F.detectForms(doc, O), doc, 'login');
  assert.strictEqual(F.visibleSubmitControls(form, O).length, 2);
  assert.strictEqual(F.shouldAutoSubmit(f, setValues(f, 'alice@example.com', 'pw-123'), ORIGINS), false);
});

test('an image input counts as a second submit control', () => {
  const doc = page('login.html');
  const form = doc.getElementById('login');
  const img = doc.createElement('input'); img.type = 'image'; img.alt = 'Go'; form.append(img);
  const f = byScope(F.detectForms(doc, O), doc, 'login');
  assert.strictEqual(F.visibleSubmitControls(form, O).length, 2);
  assert.strictEqual(F.shouldAutoSubmit(f, setValues(f, 'alice@example.com', 'pw-123'), ORIGINS), false);
});

test('hidden and disabled submit controls do not count', () => {
  const doc = page('login.html');
  const form = doc.getElementById('login');
  const hidden = doc.createElement('button'); hidden.type = 'submit'; hidden.style.display = 'none'; form.append(hidden);
  const disabled = doc.createElement('input'); disabled.type = 'submit'; disabled.disabled = true; form.append(disabled);
  const f = byScope(F.detectForms(doc, O), doc, 'login');
  assert.strictEqual(F.visibleSubmitControls(form, O).length, 1);
  assert.strictEqual(F.shouldAutoSubmit(f, setValues(f, 'alice@example.com', 'pw-123'), ORIGINS), true);
});

test('button with no type counts as the submit control; type=button does not', () => {
  const doc = new JSDOM(`<form id="f"><input name="username" autocomplete="username"><input name="password" type="password" autocomplete="current-password"><button>Go</button></form>`, { url: 'https://example.com/' }).window.document;
  const f = F.detectForms(doc, O).find((x) => x.scope.id === 'f');
  assert.strictEqual(f.kind, 'login');
  assert.strictEqual(F.visibleSubmitControls(doc.getElementById('f'), O).length, 1);
  assert.strictEqual(F.shouldAutoSubmit(f, setValues(f, 'u', 'p'), ORIGINS), true);

  const doc2 = new JSDOM(`<form id="f"><input name="username" autocomplete="username"><input name="password" type="password" autocomplete="current-password"><button type="button">Go</button></form>`, { url: 'https://example.com/' }).window.document;
  const f2 = F.detectForms(doc2, O).find((x) => x.scope.id === 'f');
  assert.strictEqual(F.visibleSubmitControls(doc2.getElementById('f'), O).length, 0);
  assert.strictEqual(F.shouldAutoSubmit(f2, setValues(f2, 'u', 'p'), ORIGINS), false);
});

test('mismatched fill origin -> no submit; origin known on only one side -> no submit', () => {
  const doc = page('login.html');
  const f = byScope(F.detectForms(doc, O), doc, 'login');
  const filled = setValues(f, 'alice@example.com', 'pw-123');
  assert.strictEqual(F.shouldAutoSubmit(f, filled, { ignoreLayout: true, pageOrigin: 'https://example.com', fillOrigin: 'https://other.example' }), false);
  assert.strictEqual(F.shouldAutoSubmit(f, filled, { ignoreLayout: true, pageOrigin: 'https://example.com' }), false);
  assert.strictEqual(F.shouldAutoSubmit(f, filled, { ignoreLayout: true, fillOrigin: 'https://example.com' }), false);
});

test('multi-step: username step submits its step; password step submits its step', () => {
  const doc = page('multi-step.html');
  const step1 = F.detectForms(doc, O)[0];
  assert.strictEqual(step1.kind, 'login_username');
  assert.strictEqual(F.shouldAutoSubmit(step1, setValues(step1, 'alice@example.com', ''), ORIGINS), true);
  assert.strictEqual(F.shouldAutoSubmit(step1, setValues(step1, '', ''), ORIGINS), false);

  doc.getElementById('step1').style.display = 'none';
  doc.getElementById('step2').style.display = '';
  const step2 = F.detectForms(doc, O)[0];
  assert.strictEqual(step2.kind, 'login');
  assert.strictEqual(step2.fields.username, null);
  assert.strictEqual(F.shouldAutoSubmit(step2, setValues(step2, '', 'pw-123'), ORIGINS), true);
  assert.strictEqual(F.shouldAutoSubmit(step2, setValues(step2, '', ''), ORIGINS), false);
});

test('never auto-submits cards, signup, change-password, totp, or formless groups', () => {
  const cardDoc = page('card.html');
  const card = F.detectForms(cardDoc, O).find((x) => x.kind === 'card');
  assert.ok(card, 'card form detected');
  assert.strictEqual(F.shouldAutoSubmit(card, { username: true, password: true }, ORIGINS), false);

  const signupDoc = page('signup.html');
  const signup = byScope(F.detectForms(signupDoc, O), signupDoc, 'signup');
  assert.strictEqual(signup.kind, 'signup');
  assert.strictEqual(F.shouldAutoSubmit(signup, { username: true, password: true }, ORIGINS), false);
  const change = byScope(F.detectForms(signupDoc, O), signupDoc, 'change');
  assert.strictEqual(change.kind, 'change_password');
  assert.strictEqual(F.shouldAutoSubmit(change, { username: true, password: true }, ORIGINS), false);

  const loginDoc = page('login.html');
  const formless = byScope(F.detectForms(loginDoc, O), loginDoc, null);
  assert.strictEqual(formless.kind, 'login');
  assert.strictEqual(F.shouldAutoSubmit(formless, { username: true, password: true }, ORIGINS), false,
    'formless group has no <form> to submit');
});

test('options normalizeDomain: trims, lowercases, strips www.', () => {
  const { normalizeDomain } = require('../options/options.js');
  assert.strictEqual(normalizeDomain('  WWW.Example.COM '), 'example.com');
  assert.strictEqual(normalizeDomain('sub.example.com'), 'sub.example.com');
  assert.strictEqual(normalizeDomain(''), '');
});
