// Tiny i18n helper: chrome.i18n (_locales/en|my|th) with English fallback.
(function (root) {
  'use strict';
  function t(key, fallback) {
    try { const m = root.chrome && chrome.i18n && chrome.i18n.getMessage(key); if (m) return m; } catch (_) { /* preview */ }
    return fallback != null ? fallback : key;
  }
  function apply(doc) {
    doc.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n, el.textContent); });
    doc.querySelectorAll('[data-i18n-ph]').forEach((el) => { el.placeholder = t(el.dataset.i18nPh, el.placeholder); });
    doc.querySelectorAll('[data-i18n-title]').forEach((el) => { el.title = t(el.dataset.i18nTitle, el.title); });
  }
  root.GitpassI18n = { t, apply };
})(typeof globalThis !== 'undefined' ? globalThis : this);
