// Right-click context menu (#213): menu contents, secret-free titles, click
// routing to the clicked tab/frame, and the password-rules clamp the suggest
// path relies on (field rules -> page:generate in content.js).
const test = require('node:test');
const assert = require('node:assert');
const CM = require('../bg-context.js');
const PR = require('../lib/password-rules.js');

const t = (k, fb) => fb;
const originOf = (url) => { try { return new URL(url).origin; } catch { return null; } };
const isFillableOrigin = (o) => { try { const u = new URL(o); return u.protocol === 'https:' && !!u.hostname; } catch { return false; } };
const TAB = { id: 42, url: 'https://site.example/login' };

function mockChrome() {
  const created = [];
  let removed = 0;
  const listeners = {};
  return {
    created, get removed() { return removed; }, listeners,
    contextMenus: {
      removeAll: (cb) => { removed++; if (cb) cb(); },
      create: (item) => { created.push(item); },
      onClicked: { addListener: (f) => { listeners.clicked = f; } },
    },
    runtime: {
      onInstalled: { addListener: (f) => { listeners.installed = f; } },
      onStartup: { addListener: (f) => { listeners.startup = f; } },
    },
    tabs: {
      onActivated: { addListener: (f) => { listeners.activated = f; } },
      onUpdated: { addListener: (f) => { listeners.updated = f; } },
    },
  };
}

function deps(overrides = {}) {
  const calls = { host: [], fillTab: [], sendToTab: [], fillPaymentFrames: [], openPopup: 0 };
  const d = {
    chrome: mockChrome(),
    activeTab: async () => TAB,
    host: async (cmd, args) => {
      calls.host.push([cmd, args]);
      if (cmd === 'status') return { unlocked: true };
      if (cmd === 'match') return { items: [{ id: 'v1:i1', title: 'Example', subtitle: 'alice', category: 'login' }] };
      if (cmd === 'list_items') return { items: [] };
      if (cmd === 'fill_card') return { cc_number: '4111', cc_name: 'Alice' };
      if (cmd === 'fill_identity') return { name: 'Alice' };
      return {};
    },
    fillTab: async (tabId, origin, itemId, extra) => { calls.fillTab.push([tabId, origin, itemId, extra]); },
    sendToTab: async (tabId, msg, frameId) => { calls.sendToTab.push([tabId, msg, frameId]); return { filled: true }; },
    fillPaymentFrames: async (tabId, fields) => { calls.fillPaymentFrames.push([tabId, fields]); return 0; },
    openPopup: async () => { calls.openPopup++; },
    originOf, isFillableOrigin, t,
    calls,
  };
  return Object.assign(d, overrides);
}

test('locked vault: exactly one Unlock entry, no fill or suggest entries', () => {
  const items = CM.buildMenu({ locked: true }, t);
  assert.deepStrictEqual(items.map((i) => i.id), ['ctx-unlock']);
  assert.strictEqual(items[0].title, 'Unlock gitpass…');
});

test('unlocked: login submenu (max 5) + search + suggest + open; no card/identity parents without items', () => {
  const logins = Array.from({ length: 7 }, (_, i) => ({ id: 'v1:i' + i, title: 'Site ' + i, subtitle: 'user' + i, category: 'login' }));
  const items = CM.buildMenu({ locked: false, logins, cards: [], identities: [] }, t);
  const ids = items.map((i) => i.id);
  assert.strictEqual(ids[0], 'ctx-root');
  assert.ok(ids.includes('ctx-fill-login'));
  assert.strictEqual(ids.filter((x) => x.startsWith('login:')).length, 5, 'submenu caps at 5');
  assert.ok(ids.includes('ctx-search') && ids.includes('ctx-suggest') && ids.includes('ctx-open'));
  assert.ok(!ids.includes('ctx-fill-card') && !ids.includes('ctx-fill-identity'));
  assert.strictEqual(items.find((i) => i.id === 'login:v1:i0').title, 'Site 0 (user0)');
});

test('unlocked with cards and identities: submenus appear; without logins there is no fill-login parent', () => {
  const items = CM.buildMenu({ locked: false, logins: [], cards: [{ id: 'v1:c1', title: 'Visa' }], identities: [{ id: 'v1:d1', title: 'Me' }] }, t);
  const ids = items.map((i) => i.id);
  assert.ok(!ids.includes('ctx-fill-login') && ids.includes('ctx-search'));
  assert.ok(ids.includes('ctx-fill-card') && ids.includes('card:v1:c1'));
  assert.ok(ids.includes('ctx-fill-identity') && ids.includes('identity:v1:d1'));
  assert.strictEqual(items.find((i) => i.id === 'card:v1:c1').title, 'Visa');
});

test('unavailable host: parent plus Open only', () => {
  const items = CM.buildMenu({ unavailable: true }, t);
  assert.deepStrictEqual(items.map((i) => i.id), ['ctx-root', 'ctx-open']);
});

test('menu titles never contain secrets, even when the item objects carry them', () => {
  const items = CM.buildMenu({
    locked: false,
    logins: [{ id: 'v1:i1', title: 'Example', subtitle: 'alice', password: 'FAKE-pw-secret-123', category: 'login' }],
    cards: [{ id: 'v1:c1', title: 'Visa', number: '4111111111111111', category: 'credit_card' }],
    identities: [],
  }, t);
  const titles = items.map((i) => i.title).join('\n');
  assert.ok(!titles.includes('FAKE-pw-secret-123'), 'password leaked into a title');
  assert.ok(!titles.includes('4111111111111111'), 'card number leaked into a title');
  assert.ok(titles.includes('Example (alice)'));
});

test('rebuild writes the locked menu through the mocked chrome.contextMenus', async () => {
  const d = deps({ host: async (cmd) => { if (cmd === 'status') return { unlocked: false }; return {}; } });
  await CM.rebuild(d);
  assert.strictEqual(d.chrome.removed, 1);
  assert.deepStrictEqual(d.chrome.created.map((i) => i.id), ['ctx-unlock']);
});

test('loadState: locked from status, locked from a match failure, unavailable on a non-fillable tab', async () => {
  const locked = await CM.loadState(deps({ host: async (cmd) => { if (cmd === 'status') return { unlocked: false }; throw new Error('unreachable'); } }));
  assert.strictEqual(locked.locked, true);
  const locked2 = await CM.loadState(deps({
    host: async (cmd) => {
      if (cmd === 'status') return { unlocked: true };
      if (cmd === 'match') { const e = new Error('locked'); e.code = 'locked'; throw e; }
      return { items: [] };
    },
  }));
  assert.strictEqual(locked2.locked, true);
  const bad = await CM.loadState(deps({ activeTab: async () => ({ id: 9, url: 'chrome://extensions/' }) }));
  assert.strictEqual(bad.unavailable, true);
});

test('login click fills the clicked tab AND frame (not frame 0)', async () => {
  const d = deps();
  const r = await CM.handleClick({ menuItemId: 'login:v1:i9', frameId: 7 }, TAB, d);
  assert.deepStrictEqual(r, { action: 'fill', kind: 'login', frameId: 7 });
  assert.deepStrictEqual(d.calls.fillTab, [[42, 'https://site.example', 'v1:i9', { frameId: 7 }]]);
});

test('card click uses fill_card plus the clicked frame; identity click uses fill_identity', async () => {
  const d = deps();
  const r = await CM.handleClick({ menuItemId: 'card:v1:c9', frameId: 3 }, TAB, d);
  assert.strictEqual(r.action, 'fill');
  assert.strictEqual(d.calls.host.find(([c]) => c === 'fill_card')[1].item_id, 'v1:c9');
  assert.strictEqual(d.calls.host.find(([c]) => c === 'fill_card')[1].source, 'popup');
  assert.deepStrictEqual(d.calls.sendToTab, [[42, { type: 'gitpass-fill-fields', kind: 'card', origin: 'https://site.example', fields: { cc_number: '4111', cc_name: 'Alice' } }, 3]]);
  assert.strictEqual(d.calls.fillPaymentFrames.length, 1);
  const d2 = deps();
  await CM.handleClick({ menuItemId: 'identity:v1:d9', frameId: 0 }, TAB, d2);
  assert.ok(d2.calls.host.some(([c]) => c === 'fill_identity'));
  assert.strictEqual(d2.calls.sendToTab[0][2], 0);
});

test('suggest click asks the clicked frame; unlock and open route to their actions', async () => {
  const d = deps();
  const s = await CM.handleClick({ menuItemId: 'ctx-suggest', frameId: 5 }, TAB, d);
  assert.deepStrictEqual(s, { action: 'suggest', filled: true, frameId: 5 });
  assert.deepStrictEqual(d.calls.sendToTab, [[42, { type: 'gitpass-suggest-password', origin: 'https://site.example' }, 5]]);
  await CM.handleClick({ menuItemId: 'ctx-unlock', frameId: 0 }, TAB, d);
  assert.deepStrictEqual(d.calls.host.find(([c]) => c === 'request_unlock')[0], 'request_unlock');
  await CM.handleClick({ menuItemId: 'ctx-open', frameId: 0 }, TAB, d);
  await CM.handleClick({ menuItemId: 'ctx-search', frameId: 0 }, TAB, d);
  assert.strictEqual(d.calls.openPopup, 2);
});

test('clicks on non-fillable tabs and unknown ids are ignored, never filled', async () => {
  const d = deps();
  const r = await CM.handleClick({ menuItemId: 'login:v1:i9', frameId: 0 }, { id: 9, url: 'chrome://extensions/' }, d);
  assert.strictEqual(r.action, 'ignore');
  assert.strictEqual(d.calls.fillTab.length, 0);
  const r2 = await CM.handleClick({ menuItemId: 'ctx-root', frameId: 0 }, TAB, d);
  assert.strictEqual(r2.action, 'ignore');
});

test('parseMenuId keeps vault:item ids with colons intact', () => {
  assert.deepStrictEqual(CM.parseMenuId('login:vault1:item9'), { kind: 'login', itemId: 'vault1:item9' });
  assert.strictEqual(CM.parseMenuId('ctx-suggest'), null);
});

test('password rules: the suggest path clamps to maxlength and refuses too-short rules', () => {
  const fit = PR.sanitize(PR.parse('minlength: 8; maxlength: 12; required: lower; required: digit'));
  assert.strictEqual(fit.ok, true);
  assert.ok(fit.len <= 12, 'suggested length must respect maxlength, got ' + fit.len);
  const tooShort = PR.sanitize(PR.parse('maxlength: 4'));
  assert.deepStrictEqual(tooShort, { ok: false, reason: 'too_short' });
  const fromAttr = PR.sanitize({ maxlength: 16 });
  assert.strictEqual(fromAttr.len, 16);
});
