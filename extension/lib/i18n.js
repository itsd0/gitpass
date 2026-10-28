// Tiny i18n helper: chrome.i18n (_locales/en|my|th) with English fallback.
(function (root) {
  'use strict';
  function t(key, fallback, subs) {
    try { const m = root.chrome && chrome.i18n && chrome.i18n.getMessage(key, subs); if (m) return m; } catch (_) { /* preview */ }
    let s = fallback != null ? fallback : key;
    if (subs) s = s.replace(/\$(\d)/g, (m, n) => (subs[Number(n) - 1] != null ? String(subs[Number(n) - 1]) : m));
    return s;
  }
  function apply(doc) {
    doc.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n, el.textContent); });
    doc.querySelectorAll('[data-i18n-ph]').forEach((el) => { el.placeholder = t(el.dataset.i18nPh, el.placeholder); });
    doc.querySelectorAll('[data-i18n-title]').forEach((el) => { el.title = t(el.dataset.i18nTitle, el.title); });
  }
  root.GitpassI18n = { t, apply };
})(typeof globalThis !== 'undefined' ? globalThis : this);
