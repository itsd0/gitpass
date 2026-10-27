// Passkey approval window. One explicit click here is the user's consent for one passkey_create/get.
/* global chrome, GitpassI18n */
'use strict';
(async function () {
  const t = (k, f) => GitpassI18n.t(k, f); GitpassI18n.apply(document);
  const req = new URLSearchParams(location.search).get('req');
  const $ = (id) => document.getElementById(id);
  const send = (type, extra = {}) => new Promise((res) => chrome.runtime.sendMessage({ type, req, ...extra }, (r) => res(r || { ok: false, error: 'no response' })));
  const r = await send('passkey-ui:request');
  if (!r.ok) { $('title').textContent = t('pkExpired', 'This request is no longer active.'); $('approve').disabled = true; return; }
  const q = r.result;
  $('title').textContent = q.kind === 'create' ? t('pkCreateTitle', 'Save a passkey?') : t('pkGetTitle', 'Sign in with a passkey?');
  $('site').textContent = q.rp_id + (q.origin && new URL(q.origin).hostname !== q.rp_id ? ` (${q.origin})` : '');
  $('account').textContent = q.user_name || '—';
  let busy = false;
  $('deny').onclick = async () => { if (busy) return; busy = true; await send('passkey-ui:decide', { approved: false }); window.close(); };
  $('approve').onclick = async () => {
    if (busy) return; busy = true; $('msg').textContent = '';
    const st = await send('passkey-ui:status');
    if (st.ok && st.result && st.result.unlocked === false) { await send('passkey-ui:request_unlock'); $('msg').textContent = t('pkLocked', 'gitpass is locked. Unlock the app to continue.'); busy = false; return; }
    const d = await send('passkey-ui:decide', { approved: true });
    if (!d.ok && d.code === 'locked') { $('msg').textContent = t('pkLocked', 'gitpass is locked. Unlock the app to continue.'); busy = false; return; }
    $('msg').textContent = d.ok && d.result.error ? d.result.error : t('pkDone', 'Done. You can close this window.');
    setTimeout(() => window.close(), 300);
  };
})();
