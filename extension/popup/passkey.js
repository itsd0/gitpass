// Passkey approval window. One explicit click here is the user's consent for one passkey_create/get.
/* global chrome, GitpassI18n, GitpassIcons */
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

  const matching = Array.isArray(q.matching) ? q.matching : [];
  const isMulti = q.kind === 'get' && matching.length >= 2;
  let selectedCredId = matching[0] ? matching[0].credential_id : null;

  if (isMulti) {
    $('account-label').style.display = 'none';
    $('account').style.display = 'none';
    $('account-choice').style.display = 'block';
    $('passkeys-count').textContent = t('pkPasskeysCount', '{n} passkeys for this site').replace('{n}', matching.length);

    const vaultNames = new Set(matching.map((m) => m.vault_name).filter(Boolean));
    const showVault = vaultNames.size > 1;
    const listEl = $('account-list');
    let selectedIndex = 0;
    const rows = [];

    function selectRow(idx, shouldFocus = true) {
      if (idx < 0 || idx >= matching.length) return;
      selectedIndex = idx;
      selectedCredId = matching[idx].credential_id;
      rows.forEach((row, i) => {
        const isSel = i === selectedIndex;
        row.classList.toggle('selected', isSel);
        row.setAttribute('aria-selected', isSel ? 'true' : 'false');
        row.setAttribute('tabindex', isSel ? '0' : '-1');
      });
      listEl.setAttribute('aria-activedescendant', `pk-ui-opt-${selectedIndex}`);
      if (rows[selectedIndex]) {
        rows[selectedIndex].scrollIntoView({ block: 'nearest' });
        if (shouldFocus) rows[selectedIndex].focus();
      }
    }

    matching.forEach((m, idx) => {
      const item = document.createElement('div');
      item.className = 'pk-item' + (idx === 0 ? ' selected' : '');
      item.setAttribute('role', 'option');
      item.setAttribute('id', `pk-ui-opt-${idx}`);
      item.setAttribute('aria-selected', idx === 0 ? 'true' : 'false');
      item.setAttribute('tabindex', idx === 0 ? '0' : '-1');
      item.title = m.user_name || '';

      const iconWrap = document.createElement('div');
      iconWrap.className = 'ic';
      if (typeof GitpassIcons !== 'undefined' && GitpassIcons.monogramIcon) {
        iconWrap.innerHTML = GitpassIcons.monogramIcon(m.user_name || q.rp_id, 20);
      } else {
        const mono = document.createElement('span');
        mono.className = 'monogram';
        mono.textContent = (m.user_name || q.rp_id || '?').trim().charAt(0).toUpperCase();
        iconWrap.appendChild(mono);
      }

      const info = document.createElement('div');
      info.className = 'pk-item-info';
      const u = document.createElement('span');
      u.className = 'pk-item-user';
      u.textContent = m.user_name || '—';
      u.title = m.user_name || '';
      info.appendChild(u);

      if (showVault && m.vault_name) {
        const v = document.createElement('span');
        v.className = 'pk-item-vault';
        v.textContent = m.vault_name;
        info.appendChild(v);
      }

      item.append(iconWrap, info);
      item.onclick = () => selectRow(idx, true);
      listEl.appendChild(item);
      rows.push(item);
    });

    listEl.onkeydown = (e) => {
      if (e.key === 'ArrowDown') { e.preventDefault(); selectRow(Math.min(matching.length - 1, selectedIndex + 1)); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); selectRow(Math.max(0, selectedIndex - 1)); }
      else if (e.key === 'Home') { e.preventDefault(); selectRow(0); }
      else if (e.key === 'End') { e.preventDefault(); selectRow(matching.length - 1); }
      else if (e.key === 'Enter') { e.preventDefault(); $('approve').click(); }
      else if (e.key === 'Escape') { e.preventDefault(); $('deny').click(); }
    };

    setTimeout(() => { if (rows[0]) rows[0].focus(); }, 50);
  } else {
    $('account').textContent = q.user_name || '—';
  }

  let busy = false;
  $('deny').onclick = async () => { if (busy) return; busy = true; await send('passkey-ui:decide', { approved: false }); window.close(); };
  $('approve').onclick = async () => {
    if (busy) return; busy = true; $('msg').textContent = '';
    const st = await send('passkey-ui:status');
    if (st.ok && st.result && st.result.unlocked === false) { await send('passkey-ui:request_unlock'); $('msg').textContent = t('pkLocked', 'gitpass is locked. Unlock the app to continue.'); busy = false; return; }
    const extra = { approved: true };
    if (selectedCredId) extra.credentialId = selectedCredId;
    const d = await send('passkey-ui:decide', extra);
    if (!d.ok && d.code === 'locked') { $('msg').textContent = t('pkLocked', 'gitpass is locked. Unlock the app to continue.'); busy = false; return; }
    $('msg').textContent = d.ok && d.result && d.result.error ? d.result.error : t('pkDone', 'Done. You can close this window.');
    setTimeout(() => window.close(), 300);
  };
})();
