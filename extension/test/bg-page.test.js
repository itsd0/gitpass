// page:* routing rules in bg-page.js, with a fake native host.
const test = require('node:test');
const assert = require('node:assert');
globalThis.GitpassPolicy = require('../lib/policy.js');
const B = require('../bg-page.js');

const tab = (url, frameId = 0, id = 1) => ({ tab: { id, url }, frameId, url });
const VAULTS = [{ id: 'v1', name: 'Personal', item_count: 2 }, { id: 'v2', name: 'Work', item_count: 0 }];
function fakeHost(items, pw = 'old') {
  const calls = [];
  const h = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'match') return { items };
    if (cmd === 'fill') { if (args.origin !== 'https://site.example') { const e = new Error('origin_mismatch'); throw e; } return { username: 'me', password: pw }; }
    if (cmd === 'save_login') return { item_id: 'v:new' };
    if (cmd === 'update_password') return { ok: true };
    if (cmd === 'totp') return { code: '123456', remaining: 10 };
    if (cmd === 'list_items') return { items: [{ id: 'v:c', category: 'credit_card' }, { id: 'v:l', category: 'login' }] };
    if (cmd === 'list_vaults') return { vaults: VAULTS };
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

test('sameSite', () => {
  assert.ok(GitpassPolicy.sameSite('https://login.a.com', 'https://www.a.com'));
  assert.ok(!GitpassPolicy.sameSite('https://a.com', 'http://a.com'));
  assert.ok(!GitpassPolicy.sameSite('https://a.com', 'https://b.com'));
});
