// page:* routing rules in bg-page.js, with a fake native host.
const test = require('node:test');
const assert = require('node:assert');
globalThis.chrome = {
  runtime: {
    id: 'mock',
    getURL: (p) => 'chrome-extension://mock/' + p,
  },
};
globalThis.GitpassPolicy = require('../lib/policy.js');
const B = require('../bg-page.js');

const tab = (url, frameId = 0, id = 1) => ({ tab: { id, url }, frameId, url });
const VAULTS = [{ id: 'v1', name: 'Personal', item_count: 2 }, { id: 'v2', name: 'Work', item_count: 0 }];
function fakeHost(items, pw = 'old') {
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'match') return { items: args.origin === 'https://site.example' ? items : [] };
    if (cmd === 'fill') { if (args.origin !== 'https://site.example' && !args.user_confirm && !args.confirm) { const e = new Error('origin_mismatch'); throw e; } return { username: 'me', password: pw }; }
    if (cmd === 'save_login') return { item_id: 'v:new' };
    if (cmd === 'update_password') return { ok: true };
    if (cmd === 'totp') return { code: '123456', remaining: 10 };
    if (cmd === 'list_items') return { items: [{ id: 'v:c', category: 'credit_card' }, { id: 'v:l', category: 'login', title: 'Site', subtitle: 'me', url: 'https://site.example', favorite: true, secret: 'leaked' }].filter((i) => !args.category || i.category === args.category) };
    if (cmd === 'list_vaults') return { vaults: VAULTS };
    if (cmd === 'add_url') return { ok: true };
    return {};
  };
  h.calls = calls; return h;
}
const login = { id: 'v:1', category: 'login', title: 'Site', subtitle: 'me', has_totp: true };

test('rejects iframes, non-tabs, http and uses sender origin', async () => {
  const h = fakeHost([login]);
  await assert.rejects(B.handle({ type: 'page:match' }, tab('https://site.example/x', 3), h), /top frame/);
  await assert.rejects(B.handle({ type: 'page:match' }, { frameId: 0 }, h), /from a tab/);
  await assert.rejects(B.handle({ type: 'page:match' }, tab('http://site.example/'), h), /not fillable/);
  const r = await B.handle({ type: 'page:match', origin: 'https://evil.example' }, tab('https://site.example/login'), h);
  assert.strictEqual(r.origin, 'https://site.example');
  assert.deepStrictEqual(h.calls[0], ['match', { origin: 'https://site.example' }]);
});

test('fill passes sender origin to host; totp only for matching items', async () => {
  const h = fakeHost([login]);
  const r = await B.handle({ type: 'page:fill', item_id: 'v:1' }, tab('https://site.example/'), h);
  assert.deepStrictEqual(r, { origin: 'https://site.example', username: 'me', password: 'old', totp: null });
  assert.strictEqual((await B.handle({ type: 'page:totp', item_id: 'v:1' }, tab('https://site.example/'), h)).code, '123456');
  await assert.rejects(B.handle({ type: 'page:totp', item_id: 'v:other' }, tab('https://site.example/'), h), /does not match/);

  // Cross-site fill without background-issued approval is refused even if message sends user_confirm
  await assert.rejects(B.handle({ type: 'page:fill', item_id: 'v:1', user_confirm: true }, tab('https://other.example/'), h), /origin_mismatch/);

  // Background approval granted via confirm UI allows single-use cross-site fill
  B._crossSiteApprovals.set('1:https://other.example:v:1', { ts: Date.now() });
  const cross = await B.handle({ type: 'page:fill', item_id: 'v:1' }, tab('https://other.example/'), h);
  assert.strictEqual(cross.username, 'me');
  assert.strictEqual(h.calls.at(-1)[1].user_confirm, true);

  // Approval is single-use: immediate second fill is refused
  await assert.rejects(B.handle({ type: 'page:fill', item_id: 'v:1' }, tab('https://other.example/'), h), /origin_mismatch/);

  // Cross-site TOTP without background approval is refused
  await assert.rejects(B.handle({ type: 'page:totp', item_id: 'v:1' }, tab('https://other.example/'), h), /does not match/);

  // Background approval granted via confirm UI allows single-use cross-site TOTP
  B._crossSiteApprovals.set('1:https://other.example:v:1', { ts: Date.now() });
  const crossTotp = await B.handle({ type: 'page:totp', item_id: 'v:1' }, tab('https://other.example/'), h);
  assert.strictEqual(crossTotp.code, '123456');

  // Approval is single-use: immediate second TOTP is refused
  await assert.rejects(B.handle({ type: 'page:totp', item_id: 'v:1' }, tab('https://other.example/'), h), /does not match/);
});

test('page:search is gated by trusted activation; page:add_url requires prior confirmed cross-site fill', async () => {
  const h = fakeHost([]);
  const sTab = tab('https://site.example/');

  // page:search refused when search not active for the tab
  await assert.rejects(B.handle({ type: 'page:search', query: 'sit' }, sTab, h), /search not active/);

  // page:search_enable requires user click
  await assert.rejects(B.handle({ type: 'page:search_enable' }, sTab, h), /search needs a user click/);
  await B.handle({ type: 'page:search_enable', user_click: true }, sTab, h);

  // Now search succeeds and returns logins without secrets
  const s = await B.handle({ type: 'page:search', query: 'sit' }, sTab, h);
  assert.strictEqual(s.items.length, 1);
  assert.strictEqual(s.items[0].id, 'v:l');
  assert.strictEqual(s.items[0].title, 'Site');
  assert.strictEqual(s.items[0].favorite, true);
  assert.ok(!('secret' in s.items[0]), 'no secrets returned in search summary');

  // Verify search activation does not slide and expires after 60s
  const activeEntry = B._searchActive.get(sTab.tab.id);
  assert.ok(activeEntry);
  const activatedTs = activeEntry.ts;
  await B.handle({ type: 'page:search', query: 'sit' }, sTab, h);
  assert.strictEqual(B._searchActive.get(sTab.tab.id).ts, activatedTs, 'search activation ts does not slide on query');

  activeEntry.ts = Date.now() - 61 * 1000;
  await assert.rejects(B.handle({ type: 'page:search', query: 'sit' }, sTab, h), /search not active/);

  // Re-enable for subsequent disable test
  await B.handle({ type: 'page:search_enable', user_click: true }, sTab, h);

  // page:search_disable deactivates search
  await B.handle({ type: 'page:search_disable' }, sTab, h);
  await assert.rejects(B.handle({ type: 'page:search', query: 'sit' }, sTab, h), /search not active/);

  // page:add_url requires user_click and prior confirmed cross-site fill approval
  await assert.rejects(B.handle({ type: 'page:add_url', item_id: 'v:l' }, sTab, h), /click/);
  await assert.rejects(B.handle({ type: 'page:add_url', item_id: 'v:l', user_click: true }, sTab, h), /confirmed cross-site fill/);

  // Grant add_url approval (as granted after a confirmed cross-site fill)
  B._addUrlApprovals.set('1:https://site.example:v:l', { ts: Date.now() });
  const added = await B.handle({ type: 'page:add_url', item_id: 'v:l', user_click: true }, sTab, h);
  assert.deepStrictEqual(added, { origin: 'https://site.example', ok: true });
  assert.deepStrictEqual(h.calls.at(-1), ['add_url', { item_id: 'v:l', origin: 'https://site.example', url: 'https://site.example' }]);

  // add_url approval is consumed (single-use)
  await assert.rejects(B.handle({ type: 'page:add_url', item_id: 'v:l', user_click: true }, sTab, h), /confirmed cross-site fill/);
});

test('page:list only cards/identities; card fill needs a click flag', async () => {
  const h = fakeHost([]);
  await assert.rejects(B.handle({ type: 'page:list', category: 'login' }, tab('https://site.example/'), h), /not listable/);
  const r = await B.handle({ type: 'page:list', category: 'credit_card' }, tab('https://site.example/'), h);
  assert.deepStrictEqual(r.items.map((i) => i.id), ['v:c']);
  await assert.rejects(B.handle({ type: 'page:fill_card', item_id: 'v:c' }, tab('https://site.example/'), h), /click/);
});

test('capture -> save banner -> save_login, password never returned to page', async () => {
  const h = fakeHost([]);
  const s = tab('https://site.example/login', 0, 7);
  const r = await B.handle({ type: 'page:capture', kind: 'login', username: 'new@x', password: 'pw1' }, s, h);
  assert.deepStrictEqual(r, { prompt: 'save', username: 'new@x', title: null, vaults: [{ id: 'v1', name: 'Personal' }, { id: 'v2', name: 'Work' }] });
  const p = await B.handle({ type: 'page:pending' }, tab('https://www.site.example/home', 0, 7), h); // redirect, same site
  assert.strictEqual(p.prompt, 'save'); assert.ok(!('password' in p));
  assert.deepStrictEqual(p.vaults, [{ id: 'v1', name: 'Personal' }, { id: 'v2', name: 'Work' }]);
  await B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h);
  assert.deepStrictEqual(h.calls.at(-1), ['save_login', { origin: 'https://site.example', title: 'site.example', username: 'new@x', password: 'pw1' }]);
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h), /nothing to save/);
});

test('capture with changed password -> update; same password -> no prompt; multi-step username carried', async () => {
  const s = tab('https://site.example/', 0, 9);
  let h = fakeHost([login], 'old');
  assert.strictEqual((await B.handle({ type: 'page:capture', kind: 'login', username: 'me', password: 'old' }, s, h)).prompt, null);
  h = fakeHost([login], 'old');
  await B.handle({ type: 'page:capture', kind: 'username', username: 'me' }, s, h);
  const r = await B.handle({ type: 'page:capture', kind: 'login', username: '', password: 'newer' }, s, h);
  assert.strictEqual(r.prompt, 'update'); assert.strictEqual(r.username, 'me');
  await B.handle({ type: 'page:save_decision', action: 'update', user_click: true }, s, h);
  assert.deepStrictEqual(h.calls.at(-1), ['update_password', { item_id: 'v:1', password: 'newer' }]);
  assert.strictEqual((await B.handle({ type: 'page:pending' }, tab('https://other.example/', 0, 9), h)).prompt, null);
});

test('save decision: trimmed custom title, chosen vault, page-side item id ignored', async () => {
  const h = fakeHost([]);
  const s = tab('https://site.example/login', 0, 21);
  await B.handle({ type: 'page:capture', kind: 'login', username: 'new@x', password: 'pw1' }, s, h);
  const r = await B.handle({
    type: 'page:save_decision', action: 'save', title: '  Work Login  ', vault_id: 'v2',
    item_id: 'v:victim', username: 'evil', password: 'evil', user_click: true,
  }, s, h);
  assert.deepStrictEqual(r, { done: true, action: 'save', item_id: 'v:new' });
  assert.deepStrictEqual(h.calls.find(([cmd]) => cmd === 'save_login')[1], {
    origin: 'https://site.example', title: 'Work Login', username: 'new@x', password: 'pw1', vault_id: 'v2',
  });
});

test('save decision: rejected vault/title keep the pending capture for retry', async () => {
  const s = tab('https://site.example/login', 0, 22);
  const capture = (host) => B.handle({ type: 'page:capture', kind: 'login', username: 'new@x', password: 'pw1' }, s, host);
  let h = fakeHost([]);
  await capture(h);
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'save', vault_id: 'nope', user_click: true }, s, h), /vault not found/);
  assert.ok(!h.calls.some(([cmd]) => cmd === 'save_login'));
  // Retry with the still-pending capture: the banner can just pick another vault.
  await B.handle({ type: 'page:save_decision', action: 'save', vault_id: 'v2', user_click: true }, s, h);
  assert.strictEqual(h.calls.find(([cmd]) => cmd === 'save_login')[1].vault_id, 'v2');
  h = fakeHost([]);
  await capture(h);
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'save', title: 'x'.repeat(201), user_click: true }, s, h), /too long/);
  assert.ok(!h.calls.some(([cmd]) => cmd === 'save_login'));
  // A cleared title still falls back to the hostname (same as the old banner).
  h = fakeHost([]);
  await capture(h);
  await B.handle({ type: 'page:save_decision', action: 'save', title: '   ', user_click: true }, s, h);
  assert.strictEqual(h.calls.find(([cmd]) => cmd === 'save_login')[1].title, 'site.example');
});

test('locked vault lookup surfaces locked and keeps the capture for retry', async () => {
  const s = tab('https://site.example/login', 0, 26);
  let failLocked = 0;
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'match') return { items: [] };
    if (cmd === 'list_vaults') {
      if (failLocked > 0) { failLocked--; const e = new Error('vault is locked'); e.code = 'locked'; throw e; }
      return { vaults: VAULTS };
    }
    if (cmd === 'save_login') return { item_id: 'v:new' };
    return {};
  };
  await B.handle({ type: 'page:capture', kind: 'login', username: 'new@x', password: 'pw1' }, s, h);
  failLocked = 1;
  const e = await B.handle({ type: 'page:save_decision', action: 'save', vault_id: 'v2', user_click: true }, s, h).catch((x) => x);
  assert.strictEqual(e.code, 'locked', 'the banner must see locked, not a missing vault');
  assert.ok(!calls.some(([cmd]) => cmd === 'save_login'));
  const r = await B.handle({ type: 'page:save_decision', action: 'save', vault_id: 'v2', user_click: true }, s, h);
  assert.deepStrictEqual(r, { done: true, action: 'save', item_id: 'v:new' });
  assert.strictEqual(calls.filter(([cmd]) => cmd === 'save_login').length, 1);
});

test('a second decision while saving is rejected and only one item is written', async () => {
  const s = tab('https://site.example/login', 0, 27);
  let release;
  const gate = new Promise((res) => { release = res; });
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'match') return { items: [] };
    if (cmd === 'list_vaults') return { vaults: VAULTS };
    if (cmd === 'save_login') { await gate; return { item_id: 'v:new' }; }
    return {};
  };
  await B.handle({ type: 'page:capture', kind: 'login', username: 'new@x', password: 'pw1' }, s, h);
  const first = B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h);
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h), /already saving/);
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'dismiss' }, s, h), /already saving/);
  release();
  assert.deepStrictEqual(await first, { done: true, action: 'save', item_id: 'v:new' });
  assert.strictEqual(calls.filter(([cmd]) => cmd === 'save_login').length, 1);
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h), /nothing to save/);
});

test('a capture during an in-flight save is processed after it, not dropped', async () => {
  const s = tab('https://site.example/login', 0, 28);
  let release;
  const gate = new Promise((res) => { release = res; });
  const h = async (cmd, args) => {
    if (cmd === 'match') return { items: [] };
    if (cmd === 'list_vaults') return { vaults: VAULTS };
    if (cmd === 'save_login') { await gate; return { item_id: 'v:new' }; }
    return {};
  };
  await B.handle({ type: 'page:capture', kind: 'login', username: 'first@x', password: 'pw1' }, s, h);
  const saving = B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h);
  const during = B.handle({ type: 'page:capture', kind: 'login', username: 'second@x', password: 'pw2' }, s, h);
  release();
  assert.deepStrictEqual(await saving, { done: true, action: 'save', item_id: 'v:new' });
  const cap = await during;
  assert.strictEqual(cap.prompt, 'save');
  assert.strictEqual(cap.username, 'second@x');
  assert.strictEqual((await B.handle({ type: 'page:pending' }, s, h)).username, 'second@x');
});

test('a pending lookup during a save sees its outcome (success or retry)', async () => {
  // Success: the redirected page must not resurrect the banner.
  const ok = tab('https://site.example/login', 0, 29);
  let releaseOk;
  const okGate = new Promise((res) => { releaseOk = res; });
  const okHost = async (cmd) => {
    if (cmd === 'match') return { items: [] };
    if (cmd === 'list_vaults') return { vaults: VAULTS };
    if (cmd === 'save_login') { await okGate; return { item_id: 'v:new' }; }
    return {};
  };
  await B.handle({ type: 'page:capture', kind: 'login', username: 'first@x', password: 'pw1' }, ok, okHost);
  const saving = B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, ok, okHost);
  const pendingDuring = B.handle({ type: 'page:pending' }, ok, okHost);
  releaseOk();
  await saving;
  assert.deepStrictEqual(await pendingDuring, { prompt: null });

  // Failure: the capture is kept, so the queued pending still offers a retry.
  const bad = tab('https://site.example/login', 0, 30);
  let releaseBad;
  const badGate = new Promise((res) => { releaseBad = res; });
  let fail = true;
  const badHost = async (cmd) => {
    if (cmd === 'match') return { items: [] };
    if (cmd === 'list_vaults') return { vaults: VAULTS };
    if (cmd === 'save_login') { await badGate; if (fail) { const e = new Error('host write failed'); e.code = 'internal'; throw e; } return { item_id: 'v:new' }; }
    return {};
  };
  await B.handle({ type: 'page:capture', kind: 'login', username: 'retry@x', password: 'pw3' }, bad, badHost);
  const failing = B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, bad, badHost);
  const pendingAfterFailure = B.handle({ type: 'page:pending' }, bad, badHost);
  releaseBad();
  await assert.rejects(failing, /host write failed/);
  const shown = await pendingAfterFailure;
  assert.strictEqual(shown.prompt, 'save');
  assert.strictEqual(shown.username, 'retry@x');
  fail = false; // the banner's Save now succeeds against the retained capture
  assert.deepStrictEqual(await B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, bad, badHost), { done: true, action: 'save', item_id: 'v:new' });
});

test('update prompt: save_new makes a second login; save/update actions are not interchangeable', async () => {
  const s = tab('https://site.example/', 0, 23);
  let h = fakeHost([login], 'old');
  assert.strictEqual((await B.handle({ type: 'page:capture', kind: 'login', username: 'me', password: 'newer' }, s, h)).prompt, 'update');
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h), /action/);
  h = fakeHost([login], 'old');
  assert.strictEqual((await B.handle({ type: 'page:capture', kind: 'login', username: 'me', password: 'newer' }, s, h)).prompt, 'update');
  const r = await B.handle({ type: 'page:save_decision', action: 'save_new', title: 'Second account', vault_id: 'v2', user_click: true }, s, h);
  assert.deepStrictEqual(r, { done: true, action: 'save', item_id: 'v:new' });
  assert.deepStrictEqual(h.calls.find(([cmd]) => cmd === 'save_login')[1], {
    origin: 'https://site.example', title: 'Second account', username: 'me', password: 'newer', vault_id: 'v2',
  });
  // save_new is only for update prompts, and update needs an update prompt.
  const savePrompt = fakeHost([]);
  assert.strictEqual((await B.handle({ type: 'page:capture', kind: 'login', username: 'other', password: 'pw' }, s, savePrompt)).prompt, 'save');
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'save_new', user_click: true }, s, savePrompt), /action/);
  assert.strictEqual((await B.handle({ type: 'page:capture', kind: 'login', username: 'other', password: 'pw' }, s, savePrompt)).prompt, 'save');
  await assert.rejects(B.handle({ type: 'page:save_decision', action: 'update', user_click: true }, s, savePrompt), /action/);
});

test('page:open_app forwards only the item saved from this tab', async () => {
  const h = fakeHost([]);
  const s = tab('https://site.example/', 0, 24);
  await B.handle({ type: 'page:open_app' }, s, h);
  assert.deepStrictEqual(h.calls.at(-1), ['open_in_app', {}]);
  await B.handle({ type: 'page:capture', kind: 'login', username: 'new@x', password: 'pw1' }, s, h);
  await B.handle({ type: 'page:save_decision', action: 'save', user_click: true }, s, h);
  await B.handle({ type: 'page:open_app', last_save: true, item_id: 'v:victim' }, s, h);
  assert.deepStrictEqual(h.calls.at(-1), ['open_in_app', { item_id: 'v:new' }]);
  await B.handle({ type: 'page:open_app', last_save: true }, tab('https://other.example/', 0, 25), h);
  assert.deepStrictEqual(h.calls.at(-1), ['open_in_app', {}]);
});

test('fromConfirmUi handles approve and cancel decisions', () => {
  let resolved = null, rejected = null;
  B._pendingConfirms.set('req-1', {
    reqId: 'req-1',
    tabId: 5,
    origin: 'https://site.example',
    itemId: 'v:1',
    pageHost: 'site.example',
    itemHost: 'other.example',
    title: 'Other Login',
    resolve: (v) => { resolved = v; },
    reject: (e) => { rejected = e; },
  });

  // Query metadata for UI
  const reqData = B.fromConfirmUi({ type: 'confirm-ui:request', req: 'req-1' });
  assert.deepStrictEqual(reqData, { pageHost: 'site.example', itemHost: 'other.example', title: 'Other Login' });

  // Approve decision grants approval and resolves
  const dec = B.fromConfirmUi({ type: 'confirm-ui:decide', req: 'req-1', approved: true });
  assert.strictEqual(dec.approved, true);
  assert.deepStrictEqual(resolved, { approved: true });
  assert.ok(B._crossSiteApprovals.has('5:https://site.example:v:1'));

  // Cancel decision rejects
  B._pendingConfirms.set('req-2', {
    reqId: 'req-2',
    tabId: 5,
    origin: 'https://site.example',
    itemId: 'v:2',
    pageHost: 'site.example',
    itemHost: 'other.example',
    title: 'Other 2',
    resolve: () => {},
    reject: (e) => { rejected = e; },
  });
  B.fromConfirmUi({ type: 'confirm-ui:decide', req: 'req-2', approved: false });
  assert.ok(rejected && /cancelled/i.test(rejected.message));

  // Sender validation: content script senders or mismatched req are refused
  const confirmUiUrl = 'chrome-extension://mock/popup/confirm.html?req=req-3';
  const validSender = { id: 'mock', url: confirmUiUrl };
  const contentScriptSender = { id: 'mock', url: 'https://site.example/login' };
  const contentScriptWithTab = { id: 'mock', tab: { id: 10, url: 'https://site.example/login' }, url: 'https://site.example/login' };
  const wrongExtSender = { id: 'wrong', url: confirmUiUrl };

  assert.strictEqual(B.isConfirmUiSender(validSender), true);
  assert.strictEqual(B.isConfirmUiSender(contentScriptSender), false);
  assert.strictEqual(B.isConfirmUiSender(contentScriptWithTab), false);
  assert.strictEqual(B.isConfirmUiSender(wrongExtSender), false);

  assert.throws(() => B.fromConfirmUi({ type: 'confirm-ui:request', req: 'req-3' }, contentScriptSender), /unauthorized sender/);
  assert.throws(() => B.fromConfirmUi({ type: 'confirm-ui:decide', req: 'req-3', approved: true }, contentScriptWithTab), /unauthorized sender/);
  assert.throws(() => B.fromConfirmUi({ type: 'confirm-ui:request', req: 'req-wrong' }, validSender), /mismatched request id/);
});

test('confirm-ui TTL expiry rejects pending waiter and cleans up pendingConfirms', () => {
  let rejected = null;
  const cleanup = () => { B._pendingConfirms.delete('req-exp'); };
  B._pendingConfirms.set('req-exp', {
    reqId: 'req-exp',
    tabId: 5,
    origin: 'https://site.example',
    itemId: 'v:exp',
    pageHost: 'site.example',
    itemHost: 'other.example',
    title: 'Other Exp',
    resolve: () => {},
    reject: (e) => { rejected = e; },
    cleanup,
    ts: Date.now() - 70000,
  });

  assert.throws(() => B.fromConfirmUi({ type: 'confirm-ui:request', req: 'req-exp' }), /request expired/);
  assert.ok(rejected && /request expired/i.test(rejected.message));
  assert.strictEqual(B._pendingConfirms.has('req-exp'), false);
});

test('background.js rejects confirm-ui:decide from a sender with a tab', async () => {
  let messageListener = null;
  const originalChrome = globalThis.chrome;
  globalThis.chrome = {
    runtime: {
      id: 'mock-ext',
      getURL: (p) => 'chrome-extension://mock-ext/' + p,
      onMessage: { addListener: (fn) => { messageListener = fn; } },
      connectNative: () => ({ onMessage: { addListener: () => {} }, onDisconnect: { addListener: () => {} }, postMessage: () => {} }),
    },
    idle: { onStateChanged: { addListener: () => {} } },
    tabs: { onRemoved: { addListener: () => {} } },
    windows: { onRemoved: { addListener: () => {} } },
  };

  delete require.cache[require.resolve('../background.js')];
  require('../background.js');
  assert.ok(messageListener, 'background message listener registered');

  const tabSender = {
    id: 'mock-ext',
    tab: { id: 42, url: 'https://evil.example/login' },
    url: 'https://evil.example/login',
  };

  const res = await new Promise((resolve) => {
    messageListener({ type: 'confirm-ui:decide', req: 'some-req', approved: true }, tabSender, resolve);
  });

  assert.strictEqual(res.ok, false);
  assert.match(res.error, /unknown message/);

  globalThis.chrome = originalChrome;
});

test('sameSite', () => {
  assert.ok(GitpassPolicy.sameSite('https://login.a.com', 'https://www.a.com'));
  assert.ok(!GitpassPolicy.sameSite('https://a.com', 'http://a.com'));
  assert.ok(!GitpassPolicy.sameSite('https://a.com', 'https://b.com'));
});

test('page:generate derives origin strictly from sender, not from message, and records to host', async () => {
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'generate') return { password: 'FAKE-generated-pw-123' };
    if (cmd === 'record_generated_password') return { ok: true };
    return {};
  };
  h.calls = calls;

  const s = tab('https://real-site.example/signup', 0, 42);
  const msg = {
    type: 'page:generate',
    origin: 'https://attacker.example',
  };

  const r = await B.handle(msg, s, h);
  assert.strictEqual(r.origin, 'https://real-site.example');
  assert.strictEqual(r.password, 'FAKE-generated-pw-123');

  const genCall = calls.find(([cmd]) => cmd === 'generate');
  assert.ok(genCall, 'generate must be called');

  const recCall = calls.find(([cmd]) => cmd === 'record_generated_password');
  assert.ok(recCall, 'record_generated_password must be called');
  assert.strictEqual(recCall[1].password, 'FAKE-generated-pw-123');
  assert.strictEqual(recCall[1].origin, 'https://real-site.example');
  assert.strictEqual(recCall[1].source, 'extension');
});

test('page:generate with site rules records generated password to host with sender origin', async () => {
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'generate') return { password: 'FAKE-rules-pw-456' };
    if (cmd === 'record_generated_password') return { ok: true };
    return {};
  };
  h.calls = calls;
  const s = tab('https://real-site.example/signup');
  const r = await B.handle({ type: 'page:generate', rules: { maxlength: 16 } }, s, h);
  assert.strictEqual(r.password, 'FAKE-rules-pw-456');
  const recCall = calls.find(([cmd]) => cmd === 'record_generated_password');
  assert.ok(recCall, 'record_generated_password must be called with site rules');
  assert.strictEqual(recCall[1].password, 'FAKE-rules-pw-456');
  assert.strictEqual(recCall[1].origin, 'https://real-site.example');
  assert.strictEqual(recCall[1].source, 'extension');
});

test('page:generate without rules keeps the 20-character default', async () => {
  const h = fakeHost([]);
  const r = await B.handle({ type: 'page:generate' }, tab('https://site.example/signup'), h);
  assert.strictEqual(r.origin, 'https://site.example');
  assert.deepStrictEqual(h.calls[0], ['generate', { len: 20, digits: true, symbols: true }]);
});

test('page:generate clamps site rules and sends only validated classes to the host (#188)', async () => {
  const h = fakeHost([]);
  await B.handle({ type: 'page:generate', rules: { maxlength: 16, minlength: 3, entries: [{ kind: 'lower', required: true }, { kind: 'emoji' }, { kind: 'special', chars: '-_ a9', required: false }], junk: '<html>' } }, tab('https://site.example/signup'), h);
  assert.deepStrictEqual(h.calls[0], ['generate', { len: 16, rules: { classes: [{ kind: 'lower', required: true }, { kind: 'special', required: false, chars: '-_' }] } }]);
});

test('page:generate rejects rules that cannot hold a strong password', async () => {
  for (const rules of [{ maxlength: 4 }, { minlength: 200 }, { minlength: 30, maxlength: 12 }]) {
    const h = fakeHost([]);
    await assert.rejects(B.handle({ type: 'page:generate', rules }, tab('https://site.example/signup'), h), /password rules/);
    assert.strictEqual(h.calls.length, 0, 'the host is never asked');
  }
});

test('page:generate ignores a non-object rules value instead of crashing', async () => {
  const h = fakeHost([]);
  await B.handle({ type: 'page:generate', rules: 'maxlength: 4' }, tab('https://site.example/signup'), h);
  assert.deepStrictEqual(h.calls[0], ['generate', { len: 20, rules: { classes: [{ kind: 'lower', required: false }, { kind: 'upper', required: false }, { kind: 'digit', required: true }, { kind: 'special', required: true }] } }]);
});

test('decide(): empty username with 0, 1, 2+ matching logins, preselection and unchanged password', async () => {
  const origin = 'https://site.example';
  const makeHost = (items, passwords = {}) => async (cmd, args) => {
    if (cmd === 'match') return { items };
    if (cmd === 'fill') return { username: 'user', password: passwords[args.item_id] || 'stored-pass' };
    return {};
  };

  // 0 matching logins -> save prompt
  const h0 = makeHost([]);
  const d0 = await B.decide(h0, origin, { username: '', password: 'new-pass' });
  assert.deepStrictEqual(d0, { prompt: 'save' });

  // 1 matching login -> changed password yields update
  const item1 = { id: 'v:1', category: 'login', title: 'Main', subtitle: 'user1@example.com' };
  const h1 = makeHost([item1], { 'v:1': 'old-pass' });
  const d1 = await B.decide(h1, origin, { username: '', password: 'new-pass' });
  assert.strictEqual(d1.prompt, 'update');
  assert.strictEqual(d1.item_id, 'v:1');
  assert.strictEqual(d1.title, 'Main');
  assert.strictEqual(d1.hasUser, true);

  // 1 matching login -> unchanged password gives no prompt (null)
  const h1same = makeHost([item1], { 'v:1': 'same-pass' });
  const d1same = await B.decide(h1same, origin, { username: '', password: 'same-pass' });
  assert.strictEqual(d1same, null);

  // 2+ matching logins -> preselects most recently used when no lastFilled
  const itemOld = { id: 'v:old', category: 'login', title: 'Old Acc', subtitle: 'old@example.com', last_used: 1000 };
  const itemNew = { id: 'v:new', category: 'login', title: 'New Acc', subtitle: 'new@example.com', last_used: 5000 };
  const itemNoUser = { id: 'v:blank', category: 'login', title: 'Blank Acc', subtitle: '', last_used: 2000 };
  const h2 = makeHost([itemOld, itemNew, itemNoUser], { 'v:new': 'pass-old' });
  const d2 = await B.decide(h2, origin, { username: '', password: 'changed-pass' }, 12);
  assert.strictEqual(d2.prompt, 'update');
  assert.strictEqual(d2.item_id, 'v:new', 'item with highest last_used preselected');
  assert.deepStrictEqual(d2.accounts.map((a) => a.id), ['v:new', 'v:blank', 'v:old'], 'sorted newest last_used first');

  // 2+ matching logins -> preselection from lastFilled
  B.rememberFilled(12, origin, 'v:old');
  const d2lf = await B.decide(h2, origin, { username: '', password: 'changed-pass' }, 12);
  assert.strictEqual(d2lf.prompt, 'update');
  assert.strictEqual(d2lf.item_id, 'v:old', 'lastFilled takes precedence over last_used');

  // 2+ matching logins -> unchanged password on chosen login gives no prompt (null)
  const h2same = makeHost([itemOld, itemNew], { 'v:old': 'unchanged-pass', 'v:new': 'other-pass' });
  const d2same = await B.decide(h2same, origin, { username: '', password: 'unchanged-pass' }, 12);
  assert.strictEqual(d2same, null, 'unchanged password on preselected login suppresses banner');
});

test('save decision: item_id not matching origin is rejected', async () => {
  const origin = 'https://site.example';
  const itemA = { id: 'v:a', category: 'login', title: 'Site A', subtitle: 'a@site.com' };
  const itemB = { id: 'v:b', category: 'login', title: 'Site B', subtitle: 'b@site.com' };
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'match') return { items: args.origin === origin ? [itemA, itemB] : [] };
    if (cmd === 'fill') return { username: 'a@site.com', password: 'old-password' };
    if (cmd === 'update_password') return { ok: true };
    if (cmd === 'list_vaults') return { vaults: VAULTS };
    return {};
  };

  const s = tab(origin + '/account', 0, 42);
  // Capture with empty username on a site with 2 matching logins
  const capRes = await B.handle({ type: 'page:capture', kind: 'login', username: '', password: 'new-password' }, s, h);
  assert.strictEqual(capRes.prompt, 'update');
  assert.ok(capRes.accounts && capRes.accounts.length === 2);

  // Attempt update with forged item_id from a different origin/item
  await assert.rejects(
    B.handle({ type: 'page:save_decision', action: 'update', item_id: 'v:foreign', user_click: true }, s, h),
    (e) => e.code === 'origin_mismatch' || /item does not match/.test(e.message)
  );

  // Updating with one of the valid matching items succeeds
  const okRes = await B.handle({ type: 'page:save_decision', action: 'update', item_id: 'v:b', user_click: true }, s, h);
  assert.strictEqual(okRes.done, true);
  assert.strictEqual(okRes.action, 'update');
  assert.strictEqual(okRes.item_id, 'v:b');
  const upCall = calls.find(([cmd]) => cmd === 'update_password');
  assert.deepStrictEqual(upCall[1], { item_id: 'v:b', password: 'new-password' });
});
