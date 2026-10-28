// Cross-site fill confirmation window (extension-owned, isolated from page context).
/* global chrome, GitpassI18n */
'use strict';
(async function () {
  const t = (k, f, p) => GitpassI18n.t(k, f, p);
  GitpassI18n.apply(document);
  const req = new URLSearchParams(location.search).get('req');
  const $ = (id) => document.getElementById(id);
  const send = (type, extra = {}) =>
    new Promise((res) => chrome.runtime.sendMessage({ type, req, ...extra }, (r) => res(r || { ok: false, error: 'no response' })));

  const r = await send('confirm-ui:request');
  if (!r.ok || !r.result) {
    $('msg').textContent = t('pkExpired', 'This request is no longer active.');
    $('fillBtn').disabled = true;
    return;
  }

  const { pageHost, itemHost } = r.result;
  $('msg').textContent = t('csConfirmCrossSite', `Fill on ${pageHost}? This login is for ${itemHost}`, [pageHost, itemHost]);

  let busy = false;
  const doCancel = async () => {
    if (busy) return;
    busy = true;
    await send('confirm-ui:decide', { approved: false });
    window.close();
  };
  const doFill = async () => {
    if (busy) return;
    busy = true;
    await send('confirm-ui:decide', { approved: true });
    window.close();
  };

  $('cancelBtn').addEventListener('click', doCancel);
  $('fillBtn').addEventListener('click', doFill);

  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      doCancel();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      doFill();
    }
  });

  setTimeout(() => $('fillBtn').focus(), 10);
})();
