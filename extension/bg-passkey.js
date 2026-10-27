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
  /** From the content bridge: queue the request and prompt user in-page. Resolves with the host result. */
  async function fromPage(msg, sender, host) {
    const kind = msg.type === 'passkey:create' ? 'create' : 'get';
    const origin = senderOrigin(sender);
    const a = msg.args && typeof msg.args === 'object' ? msg.args : {};
    const args = {};
    for (const k of ['rp_id', 'rp_name', 'challenge', 'user_id', 'user_name', 'user_display_name']) if (typeof a[k] === 'string') args[k] = a[k].slice(0, 4096);
    for (const k of ['algorithms', 'exclude_credentials', 'allow_credentials']) if (Array.isArray(a[k])) args[k] = a[k].slice(0, 64);
    const rpId = args.rp_id || new URL(origin).hostname;

    let accountName = args.user_name || args.user_display_name || '';

    // Bug 4: If gitpass has no passkey for the site, don't show the prompt at all; pass call straight through
    if (kind === 'get' && host) {
      const q = await host('passkey_query', { origin, rp_id: rpId, allow_credentials: args.allow_credentials }).catch(() => null);
      if (!q || !q.matching || q.matching.length === 0) {
        throw err('not_found', 'No matching passkey found in gitpass');
      }
      accountName = q.matching[0].user_name || '';
    }

    const id = 'r' + (++n) + '-' + Date.now();
    return new Promise((resolve, reject) => {
      reqs.set(id, { kind, origin, args, tabId: sender.tab.id, resolve, reject, user_name: accountName });

      // Bug 3: Show prompt in-page in the current tab via content script
      if (root.chrome && chrome.tabs && chrome.tabs.sendMessage && sender.tab && typeof sender.tab.id === 'number') {
        chrome.tabs.sendMessage(sender.tab.id, {
          type: 'gitpass-passkey-prompt',
          reqId: id,
          kind,
          rpId,
          userName: accountName,
          origin
        }, { frameId: 0 }, (response) => {
          if (chrome.runtime && chrome.runtime.lastError || !response) {
            // Content script unreachable or error: fall back to extension popup window if available
            if (root.chrome && chrome.windows && chrome.windows.create) {
              chrome.windows.create({ url: `${APPROVE_URL()}?req=${encodeURIComponent(id)}`, type: 'popup', width: 380, height: 420, focused: true })
                .then((w) => { const r = reqs.get(id); if (r) r.winId = w.id; })
                .catch((e) => { reqs.delete(id); reject(err('denied', String(e.message || e))); });
              return;
            }
            reqs.delete(id);
            reject(err('cancelled', 'Prompt closed'));
            return;
          }
          if (response.approved) {
            reqs.delete(id);
            if (!host) { reject(err('no_host', 'no host')); return; }
            const title = args.rp_name || rpId;
            runApproved({ kind, host, args: { ...args, origin, source: 'popup', approved: true }, tabId: sender.tab.id, reqId: id, title })
              .then(resolve, reject);
          } else {
            reqs.delete(id);
            reject(err('cancelled', 'The user cancelled'));
          }
        });
        return;
      }

      // Standalone window fallback if tabs messaging is unavailable
      if (root.chrome && chrome.windows && chrome.windows.create) {
        chrome.windows.create({ url: `${APPROVE_URL()}?req=${encodeURIComponent(id)}`, type: 'popup', width: 380, height: 420, focused: true })
          .then((w) => { const r = reqs.get(id); if (r) r.winId = w.id; })
          .catch((e) => { reqs.delete(id); reject(err('denied', String(e.message || e))); });
      } else {
        reqs.delete(id);
        reject(err('denied', 'No UI prompt mechanism available'));
      }
    });
  }
  /** Host error -> a sentence for the in-page dialog (never a silent failure). */
  function describe(e) {
    const c = e && e.code;
    if (c === 'locked') return 'gitpass is locked. Unlock the gitpass app, then retry.';
    if (c === 'exists') return 'A passkey for this account is already saved in gitpass.';
    if (c === 'app_not_running' || c === 'no_host' || /native host|disconnected/i.test(String(e && e.message))) return 'The gitpass app or helper is not reachable. Open gitpass, then retry.';
    if (c === 'origin_mismatch') return 'This site asked for a passkey for a different domain (rpId does not match the page).';
    if (c === 'not_supported') return String((e && e.message) || 'Not supported by gitpass.');
    return String((e && e.message) || c || 'unknown error') + (c ? ` (${c})` : '');
  }
  function askTab(tabId, msg, ms = 120000) {
    return new Promise((resolve) => {
      const to = setTimeout(() => resolve({ action: 'close' }), ms);
      try {
        chrome.tabs.sendMessage(tabId, msg, { frameId: 0 }, (r) => { clearTimeout(to); void (chrome.runtime && chrome.runtime.lastError); resolve(r || { action: 'close' }); });
      } catch (_) { clearTimeout(to); resolve({ action: 'close' }); }
    });
  }
  /** After the in-page approval: call the host, report the outcome in the page dialog, and on failure let
   *  the user retry (unlocking first if needed), fall back to the browser's own passkey UI, or close. */
  async function runApproved({ kind, host, args, tabId, reqId, title }) {
    for (let attempt = 0; attempt < 5; attempt++) {
      let res, e;
      try { res = await host('passkey_' + kind, args); } catch (x) { e = x; }
      if (!e) { await askTab(tabId, { type: 'gitpass-passkey-result', reqId, ok: true, title }, 5000); return res; }
      const choice = await askTab(tabId, { type: 'gitpass-passkey-result', reqId, ok: false, code: e.code || null, error: describe(e) });
      if (choice.action === 'native') throw err('fallback_native', 'The user chose the device passkey');
      if (choice.action !== 'retry') throw e;
      if (e.code === 'locked') await host('request_unlock', { reason: kind === 'create' ? 'Save a passkey' : 'Use a passkey' }).catch(() => {});
    }
    throw err('denied', 'too many retries');
  }
  /** From the approval page (an extension page in its own window). */
  async function fromUi(msg, host) {
    const r = reqs.get(msg.req);
    switch (msg.type) {
      case 'passkey-ui:request': if (!r) throw err('not_found', 'request expired');
        return { kind: r.kind, origin: r.origin, rp_id: r.args.rp_id || new URL(r.origin).hostname, user_name: r.user_name || r.args.user_name || '', rp_name: r.args.rp_name || '' };
      case 'passkey-ui:status': return host('status');
      case 'passkey-ui:request_unlock': return host('request_unlock', { reason: 'Use a passkey' });
      case 'passkey-ui:decide': {
        if (!r) throw err('not_found', 'request expired');
        if (!msg.approved) { reqs.delete(msg.req); r.reject(err('cancelled', 'The user declined')); return { done: true }; }
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
  root.GitpassPasskey = { fromPage, fromUi, isUiSender, describe, runApproved, _reqs: reqs };
})(globalThis);
