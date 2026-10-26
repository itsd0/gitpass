// gitpass passkeys, isolated world (document_start): MAIN-world requests -> background `passkey:*`.
/* global chrome */
(function () {
  'use strict';
  const TAG = '__gitpass_pk';
  window.addEventListener('message', (e) => {
    const d = e.data; if (e.source !== window || !d || d[TAG] !== 'req' || (d.kind !== 'create' && d.kind !== 'get')) return;
    const reply = (r) => window.postMessage({ [TAG]: 'res', id: d.id, ...r }, location.origin);
    try {
      chrome.runtime.sendMessage({ type: 'passkey:' + d.kind, args: d.args || {} }, (r) => {
        if (chrome.runtime.lastError || !r) return reply({ ok: false, code: 'no_host', error: 'gitpass unavailable' });
        reply(r.ok ? { ok: true, result: r.result } : { ok: false, code: r.code, error: r.error });
      });
    } catch (_) { reply({ ok: false, code: 'no_host', error: 'gitpass unavailable' }); }
  });
})();
