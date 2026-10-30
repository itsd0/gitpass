// One gitpass icon per logical form group (card / login / identity), Stripe-style card iframes,
// and whole-card fill across payment frames.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
// content.js + content/*.js in manifest order, evaluated as ONE script: a browser shares
// top-level let/const across content scripts, but jsdom w.eval calls do not share them.
const CONTENT_SRC = ['content.js', 'content/messaging.js', 'content/settings.js', 'content/detection.js', 'content/totp.js', 'content/dropdown.js', 'content/username.js', 'content/banner.js', 'content/command.js', 'content/prompts.js', 'content/wiring.js'].map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n');

function page(html, url = 'https://shop.example/checkout') {
  const dom = new JSDOM(html, { url, runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  let y = 0;
  const rect = function () { if (this._y == null) this._y = (y += 40); return { left: 10, top: this._y, right: 310, bottom: this._y + 30, width: 300, height: 30 }; };
  w.HTMLElement.prototype.getBoundingClientRect = rect;
  w.chrome = { runtime: { onMessage: { addListener() {} }, sendMessage: async (m) => (m.type === 'page:pending' ? { ok: true, result: { prompt: null } } : { ok: true, result: { origin: 'https://shop.example', items: [] } }) },
    storage: { local: { get(k, cb) { cb({}); }, onChanged: { addListener() {} } } }, i18n: { getMessage: () => '' } };
  return w;
}
const load = (w, files) => { for (const f of files) w.eval(fs.readFileSync(path.join(root, f), 'utf8')); };
const CONTENT = ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'lib/banner.js'];
const decorated = (w) => [...w.document.querySelectorAll('[data-gitpass-decorated]')].map((e) => e.id);

const STRIPE_CHECKOUT = `<!doctype html><html><body><form id="pay">
  <input id="email" type="email" autocomplete="email">
  <input id="cardNumber" name="cardNumber" autocomplete="cc-number" placeholder="1234 1234 1234 1234">
  <input id="cardExpiry" name="cardExpiry" autocomplete="cc-exp" placeholder="MM / YY">
  <input id="cardCvc" name="cardCvc" autocomplete="cc-csc" placeholder="CVC">
  <input id="billingName" name="billingName" autocomplete="cc-name">
  <select id="country" autocomplete="country"><option>TH</option></select>
  <input id="zip" autocomplete="postal-code">
  <button>Pay</button></form></body></html>`;

test('Stripe Checkout card form: exactly one icon, on the card number', async () => {
  const w = page(STRIPE_CHECKOUT); load(w, CONTENT); w.eval(CONTENT_SRC);
  await new Promise((r) => setTimeout(r, 50));
  assert.deepStrictEqual(decorated(w), ['cardNumber']);
  assert.strictEqual(w.document.querySelectorAll('gitpass-ui').length, 1);
  w.close();
});

test('one icon per group: login -> username, identity -> first field, two forms -> two icons', async () => {
  const w = page(`<!doctype html><body>
    <form id="login"><input id="u" autocomplete="username"><input id="p" type="password"><button>Sign in</button></form>
    <form id="addr"><input id="fn" autocomplete="given-name"><input id="ln" autocomplete="family-name"><input id="st" autocomplete="street-address"><input id="ph" autocomplete="tel"></form>`);
  load(w, CONTENT); w.eval(CONTENT_SRC);
  await new Promise((r) => setTimeout(r, 50));
  assert.deepStrictEqual(decorated(w).sort(), ['fn', 'u']);
  w.close();
});

test('Stripe Elements iframes: one card group anchored on the card-number frame; other iframes ignored', () => {
  const w = page(`<!doctype html><body><form>
    <input id="name" autocomplete="cc-name">
    <iframe id="ad" src="https://ads.example/frame" title="ad"></iframe>
    <iframe id="exp" name="__privateStripeFrame5" title="Secure expiration date input frame" src="https://js.stripe.com/v3/elements-inner-card-expiry.html"></iframe>
    <iframe id="num" name="__privateStripeFrame4" title="Secure card number input frame" src="https://js.stripe.com/v3/elements-inner-card-number.html"></iframe>
    <iframe id="ctl" name="__privateStripeController" src="https://js.stripe.com/v3/controller.html"></iframe>
  </form>`);
  load(w, ['lib/policy.js', 'lib/forms.js']);
  const F = w.GitpassForms, forms = F.detectForms(w.document);
  const card = forms.filter((f) => f.kind === 'card');
  assert.strictEqual(card.length, 1);
  assert.strictEqual(card[0].frames.map((f) => f.id).join(), 'num,exp');
  assert.strictEqual(card[0].fields.card.cc_name.id, 'name'); // top-frame cardholder is part of the same group
  const a = F.anchorFields(card[0]);
  assert.strictEqual(a.length, 1); assert.strictEqual(a[0].el.id, 'num'); assert.strictEqual(a[0].role, 'cc_number');
  assert.strictEqual(F.locate(forms, w.document.getElementById('num')).role, 'cc_number');
  w.close();
});

test('fillCardDocument fills whatever card fields a single payment frame holds', () => {
  const w = page(`<!doctype html><body><input id="e" name="exp-date" autocomplete="cc-exp" placeholder="MM / YY"><input id="c" name="cvc" autocomplete="cc-csc"></body>`, 'https://js.stripe.com/v3/x.html');
  load(w, ['lib/policy.js', 'lib/forms.js']);
  const n = w.GitpassForms.fillCardDocument(w.document, { number: '4242424242424242', expiry: '08/29', cvv: '123' });
  assert.strictEqual(n, 2);
  assert.strictEqual(w.document.getElementById('e').value, '08 / 29');
  assert.strictEqual(w.document.getElementById('c').value, '123');
  w.close();
});

test('policy: only https payment-processor hosts count as payment frames', () => {
  const P = require('../lib/policy.js');
  assert.ok(P.isPaymentFrameOrigin('https://js.stripe.com'));
  assert.ok(!P.isPaymentFrameOrigin('http://js.stripe.com'));
  assert.ok(!P.isPaymentFrameOrigin('https://js.stripe.com.evil.example'));
  assert.ok(!P.isPaymentFrameOrigin('https://evil.example'));
  assert.ok(!P.isPaymentFrameOrigin(null));
});

test('fillPaymentFrames sends card data only to allow-listed child frames', async () => {
  globalThis.GitpassPolicy = require('../lib/policy.js');
  const calls = [];
  globalThis.chrome = { scripting: { executeScript: async (o) => {
    calls.push(o);
    if (o.target.allFrames) return [{ frameId: 0, result: 'https://shop.example' }, { frameId: 3, result: 'https://js.stripe.com' }, { frameId: 4, result: 'https://ads.example' }, { frameId: 5, result: 'https://js.stripe.com' }];
    if (o.files) return [];
    return [{ frameId: 3, result: 1 }, { frameId: 5, result: 2 }];
  } } };
  delete require.cache[require.resolve('../bg-page.js')];
  const B = require('../bg-page.js');
  const n = await B.fillPaymentFrames(7, { number: '4242424242424242', expiry: '08/29', cvv: '123' });
  assert.strictEqual(n, 3);
  assert.strictEqual(calls[0].args, undefined, 'the probe carries no card data');
  for (const c of calls.slice(1)) assert.deepStrictEqual(c.target, { tabId: 7, frameIds: [3, 5] });
  // no payment frames -> no injection at all
  calls.length = 0;
  globalThis.chrome.scripting.executeScript = async (o) => { calls.push(o); return [{ frameId: 0, result: 'https://shop.example' }]; };
  assert.strictEqual(await B.fillPaymentFrames(7, { number: '1' }), 0);
  assert.strictEqual(calls.length, 1);
  delete globalThis.chrome;
});
