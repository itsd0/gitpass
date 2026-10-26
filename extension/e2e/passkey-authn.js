// Test-only software authenticator (Node crypto, real ES256) mirroring gitpass-host's passkey_create/get
// wire format. Used by the mock host and by test/passkey.test.js. Not shipped in the extension.
'use strict';
const crypto = require('crypto');
const b64u = (b) => Buffer.from(b).toString('base64url');
const unb64u = (s) => Buffer.from(s, 'base64url');
const sha256 = (b) => crypto.createHash('sha256').update(b).digest();
const AAGUID = Buffer.alloc(16);
// minimal canonical CBOR (major types 0,1,2,3,5 as needed)
function head(major, n) { if (n < 24) return Buffer.from([(major << 5) | n]); if (n < 256) return Buffer.from([(major << 5) | 24, n]); const b = Buffer.alloc(3); b[0] = (major << 5) | 25; b.writeUInt16BE(n, 1); return b; }
const int = (v) => v >= 0 ? head(0, v) : head(1, -1 - v);
const bytes = (b) => Buffer.concat([head(2, b.length), b]);
const text = (s) => { const b = Buffer.from(s); return Buffer.concat([head(3, b.length), b]); };
const map = (pairs) => Buffer.concat([head(5, pairs.length), ...pairs.flat()]);
function coseKey(pub) { const j = pub.export({ format: 'jwk' }); return map([[int(1), int(2)], [int(3), int(-7)], [int(-1), int(1)], [int(-2), bytes(unb64u(j.x))], [int(-3), bytes(unb64u(j.y))]]); }
function authData(rpId, count, attested) {
  const flags = 0x01 | 0x04 | 0x08 | 0x10 | (attested ? 0x40 : 0); const c = Buffer.alloc(4); c.writeUInt32BE(count);
  const parts = [sha256(rpId), Buffer.from([flags]), c];
  if (attested) { const id = attested.credId; const l = Buffer.alloc(2); l.writeUInt16BE(id.length); parts.push(AAGUID, l, id, coseKey(attested.pub)); }
  return Buffer.concat(parts);
}
function rpFor(origin, rpId) {
  const h = new URL(origin).hostname; const rp = rpId || h;
  if (!(h === rp || h.endsWith('.' + rp)) || !rp.includes('.') && rp !== 'localhost') throw Object.assign(new Error('rp_id does not match origin'), { code: 'origin_mismatch' });
  return rp;
}
const clientData = (type, challenge, origin) => Buffer.from(JSON.stringify({ type, challenge, origin, crossOrigin: false }));
/** store: { list(): [pk], put(pk) } where pk = { rp_id, user_name, user_handle, credential_id, pkcs8, sign_count } (all strings/b64url). */
function create(store, q) {
  const rp = rpFor(q.origin, q.rp_id);
  if (!q.challenge || !q.user_id) throw Object.assign(new Error('missing challenge/user_id'), { code: 'bad_request' });
  if (Array.isArray(q.algorithms) && q.algorithms.length && !q.algorithms.includes(-7)) throw Object.assign(new Error('only ES256'), { code: 'not_supported' });
  const ex = new Set(q.exclude_credentials || []);
  if (store.list().some((p) => p.rp_id === rp && ex.has(p.credential_id))) throw Object.assign(new Error('a passkey for this account is already saved'), { code: 'exists' });
  const { privateKey, publicKey } = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' });
  const credId = crypto.randomBytes(16);
  const ad = authData(rp, 0, { credId, pub: publicKey });
  const att = map([[text('fmt'), text('none')], [text('attStmt'), map([])], [text('authData'), bytes(ad)]]);
  const pk = { rp_id: rp, user_name: q.user_name || '', user_handle: q.user_id, credential_id: b64u(credId), pkcs8: privateKey.export({ format: 'der', type: 'pkcs8' }).toString('base64url'), sign_count: 0 };
  store.put(pk);
  return { item_id: 'v1:pk-' + pk.credential_id.slice(0, 8), rp_id: rp, credential_id: pk.credential_id, client_data_json: b64u(clientData('webauthn.create', q.challenge, q.origin)),
    attestation_object: b64u(att), authenticator_data: b64u(ad), public_key: b64u(publicKey.export({ format: 'der', type: 'spki' })), public_key_algorithm: -7, transports: ['internal', 'hybrid'] };
}
function get(store, q) {
  const rp = rpFor(q.origin, q.rp_id);
  if (!q.challenge) throw Object.assign(new Error('missing challenge'), { code: 'bad_request' });
  const allow = new Set(q.allow_credentials || []);
  const pk = store.list().find((p) => p.rp_id === rp && (!allow.size || allow.has(p.credential_id)));
  if (!pk) throw Object.assign(new Error('no passkey for this site'), { code: 'not_found' });
  pk.sign_count += 1; store.put(pk);
  const ad = authData(rp, pk.sign_count, null); const cd = clientData('webauthn.get', q.challenge, q.origin);
  const key = crypto.createPrivateKey({ key: unb64u(pk.pkcs8), format: 'der', type: 'pkcs8' });
  const sig = crypto.sign('sha256', Buffer.concat([ad, sha256(cd)]), key); // DER ECDSA, as WebAuthn requires
  return { item_id: 'v1:pk-' + pk.credential_id.slice(0, 8), credential_id: pk.credential_id, client_data_json: b64u(cd), authenticator_data: b64u(ad), signature: b64u(sig), user_handle: pk.user_handle, sign_count: pk.sign_count };
}
module.exports = { create, get, authData, coseKey, sha256, b64u, unb64u };
