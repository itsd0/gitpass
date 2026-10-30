// ---------- user settings (options page: chrome.storage.local.settings) ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

const SETTINGS = { autofill: true, savePrompts: true, autolockFollowsApp: true, autoFillTotp: false, autoSubmitAfterFill: false };
let neverAutoSubmit = [];
const isNeverAutoSubmit = () => neverAutoSubmit.includes(location.hostname.replace(/^www\./, '').toLowerCase());
// Only when the guards in content.js passed (top frame, fillable origin): the callbacks call rescan().
if (window.__gitpassInpage) {
  try {
    chrome.storage.local.get('settings', (r) => { Object.assign(SETTINGS, (r && r.settings) || {}); rescan(); });
    chrome.storage.local.get('neverAutoSubmit', (r) => { neverAutoSubmit = Array.isArray(r && r.neverAutoSubmit) ? r.neverAutoSubmit : []; });
    chrome.storage.onChanged.addListener((ch, area) => {
      if (area === 'local' && ch.settings) { Object.assign(SETTINGS, ch.settings.newValue || {}); if (!SETTINGS.savePrompts) closeBanner(); rescan(); }
      if (area === 'local' && ch.neverAutoSubmit) neverAutoSubmit = Array.isArray(ch.neverAutoSubmit.newValue) ? ch.neverAutoSubmit.newValue : [];
    });
  } catch (_) { /* storage unavailable: keep defaults */ }
}

// Opt-in "submit after fill" (#231). Called only from an explicit login fill, after that
// fill's origin check already passed. Uses form.requestSubmit() so page validation runs;
// never synthesizes keystrokes, never submits cards/identities/TOTP/passkeys (those
// branches never call this), and never submits on page load. Returns whether it submitted.
function maybeAutoSubmit(form, fillOrigin) {
  if (!SETTINGS.autoSubmitAfterFill || isNeverAutoSubmit()) return false;
  const F = form.fields || {};
  const pwEl = F.password || F.current;
  const filled = { username: !!(F.username && F.username.value), password: !!(pwEl && pwEl.value) };
  if (!GitpassForms.shouldAutoSubmit(form, filled, { pageOrigin: location.origin, fillOrigin })) return false;
  if (typeof form.scope.requestSubmit !== 'function') return false;
  try { form.scope.requestSubmit(); return true; } catch (_) { return false; }
}
