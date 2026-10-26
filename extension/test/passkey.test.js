// Passkeys: MAIN-world codec + a real-crypto round trip (Node webcrypto verifies the ES256 assertion
// exactly like a relying party: SPKI from getPublicKey(), signature over authData || SHA-256(clientDataJSON)).
const test = require('node:test'); const assert = require('node:assert');
const { webcrypto } = require('crypto');
const P = require('../lib/passkey-main.js');
const authn = require('../e2e/passkey-authn.js');
const enc = new TextEncoder();
function derToRaw(der) { // ECDSA DER -> r||s (32 bytes each) for webcrypto
  const d = new Uint8Array(der); let i = 2; const read = () => { i++; let l = d[i++]; let v = d.slice(i, i + l); i += l; while (v.length > 32 && v[0] === 0) v = v.slice(1); const o = new Uint8Array(32); o.set(v, 32 - v.length); return o; };
  const r = read(), s = read(); const out = new Uint8Array(64); out.set(r); out.set(s, 32); return out; }
function mem() { const m = new Map(); return { list: () => [...m.values()], put: (p) => m.set(p.credential_id, { ...p }) }; }
test('b64url round trip and option serialisation', () => {
  const bytes = new Uint8Array([0, 251, 255, 1, 2]);
  assert.deepStrictEqual(new Uint8Array(P.unb64u(P.b64u(bytes))), bytes);
  const a = P.createArgs({ challenge: bytes, rp: { id: 'example.com', name: 'Ex' }, user: { id: new Uint8Array([9]), name: 'a@x', displayName: 'A' }, pubKeyCredParams: [{ type: 'public-key', alg: -7 }, { type: 'public-key', alg: -257 }], excludeCredentials: [{ type: 'public-key', id: new Uint8Array([7]) }] });
  assert.deepStrictEqual(a, { rp_id: 'example.com', rp_name: 'Ex', challenge: P.b64u(bytes), user_id: 'CQ', user_name: 'a@x', user_display_name: 'A', algorithms: [-7, -257], exclude_credentials: ['Bw'] });
  assert.deepStrictEqual(P.getArgs({ challenge: bytes.buffer, rpId: 'example.com', allowCredentials: [{ type: 'public-key', id: new Uint8Array([7]).buffer }] }), { rp_id: 'example.com', challenge: P.b64u(bytes), allow_credentials: ['Bw'] });
  assert.throws(() => P.createArgs({ challenge: 'nope', user: { id: new Uint8Array(1) } }), TypeError);
});
test('host error codes map to WebAuthn DOMException names', () => {
  assert.strictEqual(P.errorName('exists'), 'InvalidStateError');
  assert.strictEqual(P.errorName('denied'), 'NotAllowedError');
  assert.strictEqual(P.errorName('locked'), 'NotAllowedError');
  assert.strictEqual(P.errorName('origin_mismatch'), 'SecurityError');
  assert.strictEqual(P.errorName('not_supported'), null);
});
test('real ES256: create -> credential object -> get verifies with webcrypto, signCount increments', async () => {
  const store = mem(); const origin = 'https://login.example.com'; const ch = P.b64u(webcrypto.getRandomValues(new Uint8Array(32)));
  const r = authn.create(store, { origin, rp_id: 'example.com', challenge: ch, user_id: 'dXNlcg', user_name: 'u@example.com', algorithms: [-7] });
  const cred = P.credential('create', r);
  assert.strictEqual(cred.type, 'public-key'); assert.strictEqual(cred.id, r.credential_id);
  const cd = JSON.parse(Buffer.from(cred.response.clientDataJSON).toString());
  assert.deepStrictEqual([cd.type, cd.challenge, cd.origin], ['webauthn.create', ch, origin]);
  assert.strictEqual(cred.response.getPublicKeyAlgorithm(), -7);
  const ad = new Uint8Array(cred.response.getAuthenticatorData());
  assert.deepStrictEqual(Buffer.from(ad.slice(0, 32)), authn.sha256('example.com'));
  assert.strictEqual(ad[32] & 0x45, 0x45, 'UP|UV|AT set');
  const key = await webcrypto.subtle.importKey('spki', cred.response.getPublicKey(), { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify']);
  for (const want of [1, 2]) {
    const ch2 = P.b64u(webcrypto.getRandomValues(new Uint8Array(32)));
    const a = P.credential('get', authn.get(store, { origin, rp_id: 'example.com', challenge: ch2, allow_credentials: [r.credential_id] }));
    const adA = new Uint8Array(a.response.authenticatorData); const cdA = new Uint8Array(a.response.clientDataJSON);
    assert.strictEqual(Buffer.from(adA.slice(33, 37)).readUInt32BE(), want);
    assert.strictEqual(JSON.parse(Buffer.from(cdA).toString()).challenge, ch2);
    const msg = new Uint8Array([...adA, ...new Uint8Array(await webcrypto.subtle.digest('SHA-256', cdA))]);
    assert.ok(await webcrypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, key, derToRaw(a.response.signature), msg), 'signature verifies');
    msg[40] ^= 1; assert.ok(!(await webcrypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, key, derToRaw(a.response.signature), msg)), 'tampered fails');
    assert.strictEqual(Buffer.from(a.response.userHandle).toString(), 'user');
  }
  assert.throws(() => authn.get(store, { origin: 'https://evil.test', rp_id: 'example.com', challenge: ch }), /rp_id/);
  assert.throws(() => authn.create(store, { origin, rp_id: 'example.com', challenge: ch, user_id: 'dXNlcg', exclude_credentials: [r.credential_id] }), (e) => e.code === 'exists');
  void enc;
});
