// gitpass passkeys, MAIN world (document_start). Wraps navigator.credentials.create/get for publicKey
// requests and relays them (via window.postMessage) to the isolated bridge -> background -> host.
// The page never gets key material: only the authenticator response built by gitpass.
// If gitpass can't handle a request (not supported / helper missing) the browser's own WebAuthn runs.
(function (root) {
  'use strict';
  const TAG = '__gitpass_pk';
  const b64u = (buf) => { const u = buf instanceof ArrayBuffer ? new Uint8Array(buf) : new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength); let s = ''; for (const c of u) s += String.fromCharCode(c); return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, ''); };
  const unb64u = (s) => { const b = atob(String(s).replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4)); const u = new Uint8Array(b.length); for (let i = 0; i < b.length; i++) u[i] = b.charCodeAt(i); return u.buffer; };
  const isBuf = (v) => v instanceof ArrayBuffer || ArrayBuffer.isView(v);
  /** navigator.credentials.create({publicKey}) options -> host passkey_create args (origin is added by the background). */
  function createArgs(pk) {
    if (!pk || !isBuf(pk.challenge) || !pk.user || !isBuf(pk.user.id)) throw new TypeError('invalid publicKey options');
    return { rp_id: pk.rp && pk.rp.id || undefined, rp_name: pk.rp && pk.rp.name || undefined, challenge: b64u(pk.challenge), user_id: b64u(pk.user.id),
      user_name: pk.user.name || '', user_display_name: pk.user.displayName || undefined,
      algorithms: (pk.pubKeyCredParams || []).map((p) => p.alg), exclude_credentials: (pk.excludeCredentials || []).map((c) => b64u(c.id)) };
  }
  function getArgs(pk) {
    if (!pk || !isBuf(pk.challenge)) throw new TypeError('invalid publicKey options');
    return { rp_id: pk.rpId || undefined, challenge: b64u(pk.challenge), allow_credentials: (pk.allowCredentials || []).map((c) => b64u(c.id)) };
  }
  /** host error code -> DOMException name (WebAuthn spec names). null = fall back to the browser. */
  function errorName(code) {
    switch (code) {
      case 'exists': return 'InvalidStateError';
      case 'origin_mismatch': return 'SecurityError';
      case 'not_supported': case 'no_host': return null;
      default: return 'NotAllowedError'; // denied, locked, not_found, timeout, ...
    }
  }
  function credential(kind, r) {
    const P = root.PublicKeyCredential; const proto = P && P.prototype || Object.prototype;
    const raw = unb64u(r.credential_id); const cd = unb64u(r.client_data_json);
    let response;
    if (kind === 'create') {
      const att = unb64u(r.attestation_object), ad = unb64u(r.authenticator_data), spki = unb64u(r.public_key);
      const RP = root.AuthenticatorAttestationResponse; response = Object.create(RP ? RP.prototype : Object.prototype);
      def(response, { clientDataJSON: cd, attestationObject: att, getTransports: () => (r.transports || ['internal', 'hybrid']).slice(), getAuthenticatorData: () => ad.slice(0), getPublicKey: () => spki.slice(0), getPublicKeyAlgorithm: () => r.public_key_algorithm || -7 });
    } else {
      const RP = root.AuthenticatorAssertionResponse; response = Object.create(RP ? RP.prototype : Object.prototype);
      def(response, { clientDataJSON: cd, authenticatorData: unb64u(r.authenticator_data), signature: unb64u(r.signature), userHandle: r.user_handle ? unb64u(r.user_handle) : null });
    }
    const c = Object.create(proto);
    def(c, { id: r.credential_id, rawId: raw, type: 'public-key', authenticatorAttachment: 'platform', response, getClientExtensionResults: () => ({}),
      toJSON: () => ({ id: r.credential_id, rawId: r.credential_id, type: 'public-key', authenticatorAttachment: 'platform', clientExtensionResults: {},
        response: kind === 'create' ? { clientDataJSON: r.client_data_json, attestationObject: r.attestation_object, authenticatorData: r.authenticator_data, publicKey: r.public_key, publicKeyAlgorithm: r.public_key_algorithm || -7, transports: r.transports || ['internal', 'hybrid'] }
          : { clientDataJSON: r.client_data_json, authenticatorData: r.authenticator_data, signature: r.signature, userHandle: r.user_handle || null } }) });
    return c;
  }
  function def(o, props) { for (const [k, v] of Object.entries(props)) Object.defineProperty(o, k, { value: v, enumerable: true }); }
  const api = { b64u, unb64u, createArgs, getArgs, errorName, credential, TAG };
  if (typeof module === 'object' && module.exports) { module.exports = api; return; }
  if (!root.navigator || !root.navigator.credentials || root[TAG]) return;
  Object.defineProperty(root, TAG, { value: true });
  const cc = root.navigator.credentials; const origCreate = cc.create.bind(cc), origGet = cc.get.bind(cc);
  let seq = 0; const waiting = new Map();
  root.addEventListener('message', (e) => {
    const d = e.data; if (e.source !== root || !d || d[TAG] !== 'res' || !waiting.has(d.id)) return;
    const w = waiting.get(d.id); waiting.delete(d.id); w(d);
  });
  const relay = (kind, args) => new Promise((resolve) => { const id = 'pk' + (++seq) + Math.random().toString(36).slice(2); waiting.set(id, resolve); root.postMessage({ [TAG]: 'req', id, kind, args }, root.location.origin); });
  async function run(kind, options, orig) {
    const pk = options && options.publicKey;
    if (!pk || (options.mediation === 'conditional')) return orig(options); // conditional UI: leave to the browser
    let args; try { args = kind === 'create' ? createArgs(pk) : getArgs(pk); } catch (_) { return orig(options); }
    if (kind === 'create' && args.algorithms.length && !args.algorithms.includes(-7)) return orig(options);
    if (options.signal && options.signal.aborted) throw new DOMException('aborted', 'AbortError');
    const res = await relay(kind, args);
    if (res.ok) return credential(kind, res.result);
    const name = errorName(res.code); if (!name) return orig(options);
    throw new DOMException(res.error || 'The operation either timed out or was not allowed.', name);
  }
  cc.create = function (options) { return run('create', options, origCreate); };
  cc.get = function (options) { return run('get', options, origGet); };
})(typeof globalThis !== 'undefined' ? globalThis : this);
