// gitpass background: passkey requests from pages. Every request needs an explicit click in the
// approval window (popup/passkey.html); only then is the host called with source:"popup", approved:true.
// The origin comes from the browser (sender.origin of the top frame), never from the page.
/* global chrome, GitpassPolicy */
'use strict';
(function (root) {
  const reqs = new Map(); // id -> { kind, origin, args, tabId, resolve, reject, winId }
  let n = 0;
  const err = (code, message) => Object.assign(new Error(message), { code });
  const APPROVE_URL = () => chrome.runtime.getURL('popup/passkey.html');
  function senderOrigin(sender) {
    if (!sender.tab || sender.frameId !== 0) throw err('not_supported', 'passkeys in frames are not supported');
    const o = sender.origin || GitpassPolicy.originOf(sender.url);
    if (!o || !GitpassPolicy.isFillableOrigin(o)) throw err('not_supported', 'insecure origin');
    return o;
  }
  /** From the content bridge: queue the request and open the approval window. Resolves with the host result. */
  function fromPage(msg, sender) {
    const kind = msg.type === 'passkey:create' ? 'create' : 'get';
    const origin = senderOrigin(sender);
    const a = msg.args && typeof msg.args === 'object' ? msg.args : {};
    const args = {};
    for (const k of ['rp_id', 'rp_name', 'challenge', 'user_id', 'user_name', 'user_display_name']) if (typeof a[k] === 'string') args[k] = a[k].slice(0, 4096);
    for (const k of ['algorithms', 'exclude_credentials', 'allow_credentials']) if (Array.isArray(a[k])) args[k] = a[k].slice(0, 64);
    const id = 'r' + (++n) + '-' + Date.now();
    return new Promise((resolve, reject) => {
      reqs.set(id, { kind, origin, args, tabId: sender.tab.id, resolve, reject });
      chrome.windows.create({ url: `${APPROVE_URL()}?req=${encodeURIComponent(id)}`, type: 'popup', width: 380, height: 420, focused: true })
        .then((w) => { const r = reqs.get(id); if (r) r.winId = w.id; })
        .catch((e) => { reqs.delete(id); reject(err('denied', String(e.message || e))); });
    });
  }
  /** From the approval page (an extension page in its own window). */
  async function fromUi(msg, host) {
    const r = reqs.get(msg.req);
    switch (msg.type) {
      case 'passkey-ui:request': if (!r) throw err('not_found', 'request expired');
        return { kind: r.kind, origin: r.origin, rp_id: r.args.rp_id || new URL(r.origin).hostname, user_name: r.args.user_name || '', rp_name: r.args.rp_name || '' };
      case 'passkey-ui:status': return host('status');
      case 'passkey-ui:request_unlock': return host('request_unlock', { reason: 'Use a passkey' });
      case 'passkey-ui:decide': {
        if (!r) throw err('not_found', 'request expired');
        if (!msg.approved) { reqs.delete(msg.req); r.reject(err('denied', 'The user declined')); return { done: true }; }
        try { const res = await host('passkey_' + r.kind, { ...r.args, origin: r.origin, source: 'popup', approved: true }); reqs.delete(msg.req); r.resolve(res); return { done: true }; }
        catch (e) { if (e.code === 'locked') throw e; reqs.delete(msg.req); r.reject(e); return { done: true, error: e.message, code: e.code }; }
      }
    }
    throw new Error('unknown message');
  }
  function isUiSender(sender) { return sender.id === chrome.runtime.id && typeof sender.url === 'string' && sender.url.startsWith(APPROVE_URL()); }
  if (root.chrome && chrome.windows && chrome.windows.onRemoved) chrome.windows.onRemoved.addListener((wid) => {
    for (const [id, r] of reqs) if (r.winId === wid) { reqs.delete(id); r.reject(err('denied', 'The user closed the gitpass prompt')); }
  });
  root.GitpassPasskey = { fromPage, fromUi, isUiSender, _reqs: reqs };
})(globalThis);
