// Save-card banner (#196): card helpers, card capture, background card routing,
// and the rendered card banner. Fake data only (4242 test numbers); no real vault.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');

const Banner = require('../lib/banner.js');
const F = require('../lib/forms.js');
const O = { ignoreLayout: true }; // jsdom has no layout

// ---------- pure card helpers ----------
test('card brand, masked number and default title', () => {
  assert.strictEqual(Banner.cardBrand('4242424242424242'), 'Visa');
  assert.strictEqual(Banner.cardBrand('5555555555554444'), 'Mastercard');
  assert.strictEqual(Banner.cardBrand('378282246310005'), 'Amex');
  assert.strictEqual(Banner.cardBrand('6011111111111117'), 'Discover');
  assert.strictEqual(Banner.cardBrand('1234567890123456'), '');
  assert.strictEqual(Banner.cardMasked('4242 4242 4242 4242'), '•••• 4242');
  assert.strictEqual(Banner.cardTitleDefault('4242424242424242'), 'Visa •••• 4242');
  assert.strictEqual(Banner.cardTitleDefault('1234567890123456'), '•••• 3456');
  assert.strictEqual(Banner.cardLast4('378282246310005'), '0005');
});

test('card Luhn: valid test numbers pass, mistyped numbers fail', () => {
  assert.strictEqual(Banner.cardLuhn('4242424242424242'), true);
  assert.strictEqual(Banner.cardLuhn('5555555555554444'), true);
  assert.strictEqual(Banner.cardLuhn('378282246310005'), true);
  assert.strictEqual(Banner.cardLuhn('4242424242424243'), false);
  assert.strictEqual(Banner.cardLuhn('123'), false);
  assert.strictEqual(Banner.cardLuhn('4242 4242 4242 4242'), false, 'spaces must be stripped first');
  assert.strictEqual(F.luhnCard('4242424242424242'), true);
  assert.strictEqual(F.luhnCard('4242424242424243'), false);
});

test('expiry normalisation: combined and split fields -> MM/YY', () => {
  assert.strictEqual(Banner.normalizeCardExpiry('08/29'), '08/29');
  assert.strictEqual(Banner.normalizeCardExpiry('8/2029'), '08/29');
  assert.strictEqual(Banner.normalizeCardExpiry('0829'), '08/29');
  assert.strictEqual(Banner.normalizeCardExpiry('08-2029'), '08/29');
  assert.strictEqual(Banner.normalizeCardExpiry('13/29'), '');
  assert.strictEqual(Banner.normalizeCardExpiry(''), '');
  assert.strictEqual(Banner.normalizeCardExpiryParts('8', '2029'), '08/29');
  assert.strictEqual(Banner.normalizeCardExpiryParts('August', '29'), '08/29');
  assert.strictEqual(Banner.normalizeCardExpiryParts('', '29'), '');
  assert.strictEqual(F.normCardExpiry('03 / 29'), '03/29');
});

test('sanitizeCardCapture keeps only number/name/expiry and drops the CVV', () => {
  assert.deepStrictEqual(
    Banner.sanitizeCardCapture({ number: '4242 4242 4242 4242', name: ' Nay Lin ', expiry: '8/2029', cvv: '123', cvc: '123', pin: '0000', extra: 'x' }),
    { number: '4242424242424242', name: 'Nay Lin', expiry: '08/29' });
  assert.strictEqual(Banner.sanitizeCardCapture({ number: '4242424242424243', name: 'X', expiry: '08/29' }), null, 'failing Luhn rejected');
  assert.deepStrictEqual(Banner.sanitizeCardCapture({ number: '4242424242424242', cvv: '123' }),
    { number: '4242424242424242', name: '', expiry: '' }, 'valid number without expiry still sanitises, CVV dropped');
});

// ---------- content-side capture ----------
function page(name) {
  const html = fs.readFileSync(path.join(__dirname, 'pages', name), 'utf8');
  return new JSDOM(html, { url: 'https://example.com/' }).window.document;
}
const byScope = (forms, doc, id) => forms.find((f) => (id ? f.scope.id === id : f.scope === doc));

test('card capture: number, name and expiry only — the CVV never leaves the page', () => {
  const doc = page('card.html');
  const forms = F.detectForms(doc, O);
  const p = byScope(forms, doc, 'pay');
  assert.strictEqual(p.kind, 'card');
  F.fillCard(p, { number: '4111 1111 1111 1111', cardholder: 'Nay Lin', expiry: '07/28', cvv: '123' });
  const c = F.captureCredentials(p);
  assert.deepStrictEqual(c, { kind: 'card', number: '4111111111111111', name: 'Nay Lin', expiry: '07/28' });
  assert.ok(!('cvv' in c) && !('cvc' in c) && !('csc' in c), 'no CVV key on the capture');
});

test('card capture: combined expiry field, and failing Luhn captures nothing', () => {
  const doc = page('card.html');
  const forms = F.detectForms(doc, O);
  const p2 = byScope(forms, doc, 'pay2');
  F.fillCard(p2, { card_number: '4242424242424242', exp_month: 3, exp_year: 2029 });
  assert.deepStrictEqual(F.captureCredentials(p2), { kind: 'card', number: '4242424242424242', name: '', expiry: '03/29' });
  const doc2 = page('card.html');
  const q = byScope(F.detectForms(doc2, O), doc2, 'pay');
  F.fillCard(q, { number: '4111 1111 1111 1112', cardholder: 'Nay Lin', expiry: '07/28', cvv: '123' });
  assert.strictEqual(F.captureCredentials(q), null, 'mistyped number: no capture message at all');
});

// ---------- background routing ----------
globalThis.chrome = { runtime: { id: 'mock', getURL: (p) => 'chrome-extension://mock/' + p } };
globalThis.GitpassPolicy = require('../lib/policy.js');
globalThis.GitpassBanner = Banner;
const B = require('../bg-page.js');

const tab = (url, id = 1) => ({ tab: { id, url }, frameId: 0, url });
const VAULTS = [{ id: 'v1', name: 'Personal' }];
function fakeHost(opts = {}) {
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'list_vaults') return { vaults: VAULTS };
    if (cmd === 'list_items') {
      const items = (opts.cards || []).filter((i) => !args.category || i.category === args.category);
      return { items };
    }
    if (cmd === 'get_item') return { item: { fields: (opts.expiryById || {})[args.item_id] || [] } };
    if (cmd === 'create_item') return { item_id: 'v1:newcard', title: args.item.title };
    return {};
  };
  h.calls = calls; return h;
}
const cardItem = (id, subtitle) => ({ id, category: 'credit_card', title: 'Card', subtitle });

test('page:capture card: prompt carries masked number and brand, never the full number', async () => {
  const h = fakeHost();
  const r = await B.handle({ type: 'page:capture', kind: 'card', number: '4242 4242 4242 4242', name: 'Nay Lin', expiry: '08/29', cvv: '123' }, tab('https://shop.example/checkout', 11), h);
  assert.strictEqual(r.prompt, 'save_card');
  assert.strictEqual(r.masked, '•••• 4242');
  assert.strictEqual(r.brand, 'Visa');
  assert.strictEqual(r.title, 'Visa •••• 4242');
  assert.deepStrictEqual(r.vaults, VAULTS);
  const stored = B._pendingSaves.get(11);
  assert.strictEqual(stored.number, '4242424242424242');
  assert.ok(!('cvv' in stored) && !('cvc' in stored), 'pending save holds no CVV');
});

test('page:capture card: invalid Luhn rejected; duplicate last4+expiry suppressed', async () => {
  const h = fakeHost();
  await assert.rejects(B.handle({ type: 'page:capture', kind: 'card', number: '4242424242424243', name: 'X', expiry: '08/29' }, tab('https://shop.example/checkout', 12), h), /card/);
  const dup = fakeHost({ cards: [cardItem('v:c1', '•••• 4242')], expiryById: { 'v:c1': [{ name: 'expiry', value: '08/29' }] } });
  const r = await B.handle({ type: 'page:capture', kind: 'card', number: '4242424242424242', name: 'Nay Lin', expiry: '08/29' }, tab('https://shop.example/checkout', 13), dup);
  assert.strictEqual(r.prompt, null, 'same last4 + expiry already saved: no prompt');
  assert.strictEqual(B._pendingSaves.get(13), undefined);
  const other = fakeHost({ cards: [cardItem('v:c1', '•••• 4242')], expiryById: { 'v:c1': [{ name: 'expiry', value: '09/30' }] } });
  const r2 = await B.handle({ type: 'page:capture', kind: 'card', number: '4242424242424242', name: 'Nay Lin', expiry: '08/29' }, tab('https://shop.example/checkout', 14), other);
  assert.strictEqual(r2.prompt, 'save_card', 'same last4 but different expiry: prompt shown');
});

test('page:save_decision card: create_item carries number/name/expiry only, no CVV', async () => {
  const h = fakeHost();
  await B.handle({ type: 'page:capture', kind: 'card', number: '5555555555554444', name: 'Nay Lin', expiry: '07/28', cvv: '999' }, tab('https://shop.example/checkout', 15), h);
  const r = await B.handle({ type: 'page:save_decision', action: 'save', title: 'Mastercard •••• 4444', user_click: true, vault_id: 'v1' }, tab('https://shop.example/checkout', 15), h);
  assert.strictEqual(r.done, true);
  const [, args] = h.calls.find(([c]) => c === 'create_item');
  assert.strictEqual(args.item.category, 'credit_card');
  assert.strictEqual(args.item.title, 'Mastercard •••• 4444');
  assert.strictEqual(args.item.url, 'https://shop.example');
  assert.deepStrictEqual(Object.fromEntries(args.item.sections[0].fields.map((f) => [f.id, f.value])),
    { number: '5555555555554444', cardholder: 'Nay Lin', expiry: '07/28' });
  assert.ok(!args.item.sections[0].fields.some((f) => /cvv|cvc|csc/i.test(f.id)), 'no CVV field sent to the host');
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'update', user_click: true }, tab('https://shop.example/checkout', 15), h), /nothing to save/);
});

test('page:capture card: never-save site shows no prompt', async () => {
  const h = fakeHost();
  B._neverSaveFallback.push('shop.example');
  try {
    const r = await B.handle({ type: 'page:capture', kind: 'card', number: '4242424242424242', name: '', expiry: '' }, tab('https://shop.example/checkout', 16), h);
    assert.strictEqual(r.prompt, null);
  } finally {
    B._neverSaveFallback.length = 0;
  }
});

// ---------- rendered card banner ----------
function bootContent(pending) {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://www.site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  const roots = [];
  const attach = w.Element.prototype.attachShadow;
  w.Element.prototype.attachShadow = function (init) { const r = attach.call(this, init); roots.push(r); return r; };
  w.HTMLElement.prototype.getBoundingClientRect = function () { return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 }; };
  w.chrome = {
    runtime: {
      onMessage: { addListener() {} },
      sendMessage: async (m) => ({ ok: true, result: m.type === 'page:pending' ? pending : { origin: 'https://www.site.example', items: [] } }),
    },
    i18n: { getMessage: () => '' },
  };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'lib/banner.js', 'content.js']) {
    w.eval(fs.readFileSync(path.join(__dirname, '..', f), 'utf8'));
  }
  return { dom, w, roots };
}

test('card banner: brand + masked number heading, title prefilled, full number never rendered', async () => {
  const { dom, roots } = bootContent({ prompt: 'save_card', masked: '•••• 4242', brand: 'Visa', title: 'Visa •••• 4242', vaults: [] });
  await new Promise((r) => setTimeout(r, 50));
  const b = roots.find((r) => r.querySelector('.banner'));
  assert.ok(b, 'banner shadow root exists');
  assert.strictEqual(b.querySelector('.banner .ti').textContent, 'Save card to gitpass?');
  assert.match(b.querySelector('.banner .u').textContent, /Visa · •••• 4242/);
  assert.strictEqual(b.querySelector('.banner input.inp').value, 'Visa •••• 4242');
  assert.ok(!b.querySelector('.banner').textContent.includes('4242424242424242'), 'full number never appears in the banner');
  assert.deepStrictEqual([...b.querySelectorAll('.banner .acts button')].map((x) => x.textContent), ['Never for this site', 'Not now', 'Save']);
  dom.window.close();
});
