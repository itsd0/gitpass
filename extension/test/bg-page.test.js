// page:* routing rules in bg-page.js, with a fake native host.
const test = require('node:test');
const assert = require('node:assert');
globalThis.GitpassPolicy = require('../lib/policy.js');
const B = require('../bg-page.js');

const tab = (url, frameId = 0, id = 1) => ({ tab: { id, url }, frameId, url });
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
  assert.deepStrictEqual(r, { prompt: 'save', username: 'new@x', title: null });
  const p = await B.handle({ type: 'page:pending' }, tab('https://www.site.example/home', 0, 7), h); // redirect, same site
  assert.strictEqual(p.prompt, 'save'); assert.ok(!('password' in p));
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

test('sameSite', () => {
  assert.ok(GitpassPolicy.sameSite('https://login.a.com', 'https://www.a.com'));
  assert.ok(!GitpassPolicy.sameSite('https://a.com', 'http://a.com'));
  assert.ok(!GitpassPolicy.sameSite('https://a.com', 'https://b.com'));
});
