// Task: in-page passkey save must never fail silently (hyperbeat report). Covers runApproved/describe in
// bg-passkey.js and the 'fallback_native' path in lib/passkey-main.js.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const vm = require('vm');

function loadBg(tabReplies) {
  const sent = [];
  const chrome = {
    runtime: { id: 'x', lastError: undefined },
    tabs: { sendMessage: (tabId, msg, opts, cb) => { sent.push({ tabId, msg, opts }); const r = tabReplies.shift(); setTimeout(() => cb(r), 0); } },
    windows: { onRemoved: { addListener() {} } },
  };
  const ctx = { chrome, console, URL, setTimeout, clearTimeout, crypto: globalThis.crypto };
  ctx.globalThis = ctx;
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '..', 'bg-passkey.js'), 'utf8'), ctx);
  return { P: ctx.GitpassPasskey, sent };
}
const e = (code, message) => Object.assign(new Error(message || code), { code });

test('success: host result returned and dialog told "ok" with the title', async () => {
  const { P, sent } = loadBg([{ action: 'done' }]);
  const r = await P.runApproved({ kind: 'create', host: async () => ({ credential_id: 'c' }), args: {}, tabId: 7, reqId: 'r1', title: 'Hyperbeat' });
  assert.strictEqual(r.credential_id, 'c');
  assert.strictEqual(sent.length, 1);
  assert.strictEqual(sent[0].msg.ok, true); assert.strictEqual(sent[0].msg.title, 'Hyperbeat'); assert.strictEqual(sent[0].msg.reqId, 'r1');
  assert.strictEqual(sent[0].opts.frameId, 0);
});

test('error is always reported to the dialog (never silent); close rethrows the original error', async () => {
  const { P, sent } = loadBg([{ action: 'close' }]);
  await assert.rejects(P.runApproved({ kind: 'create', host: async () => { throw e('locked'); }, args: {}, tabId: 1, reqId: 'r' }), (x) => x.code === 'locked');
  assert.strictEqual(sent[0].msg.ok, false); assert.strictEqual(sent[0].msg.code, 'locked');
  assert.match(sent[0].msg.error, /locked/i);
});

test('no reply from the tab is treated as close', async () => {
  const { P } = loadBg([undefined]);
  await assert.rejects(P.runApproved({ kind: 'create', host: async () => { throw e('denied', 'device revoked'); }, args: {}, tabId: 1, reqId: 'r' }), (x) => x.code === 'denied');
});

test('"Use device passkey" -> fallback_native', async () => {
  const { P } = loadBg([{ action: 'native' }]);
  await assert.rejects(P.runApproved({ kind: 'create', host: async () => { throw e('app_not_running'); }, args: {}, tabId: 1, reqId: 'r' }), (x) => x.code === 'fallback_native');
});

test('retry after locked asks the app to unlock, then succeeds', async () => {
  const { P, sent } = loadBg([{ action: 'retry' }, { action: 'done' }]);
  const calls = []; let n = 0;
  const host = async (cmd, a) => { calls.push(cmd); if (cmd === 'passkey_create' && n++ === 0) throw e('locked'); return cmd === 'passkey_create' ? { ok: 1 } : {}; };
  assert.strictEqual((await P.runApproved({ kind: 'create', host, args: { approved: true }, tabId: 1, reqId: 'r' })).ok, 1);
  assert.deepStrictEqual([...calls], ['passkey_create', 'request_unlock', 'passkey_create']);
  assert.strictEqual(sent[1].msg.ok, true);
});

test('retries are bounded', async () => {
  const { P } = loadBg(Array(10).fill({ action: 'retry' }));
  await assert.rejects(P.runApproved({ kind: 'get', host: async (c) => { if (c.startsWith('passkey_')) throw e('internal'); }, args: {}, tabId: 1, reqId: 'r' }), (x) => x.code === 'denied');
});

test('describe gives readable messages', () => {
  const { P } = loadBg([]);
  assert.match(P.describe(e('locked')), /Unlock the gitpass app/);
  assert.match(P.describe(e('app_not_running')), /gitpass/i);
  assert.match(P.describe(e('weird', 'boom')), /boom \(weird\)/);
});

test('passkey-main: fallback_native hands the call to the browser authenticator', async () => {
  const M = require('../lib/passkey-main.js');
  const opts = { publicKey: { rp: { id: 'example.com', name: 'Ex' }, user: { id: new Uint8Array(4), name: 'a', displayName: 'a' }, challenge: new Uint8Array(16), pubKeyCredParams: [{ type: 'public-key', alg: -7 }] } };
  let origCalls = 0;
  const r = await M.run('create', opts, async (o) => { origCalls++; assert.strictEqual(o, opts); return 'native-cred'; }, async () => ({ ok: false, code: 'fallback_native', error: 'x' }));
  assert.strictEqual(r, 'native-cred'); assert.strictEqual(origCalls, 1);
  assert.strictEqual(M.errorName('fallback_native'), null);
});
