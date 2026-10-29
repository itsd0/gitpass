// Pure helpers for the in-page save/update banner (content.js). No DOM and no chrome.* here so the
// title/vault rules and the Tab/Esc decisions can be unit tested in jsdom.
(function (root) {
  'use strict';
  const MAX_TITLE = 200;

  /** Trimmed title from page-side input; anything that is not a string becomes ''. */
  function titleValue(v) {
    return typeof v === 'string' ? v.trim() : '';
  }

  /** '' | 'required' | 'too_long' — the banner shows a message and the background re-checks. */
  function titleError(v) {
    const s = titleValue(v);
    if (!s) return 'required';
    if (s.length > MAX_TITLE) return 'too_long';
    return '';
  }

  /** Title passed to page:save_decision: trimmed, capped, or the hostname fallback. */
  function titleForSave(v, fallback) {
    return titleValue(v).slice(0, MAX_TITLE) || fallback || '';
  }

  /** Only id/name are ever rendered or sent back; ignore anything else the host may add. */
  function vaultList(vaults) {
    return (Array.isArray(vaults) ? vaults : []).filter((v) => v && typeof v.id === 'string' && v.id && typeof v.name === 'string' && v.name)
      .map((v) => ({ id: v.id, name: v.name }));
  }

  /** The chosen vault if it is still in the list, else the first (host order puts the default first). */
  function selectVault(vaults, id) {
    const list = vaultList(vaults);
    if (!list.length) return '';
    if (id && list.some((v) => v.id === id)) return id;
    return list[0].id;
  }

  /** Focus target for Tab / Shift+Tab at the edges of the banner; null lets the browser move normally. */
  function trapTab(key, shift, index, count) {
    if (key !== 'Tab' || count < 1) return null;
    if (index < 0) return 0;
    if (shift) return index === 0 ? count - 1 : null;
    return index === count - 1 ? 0 : null;
  }

  function isEscape(key) {
    return key === 'Escape';
  }

  const api = { MAX_TITLE, titleValue, titleError, titleForSave, vaultList, selectVault, trapTab, isEscape };
  root.GitpassBanner = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
