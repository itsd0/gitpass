// Toolbar icon state and match-count badge (issue #218): pure state mapping, controller behaviour with
// fake chrome.action/tabs/host, privacy of what reaches the badge, the options toggle and wiring.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

globalThis.GitpassPolicy = require('../lib/policy.js');
const B = require('../bg-badge.js');
const root = path.join(__dirname, '..');
const read = (f) => fs.readFileSync(path.join(root, f), 'utf8');

test('badgeText: empty for none, 1-9, then 9+', () => {
  assert.strictEqual(B.badgeText(0), '');
  assert.strictEqual(B.badgeText(-3), '');
  assert.strictEqual(B.badgeText(NaN), '');
  assert.strictEqual(B.badgeText(1), '1');
  assert.strictEqual(B.badgeText(9), '9');
  assert.strictEqual(B.badgeText(10), '9+');
  assert.strictEqual(B.badgeText(250), '9+');
});

test('compute: locked, app not running, unlocked with and without matches', () => {
  assert.deepStrictEqual(B.compute({ status: 'locked', count: 4, fillable: true, enabled: true }), { text: '', color: B.BADGE_COLOR, title: 'gitpass is locked', iconVariant: 'muted' });
  assert.deepStrictEqual(B.compute({ status: 'offline' }), { text: '', color: B.BADGE_COLOR, title: 'gitpass app not running', iconVariant: 'muted' });
  assert.deepStrictEqual(B.compute(undefined), { text: '', color: B.BADGE_COLOR, title: 'gitpass app not running', iconVariant: 'muted' });
  assert.deepStrictEqual(B.compute({ status: 'unlocked', fillable: true, enabled: true, count: 0 }), { text: '', color: B.BADGE_COLOR, title: 'gitpass', iconVariant: 'normal' });
  assert.deepStrictEqual(B.compute({ status: 'unlocked', fillable: true, enabled: true, count: 2 }), { text: '2', color: B.BADGE_COLOR, title: 'gitpass', iconVariant: 'normal' });
  assert.strictEqual(B.compute({ status: 'unlocked', fillable: true, enabled: true, count: 12 }).text, '9+');
  // not fillable origin or toggle off -> no badge, but still the normal unlocked icon
  assert.strictEqual(B.compute({ status: 'unlocked', fillable: false, enabled: true, count: 3 }).text, '');
  assert.strictEqual(B.compute({ status: 'unlocked', fillable: true, enabled: false, count: 3 }).text, '');
  assert.strictEqual(B.compute({ status: 'unlocked', fillable: true, enabled: false, count: 3 }).iconVariant, 'normal');
});

test('compute: tooltips come from the localized resolver', () => {
  const t = (k, d) => `[${k}] ${d}`;
  assert.strictEqual(B.compute({ status: 'locked' }, t).title, '[statusAppLocked] gitpass is locked');
  assert.strictEqual(B.compute({ status: 'offline' }, t).title, '[badgeAppNotRunning] gitpass app not running');
});

test('statusOf and loginCount map host answers', () => {
  assert.strictEqual(B.statusOf({ unlocked: true }), 'unlocked');
  assert.strictEqual(B.statusOf({ unlocked: false }), 'locked');
  assert.strictEqual(B.statusOf(null, new Error('native host disconnected')), 'offline');
  assert.strictEqual(B.loginCount({ items: [{ id: 'a', category: 'login' }, { id: 'b' }, { id: 'c', category: 'card' }] }), 2);
  assert.strictEqual(B.loginCount({}), 0);
  assert.strictEqual(B.loginCount(null), 0);
});

test('badge colour: white text on gitpass orange is readable (>= 4.5:1)', () => {
  const lum = (rgb) => { const c = rgb.map((v) => v / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)); return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; };
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [hi, lo] = [lum(hex(B.BADGE_COLOR)), lum(hex(B.BADGE_TEXT_COLOR))].sort((a, b) => b - a);
  assert.ok((hi + 0.05) / (lo + 0.05) >= 4.5);
  // same orange as the popup's light-theme accent token (194 65 12)
  assert.deepStrictEqual(hex(B.BADGE_COLOR), [194, 65, 12]);
});

// ---- controller with fakes ----
function fakes({ tabs, status = { unlocked: true }, matches = {}, enabled = true, hostFails = false } = {}) {
  const calls = [];
  const hostCalls = [];
  const record = (name) => (args) => { calls.push({ name, ...args }); return Promise.resolve(); };
  const action = {
    setBadgeText: record('text'), setBadgeBackgroundColor: record('color'), setBadgeTextColor: record('textColor'),
    setTitle: record('title'), setIcon: record('icon'),
  };
  const timers = [];
  const ctl = B.createController({
    host: async (cmd, args) => {
      hostCalls.push({ cmd, args });
      if (hostFails) throw Object.assign(new Error('app_not_running'), { code: 'app_not_running' });
      if (cmd === 'status') return status;
      if (cmd === 'match') return { items: matches[args.origin] || [] };
      throw new Error('unexpected ' + cmd);
    },
    action,
    tabs: {
      get: async (id) => { const t = tabs.find((x) => x.id === id); if (!t) throw new Error('no tab'); return t; },
      query: async (q) => (q && q.active ? tabs.filter((t) => t.active) : tabs),
    },
    getEnabled: async () => enabled,
    t: (_k, d) => d,
    setTimeout: (fn, ms) => { timers.push({ fn, ms }); return timers.length; },
    clearTimeout: (id) => { if (timers[id - 1]) timers[id - 1].fn = null; },
  });
  const flush = async () => { await new Promise((r) => setImmediate(r)); for (const t of timers.splice(0)) if (t.fn) { t.fn(); } await new Promise((r) => setImmediate(r)); await new Promise((r) => setImmediate(r)); };
  return { ctl, calls, hostCalls, flush, timers };
}
const login = (id) => ({ id, category: 'login', title: 'SECRET-TITLE-' + id, username: 'user-' + id + '@example.test' });
const forTab = (calls, id, name) => calls.filter((c) => c.tabId === id && c.name === name);

test('update: unlocked tab with matches gets a per-tab count badge', async () => {
  const f = fakes({ tabs: [{ id: 7, active: true, url: 'https://example.test/login' }], matches: { 'https://example.test': [login('a'), login('b')] } });
  const view = await f.ctl.update(7);
  assert.strictEqual(view.text, '2');
  assert.strictEqual(forTab(f.calls, 7, 'text')[0].text, '2');
  assert.strictEqual(forTab(f.calls, 7, 'color')[0].color, B.BADGE_COLOR);
  assert.strictEqual(forTab(f.calls, 7, 'icon')[0].path[128], B.ICON_NORMAL);
  assert.deepStrictEqual(f.hostCalls.map((c) => c.cmd), ['status', 'match']);
  assert.deepStrictEqual(f.hostCalls[1].args, { origin: 'https://example.test' });
});

test('update: privacy - titles, usernames and secrets never reach the badge or tooltip', async () => {
  const f = fakes({ tabs: [{ id: 1, active: true, url: 'https://example.test/' }], matches: { 'https://example.test': [login('x'), login('y'), login('z')] } });
  await f.ctl.update(1);
  const shown = JSON.stringify(f.calls);
  assert.ok(!/SECRET-TITLE|user-[xyz]|example\.test|password/i.test(shown), shown);
  assert.strictEqual(forTab(f.calls, 1, 'title')[0].title, 'gitpass');
});

test('update: no matches, non-fillable origin, and toggle off show no badge and skip the host match call', async () => {
  const none = fakes({ tabs: [{ id: 1, active: true, url: 'https://none.test/' }] });
  assert.strictEqual((await none.ctl.update(1)).text, '');
  const plain = fakes({ tabs: [{ id: 2, active: true, url: 'http://insecure.test/' }], matches: { 'http://insecure.test': [login('a')] } });
  assert.strictEqual((await plain.ctl.update(2)).text, '');
  assert.ok(!plain.hostCalls.some((c) => c.cmd === 'match'));
  const chromePage = fakes({ tabs: [{ id: 3, active: true, url: 'chrome://extensions' }] });
  assert.strictEqual((await chromePage.ctl.update(3)).text, '');
  assert.ok(!chromePage.hostCalls.some((c) => c.cmd === 'match'));
  const off = fakes({ tabs: [{ id: 4, active: true, url: 'https://example.test/' }], matches: { 'https://example.test': [login('a')] }, enabled: false });
  assert.strictEqual((await off.ctl.update(4)).text, '');
  assert.ok(!off.hostCalls.some((c) => c.cmd === 'match'));
});

test('update: locked vault and missing app use the muted icon with no count', async () => {
  const locked = fakes({ tabs: [{ id: 1, active: true, url: 'https://example.test/' }], status: { unlocked: false } });
  const v = await locked.ctl.update(1);
  assert.deepStrictEqual([v.text, v.title, v.iconVariant], ['', 'gitpass is locked', 'muted']);
  assert.strictEqual(forTab(locked.calls, 1, 'icon')[0].path[128], B.ICON_MUTED);
  assert.ok(!locked.hostCalls.some((c) => c.cmd === 'match'));
  const gone = fakes({ tabs: [{ id: 1, active: true, url: 'https://example.test/' }], hostFails: true });
  const g = await gone.ctl.update(1);
  assert.deepStrictEqual([g.text, g.title, g.iconVariant], ['', 'gitpass app not running', 'muted']);
});

test('schedule: rapid tab switching is debounced into one host round trip per tab', async () => {
  const f = fakes({ tabs: [{ id: 1, active: true, url: 'https://example.test/' }], matches: { 'https://example.test': [login('a')] } });
  for (let i = 0; i < 10; i++) f.ctl.schedule(1);
  await f.flush();
  assert.strictEqual(f.hostCalls.filter((c) => c.cmd === 'status').length, 1);
  assert.strictEqual(f.hostCalls.filter((c) => c.cmd === 'match').length, 1);
  assert.ok(f.timers.length === 0);
});

test('lock event clears the badge on every tab and mutes the icon; unlock repaints the active tab', async () => {
  const tabs = [{ id: 1, active: true, url: 'https://example.test/' }, { id: 2, active: false, url: 'https://other.test/' }];
  const f = fakes({ tabs, matches: { 'https://example.test': [login('a')] } });
  await f.ctl.update(1);
  f.calls.length = 0;
  f.ctl.onHostEvent('lock');
  await f.flush();
  for (const id of [1, 2]) {
    assert.strictEqual(forTab(f.calls, id, 'text').pop().text, '', `tab ${id} cleared`);
    assert.strictEqual(forTab(f.calls, id, 'icon').pop().path[128], B.ICON_MUTED);
  }
  assert.ok(f.calls.some((c) => c.name === 'title' && c.tabId === undefined && c.title === 'gitpass is locked'), 'global state is the locked tooltip too');
  f.calls.length = 0;
  f.ctl.onHostEvent('unlock');
  await f.flush();
  assert.strictEqual(forTab(f.calls, 1, 'text').pop().text, '1');
  f.calls.length = 0;
  f.ctl.onHostEvent('changed');
  await f.flush();
  assert.strictEqual(forTab(f.calls, 1, 'text').pop().text, '1');
});

test('update: a closed tab is ignored without throwing', async () => {
  const f = fakes({ tabs: [] });
  assert.strictEqual(await f.ctl.update(99), null);
  assert.strictEqual(await f.ctl.update(undefined), null);
  assert.strictEqual(f.calls.length, 0);
});

// ---- options toggle, locales and wiring ----
test('options page has a keyboard-accessible "Show match count" switch, on by default', () => {
  const dom = new JSDOM(read('options/options.html'));
  const box = dom.window.document.getElementById('showBadge');
  assert.ok(box && box.type === 'checkbox');
  const label = dom.window.document.querySelector('label[for="showBadge"]');
  assert.strictEqual(label.getAttribute('data-i18n'), 'optBadge');
  assert.match(read('lib/onboard-common.js'), /showBadge:\s*true/);
});

test('en/my/th define the badge strings', () => {
  for (const loc of ['en', 'my', 'th']) {
    const m = JSON.parse(read(`_locales/${loc}/messages.json`));
    for (const k of ['optBadge', 'optBadgeBody', 'badgeAppNotRunning', 'statusAppLocked']) assert.ok(m[k] && m[k].message, `${loc}.${k}`);
  }
});

test('badge script is wired for Chrome (importScripts), Firefox (background.scripts) and the Firefox build', () => {
  assert.match(read('background.js'), /importScripts\('bg-badge\.js'\)/);
  assert.match(read('background.js'), /GitpassBadge\.install\(\{ host \}\)/);
  assert.ok(JSON.parse(read('manifest.firefox.json')).background.scripts.includes('bg-badge.js'));
  assert.match(read('scripts/build-firefox.js'), /'bg-badge\.js'/);
  assert.ok(fs.existsSync(path.join(root, B.ICON_MUTED)));
});
