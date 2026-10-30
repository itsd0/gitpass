const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
// content.js + content/*.js in manifest order, evaluated as ONE script: a browser shares
// top-level let/const across content scripts, but jsdom w.eval calls do not share them.
const CONTENT_SRC = ['content.js', 'content/messaging.js', 'content/settings.js', 'content/detection.js', 'content/totp.js', 'content/dropdown.js', 'content/username.js', 'content/banner.js', 'content/command.js', 'content/prompts.js', 'content/wiring.js'].map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n');
const policy = require('../lib/policy.js');

test('cross-site detection logic', () => {
  // Logic matches policy.isCrossSite:
  // Same registrable domain and scheme -> not cross-site
  // Different registrable domain, scheme, or public suffix -> cross-site
  // Missing or empty url -> cross-site (not a native match)
  const isCrossSite = (origin, itemUrl) => policy.isCrossSite(itemUrl, origin);

  assert.strictEqual(isCrossSite('https://app.example.com', 'https://auth.example.com'), false);
  assert.strictEqual(isCrossSite('https://example.com', 'https://example.com/login'), false);
  assert.strictEqual(isCrossSite('http://localhost:3000', 'http://localhost:3000'), false);

  // Scheme mismatch (http vs https)
  assert.strictEqual(isCrossSite('https://example.com', 'http://example.com'), true);
  // Loopback port mismatch
  assert.strictEqual(isCrossSite('http://localhost:3000', 'http://localhost:8080'), true);
  // Public suffix hosts must match exact host
  assert.strictEqual(isCrossSite('https://foo.github.io', 'https://bar.github.io'), true);
  assert.strictEqual(isCrossSite('https://foo.github.io', 'https://foo.github.io'), false);

  assert.strictEqual(isCrossSite('https://example.com', 'https://github.com'), true);
  assert.strictEqual(isCrossSite('http://localhost:3000', 'https://github.com'), true);
  assert.strictEqual(isCrossSite('https://example.com', ''), true);
  assert.strictEqual(isCrossSite('https://example.com', null), true);
});

test('login search filtering and ranking', () => {
  const items = [
    { id: '1', title: 'Zebra', subtitle: 'user@zebra.com', url: 'https://zebra.com', favorite: false },
    { id: '2', title: 'Alpha', subtitle: 'user@alpha.com', url: 'https://alpha.com', favorite: false },
    { id: '3', title: 'Beta Fav', subtitle: 'user@beta.com', url: 'https://beta.com', favorite: true },
    { id: '4', title: 'GitHub Work', subtitle: 'alice_work', url: 'https://github.com', favorite: false },
  ];

  const filterAndRank = (list, query) => {
    const q = (query || '').trim().toLowerCase();
    const filtered = list.filter((i) => {
      if (!q) return true;
      return [i.title, i.subtitle, i.url].some((x) => x && String(x).toLowerCase().includes(q));
    });
    filtered.sort((a, b) => (b.favorite - a.favorite) || String(a.title).localeCompare(String(b.title)));
    return filtered;
  };

  // Ranking without query: favorites first, then alphabetical by title
  const rankedAll = filterAndRank(items, '');
  assert.deepStrictEqual(rankedAll.map((i) => i.title), ['Beta Fav', 'Alpha', 'GitHub Work', 'Zebra']);

  // Filtering by title query
  const gitMatches = filterAndRank(items, 'git');
  assert.deepStrictEqual(gitMatches.map((i) => i.id), ['4']);

  // Filtering by subtitle/username query
  const userMatches = filterAndRank(items, 'alice');
  assert.deepStrictEqual(userMatches.map((i) => i.id), ['4']);

  // Filtering by URL query
  const urlMatches = filterAndRank(items, 'zebra.com');
  assert.deepStrictEqual(urlMatches.map((i) => i.id), ['1']);

  // Empty match query
  const none = filterAndRank(items, 'nomatchhere');
  assert.strictEqual(none.length, 0);
});

test('content.js inline search and cross-site confirm integration', async () => {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  const sent = [];

  const roots = [];
  const attach = w.Element.prototype.attachShadow;
  w.Element.prototype.attachShadow = function (init) {
    const r = attach.call(this, init);
    roots.push(r);
    return r;
  };

  w.HTMLElement.prototype.getBoundingClientRect = function () {
    return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 };
  };
  w.HTMLElement.prototype.scrollIntoView = function () {};

  const sampleLogins = [
    { id: 'v1:1', category: 'login', title: 'Site Login', subtitle: 'me@site.example', url: 'https://site.example', favorite: false },
    { id: 'v1:2', category: 'login', title: 'GitHub', subtitle: 'alice', url: 'https://github.com', favorite: false },
    { id: 'v1:3', category: 'login', title: 'No URL Item', subtitle: 'nourl', url: '', favorite: false },
  ];

  let confirmApproved = false;
  w.chrome = {
    runtime: {
      onMessage: { addListener() {} },
      sendMessage: async (m) => {
        sent.push(m);
        if (m.type === 'page:pending') return { ok: true, result: { prompt: null } };
        if (m.type === 'page:match') return { ok: true, result: { origin: 'https://site.example', items: [sampleLogins[0]] } };
        if (m.type === 'page:search_enable') return { ok: true, result: { ok: true } };
        if (m.type === 'page:search_disable') return { ok: true, result: { ok: true } };
        if (m.type === 'page:search') return { ok: true, result: { origin: 'https://site.example', items: sampleLogins } };
        if (m.type === 'page:request_cross_site_confirm') {
          if (!confirmApproved) {
            const err = new Error('The user cancelled');
            err.code = 'cancelled';
            throw err;
          }
          return { ok: true, result: { approved: true } };
        }
        if (m.type === 'page:fill') return { ok: true, result: { origin: 'https://site.example', username: 'alice', password: 'secret-password' } };
        if (m.type === 'page:add_url') return { ok: true, result: { origin: 'https://site.example', ok: true } };
        return { ok: true, result: {} };
      },
    },
    i18n: { getMessage: (k) => k },
  };

  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }
  w.eval(CONTENT_SRC);
  await new Promise((r) => setTimeout(r, 50));

  function trust(e) {
    const sym = Object.getOwnPropertySymbols(e)[0];
    if (sym && e[sym]) {
      Object.defineProperty(e[sym], 'isTrusted', { get() { return true; }, set() {}, configurable: true });
    }
    return e;
  }

  // Open dropdown via badge click with trusted event
  const badgeBtn = roots[0].querySelector('button');
  assert.ok(badgeBtn, 'badge button found');
  badgeBtn.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 80));

  // Check dropdown appeared
  const menuRoot = roots.find((r) => r.querySelector('.menu'));
  assert.ok(menuRoot, 'dropdown menu root found');

  // Search gitpass option should exist in dropdown rows
  const buttons = Array.from(menuRoot.querySelectorAll('.it'));
  const searchRow = buttons.find((b) => b.textContent.includes('csSearchGitpass') || b.textContent.includes('Search gitpass'));
  assert.ok(searchRow, 'Search gitpass row found in menu');

  // Clicking Search gitpass enters search mode and renders search input
  searchRow.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 50));

  const input = menuRoot.querySelector('.search-input');
  assert.ok(input, 'search input rendered');

  // Search input typing filters items
  input.value = 'GitHub';
  input.dispatchEvent(new w.Event('input', { bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));

  const filteredButtons = Array.from(menuRoot.querySelectorAll('.it'));
  assert.strictEqual(filteredButtons.length, 1);
  assert.ok(filteredButtons[0].textContent.includes('GitHub'));

  // Clicking a cross-site result triggers request_cross_site_confirm.
  // With confirmApproved=false, the user cancelled; no fill occurs and search remains active.
  filteredButtons[0].dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 50));

  const emInputBefore = w.document.getElementById('em');
  const pwInputBefore = w.document.getElementById('pw');
  assert.strictEqual(emInputBefore.value, '', 'username field remains untouched on cancel');
  assert.strictEqual(pwInputBefore.value, '', 'password field remains untouched on cancel');
  assert.ok(menuRoot.querySelector('.search-input'), 'returned to search mode after cancel');

  // Assert no secrets exist in shadow DOM before fill
  const allShadowTextBefore = roots.map((r) => r.innerHTML).join(' ');
  assert.ok(!allShadowTextBefore.includes('secret-password'), 'no secrets in shadow DOM before fill');

  // Now user approves the cross-site confirmation window
  confirmApproved = true;
  const filteredAgain = Array.from(menuRoot.querySelectorAll('.it'));
  filteredAgain[0].dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 80));

  // Verify page:fill was sent
  const fillMsg = sent.find((m) => m.type === 'page:fill');
  assert.ok(fillMsg, 'page:fill message sent');
  assert.strictEqual(fillMsg.item_id, 'v1:2');

  // Verify fields were filled
  const emInput = w.document.getElementById('em');
  const pwInput = w.document.getElementById('pw');
  assert.strictEqual(emInput.value, 'alice');
  assert.strictEqual(pwInput.value, 'secret-password');

  // Verify post-fill prompt "Also add this site to the item" is shown
  const addBtn = Array.from(menuRoot.querySelectorAll('button')).find((b) => b.textContent.includes('Also add this site') || b.textContent.includes('csAlsoAddSite'));
  assert.ok(addBtn, 'add site prompt button shown');

  // Click "Also add this site"
  addBtn.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 80));

  const addUrlMsg = sent.find((m) => m.type === 'page:add_url');
  assert.ok(addUrlMsg, 'page:add_url message sent');
  assert.strictEqual(addUrlMsg.item_id, 'v1:2');
  assert.strictEqual(addUrlMsg.user_click, true);

  // Assert shadow DOM still does not contain secret password
  const allShadowTextAfter = roots.map((r) => r.innerHTML).join(' ');
  assert.ok(!allShadowTextAfter.includes('secret-password'), 'no secrets in shadow DOM after fill');

  w.close();
});

test('URL-less logins trigger cross-site confirmation', async () => {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  const sent = [];
  const roots = [];
  const attach = w.Element.prototype.attachShadow;
  w.Element.prototype.attachShadow = function (init) {
    const r = attach.call(this, init);
    roots.push(r);
    return r;
  };
  w.HTMLElement.prototype.getBoundingClientRect = function () {
    return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 };
  };
  w.HTMLElement.prototype.scrollIntoView = function () {};

  const sampleLogins = [
    { id: 'v1:3', category: 'login', title: 'No URL Item', subtitle: 'nourl', url: '', favorite: false },
  ];

  w.chrome = {
    runtime: {
      onMessage: { addListener() {} },
      sendMessage: async (m) => {
        sent.push(m);
        if (m.type === 'page:pending') return { ok: true, result: { prompt: null } };
        if (m.type === 'page:match') return { ok: true, result: { origin: 'https://site.example', items: [] } };
        if (m.type === 'page:search_enable') return { ok: true, result: { ok: true } };
        if (m.type === 'page:search_disable') return { ok: true, result: { ok: true } };
        if (m.type === 'page:search') return { ok: true, result: { origin: 'https://site.example', items: sampleLogins } };
        if (m.type === 'page:request_cross_site_confirm') return { ok: true, result: { approved: true } };
        if (m.type === 'page:fill') return { ok: true, result: { origin: 'https://site.example', username: 'alice', password: 'secret-password' } };
        return { ok: true, result: {} };
      },
    },
    i18n: { getMessage: (k) => k },
  };

  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }
  w.eval(CONTENT_SRC);
  await new Promise((r) => setTimeout(r, 50));

  function trust(e) {
    const sym = Object.getOwnPropertySymbols(e)[0];
    if (sym && e[sym]) {
      Object.defineProperty(e[sym], 'isTrusted', { get() { return true; }, set() {}, configurable: true });
    }
    return e;
  }

  const badgeBtn = roots[0].querySelector('button');
  badgeBtn.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 80));

  const menuRoot = roots.find((r) => r.querySelector('.menu'));
  const searchRow = Array.from(menuRoot.querySelectorAll('.it')).find((b) => b.textContent.includes('csSearchGitpass') || b.textContent.includes('Search gitpass'));
  searchRow.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 50));

  const noUrlBtn = Array.from(menuRoot.querySelectorAll('.it')).find((b) => b.textContent.includes('No URL Item'));
  assert.ok(noUrlBtn, 'No URL item button found');
  noUrlBtn.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 80));

  const confirmMsg = sent.find((m) => m.type === 'page:request_cross_site_confirm');
  assert.ok(confirmMsg, 'cross-site confirmation requested for URL-less item');
  assert.strictEqual(confirmMsg.item_id, 'v1:3');

  w.close();
});

test('cross-site TOTP fill sends page:totp after confirmation approval', async () => {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'multi-step.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://site.example/2fa', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  w.document.getElementById('step1').style.display = 'none';
  w.document.getElementById('step3').style.display = '';

  const sent = [];
  const roots = [];
  const attach = w.Element.prototype.attachShadow;
  w.Element.prototype.attachShadow = function (init) {
    const r = attach.call(this, init);
    roots.push(r);
    return r;
  };
  w.HTMLElement.prototype.getBoundingClientRect = function () {
    return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 };
  };
  w.HTMLElement.prototype.scrollIntoView = function () {};

  const sampleLogins = [
    { id: 'v1:no-totp', category: 'login', title: 'No TOTP Login', subtitle: 'user1', url: 'https://other.example', has_totp: false, favorite: false },
    { id: 'v1:totp-item', category: 'login', title: 'Other TOTP Login', subtitle: 'otheruser', url: 'https://other.example', has_totp: true, favorite: false },
  ];

  w.chrome = {
    runtime: {
      onMessage: { addListener() {} },
      sendMessage: async (m) => {
        sent.push(m);
        if (m.type === 'page:pending') return { ok: true, result: { prompt: null } };
        if (m.type === 'page:match') return { ok: true, result: { origin: 'https://site.example', items: [] } };
        if (m.type === 'page:search_enable') return { ok: true, result: { ok: true } };
        if (m.type === 'page:search_disable') return { ok: true, result: { ok: true } };
        if (m.type === 'page:search') return { ok: true, result: { origin: 'https://site.example', items: sampleLogins } };
        if (m.type === 'page:request_cross_site_confirm') return { ok: true, result: { approved: true } };
        if (m.type === 'page:totp') return { ok: true, result: { origin: 'https://site.example', code: '654321', remaining: 30 } };
        return { ok: true, result: {} };
      },
    },
    i18n: { getMessage: (k) => k },
  };

  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }
  w.eval(CONTENT_SRC);
  await new Promise((r) => setTimeout(r, 50));

  function trust(e) {
    const sym = Object.getOwnPropertySymbols(e)[0];
    if (sym && e[sym]) {
      Object.defineProperty(e[sym], 'isTrusted', { get() { return true; }, set() {}, configurable: true });
    }
    return e;
  }

  const badgeBtn = roots[0].querySelector('button');
  assert.ok(badgeBtn, 'OTP badge button found');
  badgeBtn.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 80));

  const menuRoot = roots.find((r) => r.querySelector('.menu'));
  const searchRow = Array.from(menuRoot.querySelectorAll('.it')).find((b) => b.textContent.includes('csSearchGitpass') || b.textContent.includes('Search gitpass'));
  searchRow.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 50));

  const filtered = Array.from(menuRoot.querySelectorAll('.it'));
  assert.ok(!filtered.some((b) => b.textContent.includes('No TOTP Login')), 'non-TOTP login filtered out on TOTP field');
  const totpItemBtn = filtered.find((b) => b.textContent.includes('Other TOTP Login'));
  assert.ok(totpItemBtn, 'Other TOTP Login row found');

  totpItemBtn.dispatchEvent(trust(new w.MouseEvent('click', { bubbles: true })));
  await new Promise((r) => setTimeout(r, 80));

  const confirmMsg = sent.find((m) => m.type === 'page:request_cross_site_confirm');
  assert.ok(confirmMsg, 'cross-site confirmation requested for TOTP item');
  assert.strictEqual(confirmMsg.item_id, 'v1:totp-item');

  const totpMsg = sent.find((m) => m.type === 'page:totp');
  assert.ok(totpMsg, 'page:totp sent for confirmed cross-site TOTP item');
  assert.strictEqual(totpMsg.item_id, 'v1:totp-item');

  const otpInput = w.document.getElementById('otp');
  assert.strictEqual(otpInput.value, '654321', 'TOTP field filled with code from cross-site login');

  w.close();
});
