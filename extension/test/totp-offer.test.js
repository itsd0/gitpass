// One-time code offered on the page after a login fill (#181): background memory (remember / expire /
// domain match), the chip (Tab / Enter / Esc), and content.js showing it for a code field only.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
globalThis.GitpassPolicy = require('../lib/policy.js');
const B = require('../bg-page.js');
const Chip = require('../lib/totp-chip.js');

const tab = (url, id = 1) => ({ tab: { id, url }, frameId: 0, url });
const withTotp = { id: 'v:1', category: 'login', title: 'GitHub', subtitle: 'me', has_totp: true };
const noTotp = { id: 'v:2', category: 'login', title: 'Plain', subtitle: 'me', has_totp: false };
function fakeHost(items) {
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'match') return { items };
    if (cmd === 'fill') return { username: 'me', password: 'pw' };
    if (cmd === 'totp') return { code: '123456', remaining: 9 };
    return {};
  };
  h.calls = calls; return h;
}
async function withClock(startMs, fn) {
  const real = Date.now; let now = startMs; Date.now = () => now;
  try { await fn((ms) => { now += ms; }); } finally { Date.now = real; }
}

test('offer: filled login with a code is offered on the next page of the same site, no secrets', async () => {
  const h = fakeHost([withTotp]);
  await B.handle({ type: 'page:fill', item_id: 'v:1' }, tab('https://github.com/login', 31), h);
  const r = await B.handle({ type: 'page:totp_offer' }, tab('https://github.com/sessions/two-factor', 31), h);
  assert.strictEqual(r.origin, 'https://github.com');
  assert.deepStrictEqual({ ...r.offer, expires_in_ms: undefined }, { item_id: 'v:1', title: 'GitHub', expires_in_ms: undefined });
  assert.ok(r.offer.expires_in_ms > 0 && r.offer.expires_in_ms <= B.OFFER_TTL_MS);
  assert.ok(!JSON.stringify(r).includes('123456') && !JSON.stringify(r).includes('pw'));
  assert.ok(!h.calls.some((c) => c[0] === 'totp'), 'asking for an offer never fetches the code');
});

test('offer: a sibling subdomain of the same registrable domain still matches (Google-style flows)', async () => {
  const h = fakeHost([withTotp]);
  await B.handle({ type: 'page:fill', item_id: 'v:1' }, tab('https://accounts.example.com/signin', 32), h);
  const r = await B.handle({ type: 'page:totp_offer' }, tab('https://myaccount.example.com/challenge', 32), h);
  assert.strictEqual(r.offer && r.offer.item_id, 'v:1');
});

test('offer: nothing for a different registrable domain, another tab, or after the 2 minute window', async () => {
  await withClock(1_000_000, async (advance) => {
    const h = fakeHost([withTotp]);
    await B.handle({ type: 'page:fill', item_id: 'v:1' }, tab('https://github.com/login', 33), h);
    assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://evil.example/2fa', 33), h)).offer, null, 'cross-site');
    assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://github.com/2fa', 34), h)).offer, null, 'other tab');
    advance(B.OFFER_TTL_MS - 1000);
    assert.ok((await B.handle({ type: 'page:totp_offer' }, tab('https://github.com/2fa', 33), h)).offer, 'still inside the window');
    advance(2000);
    assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://github.com/2fa', 33), h)).offer, null, 'expired');
    assert.strictEqual(B._lastFilled.has(33), false, 'expired entry is dropped');
  });
});

test('offer: only logins that have a code and still match this origin', async () => {
  const h = fakeHost([noTotp]);
  await B.handle({ type: 'page:fill', item_id: 'v:2' }, tab('https://site.example/login', 35), h);
  assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://site.example/2fa', 35), h)).offer, null);
  const h2 = fakeHost([]); // the item no longer matches this page
  await B.handle({ type: 'page:fill', item_id: 'v:1' }, tab('https://site.example/login', 36), fakeHost([withTotp]));
  assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://site.example/2fa', 36), h2)).offer, null);
  assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://site.example/2fa', 99), fakeHost([withTotp]))).offer, null, 'nothing was filled in this tab');
});

test('offer: popup/command fills remember the item too, using the code clears it, closing the tab forgets it', async () => {
  const h = fakeHost([withTotp]);
  B.rememberFilled(37, 'https://github.com', 'v:1');
  assert.ok((await B.handle({ type: 'page:totp_offer' }, tab('https://github.com/2fa', 37), h)).offer);
  await B.handle({ type: 'page:totp', item_id: 'v:1' }, tab('https://github.com/2fa', 37), h);
  assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://github.com/2fa', 37), h)).offer, null, 'used');
  B.rememberFilled(38, 'https://github.com', 'v:1'); B.forgetTab(38);
  assert.strictEqual((await B.handle({ type: 'page:totp_offer' }, tab('https://github.com/2fa', 38), h)).offer, null, 'forgotten');
  B.rememberFilled('x', 'https://github.com', 'v:1'); B.rememberFilled(39, 'https://github.com', 5);
  assert.strictEqual(B._lastFilled.has(39), false);
});

function chipDom(extra = {}) {
  const dom = new JSDOM('<!doctype html><body></body>', { pretendToBeVisual: true });
  const log = [];
  const ui = Chip.build(dom.window.document, { title: 'GitHub', label: 'Fill one-time code', dismissLabel: 'Dismiss', logo: 'data:,', ttlMs: 60000,
    onFill: () => log.push('fill'), onDismiss: () => log.push('dismiss'), trusted: () => true, ...extra });
  dom.window.document.body.append(ui.el);
  return { dom, ui, log };
}
const key = (w, k, o = {}) => new w.KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...o });

test('chip: shows the label and item title, both buttons are tabbable and labelled', () => {
  const { ui } = chipDom();
  assert.strictEqual(ui.el.querySelector('.ti').textContent, 'Fill one-time code');
  assert.strictEqual(ui.el.querySelector('.u').textContent, 'GitHub');
  assert.strictEqual(ui.el.getAttribute('aria-label'), 'Fill one-time code');
  assert.strictEqual(ui.dismissBtn.getAttribute('aria-label'), 'Dismiss');
  for (const b of ui.el.querySelectorAll('button')) assert.ok(b.tabIndex >= 0 && !b.disabled);
  assert.ok(ui.el.querySelector('.tc-ring .tc-arc'), 'countdown ring');
  ui.close();
});

test('chip: Enter and click fill once, Esc and the x dismiss, other keys do nothing', () => {
  let c = chipDom(); const w = c.dom.window;
  c.ui.fill.dispatchEvent(key(w, 'a')); c.ui.fill.dispatchEvent(key(w, 'Tab'));
  assert.deepStrictEqual(c.log, []);
  c.ui.fill.dispatchEvent(key(w, 'Enter')); c.ui.fill.dispatchEvent(key(w, 'Enter')); c.ui.fill.click();
  assert.deepStrictEqual(c.log, ['fill'], 'fills exactly once');
  c = chipDom();
  c.ui.fill.dispatchEvent(key(c.dom.window, 'Escape'));
  assert.deepStrictEqual(c.log, ['dismiss']);
  c = chipDom(); c.ui.fill.click();
  assert.deepStrictEqual(c.log, ['fill']);
  c = chipDom(); c.ui.dismissBtn.click();
  assert.deepStrictEqual(c.log, ['dismiss']);
});

test('chip: untrusted (page-dispatched) events never fill, and the offer window ends by itself', async () => {
  let c = chipDom({ trusted: (e) => e.isTrusted });
  c.ui.fill.click(); c.ui.fill.dispatchEvent(key(c.dom.window, 'Enter')); c.ui.fill.dispatchEvent(key(c.dom.window, 'Escape'));
  assert.deepStrictEqual(c.log, []);
  c.ui.close();
  c = chipDom({ ttlMs: 20 });
  await new Promise((r) => setTimeout(r, 60));
  assert.deepStrictEqual(c.log, ['dismiss']);
  c = chipDom({ ttlMs: 20 }); c.ui.close();
  await new Promise((r) => setTimeout(r, 60));
  assert.deepStrictEqual(c.log, [], 'close() stops the timer');
});

function pageWith(html, offer, url = 'https://github.com/sessions/two-factor') {
  const dom = new JSDOM(html, { url, runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window, sent = [];
  w.HTMLElement.prototype.getBoundingClientRect = function () { return { left: 10, top: 10, right: 210, bottom: 40, width: 200, height: 30 }; };
  w.chrome = {
    runtime: { onMessage: { addListener() {} }, sendMessage: async (m) => {
      sent.push(m.type);
      if (m.type === 'page:pending') return { ok: true, result: { prompt: null } };
      if (m.type === 'page:totp_offer') return { ok: true, result: { origin: new URL(url).origin, offer } };
      return { ok: true, result: { origin: new URL(url).origin, items: [] } };
    } },
    storage: { local: { get() {}, onChanged: { addListener() {} } } },
    i18n: { getMessage: () => '' },
  };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'lib/banner.js', 'lib/totp-chip.js', 'content.js']) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { w, sent };
}
const OTP_PAGE = '<!doctype html><body><form action="/2fa"><input id="otp" name="otp" autocomplete="one-time-code" inputmode="numeric" maxlength="6"><button>Verify</button></form></body>';
const LOGIN_PAGE = '<!doctype html><body><form action="/s"><input id="u" name="login" autocomplete="username"><input id="p" type="password" autocomplete="current-password"></form></body>';

test('content: a code field gets the chip with the item title when an offer exists', async () => {
  const { w, sent } = pageWith(OTP_PAGE, { item_id: 'v:1', title: 'GitHub', expires_in_ms: 90000 });
  await new Promise((r) => setTimeout(r, 60));
  assert.ok(sent.includes('page:totp_offer'));
  const hosts = [...w.document.querySelectorAll('gitpass-ui')];
  assert.ok(hosts.length >= 2, 'badge + chip, got ' + hosts.length);
  assert.ok(hosts.some((h) => h.style.display !== 'none' && h.style.top === '44px'), 'chip sits under the field');
  assert.ok(!sent.includes('page:totp'), 'no code is fetched until the chip is used');
  w.close();
});

test('content: no chip without an offer (e.g. a different site), and none on a login-only page', async () => {
  let r = pageWith(OTP_PAGE, null, 'https://other.example/2fa');
  await new Promise((x) => setTimeout(x, 60));
  assert.ok(r.sent.includes('page:totp_offer'));
  assert.strictEqual(r.w.document.querySelectorAll('gitpass-ui').length, 1, 'only the badge');
  r.w.close();
  r = pageWith(LOGIN_PAGE, { item_id: 'v:1', title: 'GitHub', expires_in_ms: 90000 });
  await new Promise((x) => setTimeout(x, 60));
  assert.ok(!r.sent.includes('page:totp_offer'), 'a login form never asks for an offer');
  r.w.close();
});
