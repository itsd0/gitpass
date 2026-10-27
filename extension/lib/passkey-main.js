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
      case 'not_supported': case 'no_host': case 'not_found': case 'cancelled': return null;
      default: return 'NotAllowedError'; // denied, locked, timeout, ...
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
  function promptNativeFallback(kind, options, orig) {
    return new Promise((resolve, reject) => {
      if (!root.document || !root.document.createElement) return orig(options).then(resolve, reject);
      const bar = root.document.createElement('div');
      bar.style.cssText = 'position:fixed;bottom:24px;left:50%;transform:translateX(-50%);background:#1C1C1E;color:#ECEDF0;border:1px solid #F28C28;border-radius:10px;padding:12px 18px;box-shadow:0 12px 32px rgba(0,0,0,.6);z-index:2147483647;display:flex;align-items:center;gap:12px;font:13px/1.4 system-ui,sans-serif';
      const text = root.document.createElement('span');
      text.textContent = 'Continue with platform passkey (Touch ID / Keychain)';
      const btn = root.document.createElement('button');
      btn.type = 'button';
      btn.textContent = 'Continue';
      btn.style.cssText = 'background:#F28C28;color:#1C1C1E;font-weight:600;border:none;border-radius:6px;padding:6px 14px;cursor:pointer;font:inherit';
      const cancelBtn = root.document.createElement('button');
      cancelBtn.type = 'button';
      cancelBtn.textContent = '✕';
      cancelBtn.style.cssText = 'background:none;color:#969AA4;border:none;cursor:pointer;font-size:16px;padding:4px';
      btn.addEventListener('click', () => {
        bar.remove();
        orig(options).then(resolve, reject);
      });
      cancelBtn.addEventListener('click', () => {
        bar.remove();
        reject(new DOMException('The operation was cancelled.', 'NotAllowedError'));
      });
      bar.append(text, btn, cancelBtn);
      (root.document.body || root.document.documentElement).appendChild(bar);
    });
  }

  async function run(kind, options, orig, relayFn) {
    const pk = options && options.publicKey;
    if (!pk || (options.mediation === 'conditional')) return orig(options); // conditional UI: leave to the browser
    let args; try { args = kind === 'create' ? createArgs(pk) : getArgs(pk); } catch (_) { return orig(options); }
    if (kind === 'create' && args.algorithms.length && !args.algorithms.includes(-7)) return orig(options);
    if (options.signal && options.signal.aborted) throw new DOMException('aborted', 'AbortError');
    const rFn = relayFn || relay;
    const res = await rFn(kind, args);
    if (res.ok) return credential(kind, res.result);
    // User cancelled or no matching credential: call original navigator.credentials method
    if (res.code === 'not_found' || res.code === 'cancelled' || res.code === 'denied') {
      try {
        return await orig(options);
      } catch (fallbackErr) {
        if (fallbackErr && (fallbackErr.name === 'NotAllowedError' || /activation|gesture/i.test(fallbackErr.message))) {
          return await promptNativeFallback(kind, options, orig);
        }
        throw fallbackErr;
      }
    }
    const name = errorName(res.code); if (!name) return orig(options);
    throw new DOMException(res.error || 'The operation either timed out or was not allowed.', name);
  }

  function def(o, props) { for (const [k, v] of Object.entries(props)) Object.defineProperty(o, k, { value: v, enumerable: true }); }
  const api = { b64u, unb64u, createArgs, getArgs, errorName, credential, run, promptNativeFallback, TAG };
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

  cc.create = function (options) { return run('create', options, origCreate); };
  cc.get = function (options) { return run('get', options, origGet); };
})(typeof globalThis !== 'undefined' ? globalThis : this);
