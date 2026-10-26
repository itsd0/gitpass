#!/usr/bin/env node
// Mock gitpass-host: speaks the v0.6 contract over native messaging with fake data. Test-only.
// Env: GITPASS_MOCK_DIR (log.jsonl + control.json). Starts LOCKED. Unlocks on request_unlock,
// on legacy `unlock`, or when control.json contains {"unlocked":true}; pushes {"event":"unlock"|"lock"}.
'use strict';
const fs = require('fs'), path = require('path');
const DIR = process.env.GITPASS_MOCK_DIR || path.join(__dirname, '.mock');
fs.mkdirSync(DIR, { recursive: true });
const LOG = path.join(DIR, 'log.jsonl'), CTRL = path.join(DIR, 'control.json'), STATE = path.join(DIR, 'state.json');
const log = (o) => fs.appendFileSync(LOG, JSON.stringify({ t: Date.now(), pid: process.pid, ...o }) + '\n');
const readJson = (p, d) => { try { return JSON.parse(fs.readFileSync(p, 'utf8')); } catch { return d; } };
// Shared across host processes (Chrome may spawn several): unlocked state lives in state.json.
const isUnlocked = () => !!readJson(STATE, {}).unlocked;
const setUnlocked = (v) => fs.writeFileSync(STATE, JSON.stringify({ unlocked: v }));

const V = { id: 'v1', name: 'Personal' };
const now = '2026-10-26T00:00:00Z';
const ITEMS = [
  { id: 'v1:i1', category: 'login', title: 'Localhost Test Login', subtitle: 'alice@example.com', url: 'http://localhost', favicon_host: 'localhost', has_totp: true, favorite: true, _u: 'alice@example.com', _p: 'Corr3ct-H0rse-Battery' },
  { id: 'v1:i2', category: 'login', title: 'GitHub', subtitle: 'alice', url: 'https://github.com', favicon_host: 'github.com', has_totp: false, favorite: false, _u: 'alice', _p: 'gh-pass-123' },
  { id: 'v1:i3', category: 'credit_card', title: 'Visa', subtitle: '•••• 1234', has_totp: false, favorite: false },
  { id: 'v1:i4', category: 'secure_note', title: 'Wi-Fi notes', subtitle: '', has_totp: false, favorite: false },
].map((i) => ({ vault_id: V.id, vault_name: V.name, updated_at: now, ...i }));
const authn = require('./passkey-authn');
const PKS = path.join(DIR, 'passkeys.json');
const pkStore = { list: () => readJson(PKS, []), put: (pk) => { const l = readJson(PKS, []).filter((x) => x.credential_id !== pk.credential_id); l.push(pk); fs.writeFileSync(PKS, JSON.stringify(l)); } };
const pub = (i) => { const o = { ...i }; delete o._u; delete o._p; return o; };
const host = (s) => { try { return new URL(s).hostname; } catch { return ''; } };
const matches = (o) => ITEMS.filter((i) => i.url && host(i.url) && host(i.url) === host(o));

function send(obj) {
  const b = Buffer.from(JSON.stringify(obj)); const h = Buffer.alloc(4); h.writeUInt32LE(b.length);
  process.stdout.write(Buffer.concat([h, b])); log({ dir: 'out', msg: obj });
}
function needUnlock() { if (!isUnlocked()) { const e = new Error('vault is locked'); e.code = 'locked'; throw e; } }

function handle(m) {
  const origin = m.origin || m.url;
  switch (m.cmd) {
    case 'hello': return { app_version: '0.6.0-mock', unlocked: isUnlocked(), account: { name: 'Mock User', email: 'alice@example.com', vault_count: 1 } };
    case 'status': return { unlocked: isUnlocked(), app_running: true, account: { name: 'Mock User', email: 'alice@example.com', vault_count: 1 }, idle_timeout_secs: 600 };
    case 'request_unlock': case 'unlock': setTimeout(() => { setUnlocked(true); send({ event: 'unlock' }); }, 0); return { unlocked: true };
    case 'lock': setUnlocked(false); setTimeout(() => send({ event: 'lock' }), 0); return { unlocked: false };
    case 'list_items': case 'search': { needUnlock(); const q = (m.query || '').toLowerCase(); let r = ITEMS.filter((i) => !q || i.title.toLowerCase().includes(q)); if (m.category) r = r.filter((i) => i.category === m.category); return { items: r.slice(0, m.limit || 100).map(pub) }; }
    case 'match': needUnlock(); return { items: matches(origin).map(pub) };
    case 'get_item': { needUnlock(); const i = ITEMS.find((x) => x.id === m.item_id); if (!i) throw Object.assign(new Error('no item'), { code: 'not_found' });
      return { item: { ...pub(i), fields: i._u ? [{ name: 'username', label: 'username', value: i._u, concealed: false, kind: 'text' }, { name: 'password', label: 'password', concealed: true, kind: 'password' }] : [], tags: [], created_at: now } }; }
    case 'reveal': { needUnlock(); const i = ITEMS.find((x) => x.id === m.item_id); return { value: i && m.field === 'password' ? i._p : (i && i._u) || '' }; }
    case 'fill': { needUnlock(); const i = ITEMS.find((x) => x.id === m.item_id); if (!i) throw Object.assign(new Error('no item'), { code: 'not_found' });
      if (!matches(origin).includes(i)) throw Object.assign(new Error('origin mismatch'), { code: 'origin_mismatch' });
      return { username: i._u, password: i._p, totp: i.has_totp ? '123456' : undefined }; }
    case 'totp': needUnlock(); return { code: '123456', remaining: 20, period: 30 };
    case 'save_login': needUnlock(); return { item_id: 'v1:new' + Date.now() };
    case 'update_password': needUnlock(); return { ok: true };
    case 'generate': return { password: 'Gen-' + 'x'.repeat(Math.max(4, (m.len || 20) - 4)) };
    case 'list_vaults': needUnlock(); return { vaults: [{ ...V, item_count: ITEMS.length }] };
    case 'open_in_app': return {};
    case 'fill_card': case 'fill_identity': needUnlock(); return {};
    case 'passkey_create': case 'passkey_get': needUnlock();
      if (!(m.source === 'popup' && m.approved === true)) throw Object.assign(new Error('approval required'), { code: 'denied' });
      return m.cmd === 'passkey_create' ? authn.create(pkStore, m) : authn.get(pkStore, m);
  }
  throw Object.assign(new Error('unknown cmd ' + m.cmd), { code: 'bad_request' });
}

log({ dir: 'start', argv: process.argv.slice(2) });
let buf = Buffer.alloc(0);
process.stdin.on('data', (d) => {
  buf = Buffer.concat([buf, d]);
  while (buf.length >= 4) {
    const n = buf.readUInt32LE(0); if (buf.length < 4 + n) break;
    const m = JSON.parse(buf.slice(4, 4 + n).toString('utf8')); buf = buf.slice(4 + n);
    log({ dir: 'in', msg: m });
    try { send({ id: m.id, ok: true, result: handle(m) }); } catch (e) { send({ id: m.id, ok: false, error: e.code || 'internal', message: e.message }); }
  }
});
process.stdin.on('end', () => process.exit(0));
// control.json lets the test simulate the desktop app unlocking/locking (e.g. Touch ID in the app).
let lastCtrl = '';
setInterval(() => {
  let c; try { c = fs.readFileSync(CTRL, 'utf8'); } catch { return; }
  if (c === lastCtrl) return; lastCtrl = c;
  const v = readJson(CTRL, {}); if (typeof v.unlocked !== 'boolean') return;
  if (v.unlocked !== isUnlocked() || v.seq) { setUnlocked(v.unlocked); send({ event: v.unlocked ? 'unlock' : 'lock' }); }
}, 100);
