/* global chrome, GitpassOnboard, GitpassI18n */
'use strict';
const $ = (id) => document.getElementById(id);
(async () => {
  if (globalThis.GitpassI18n) GitpassI18n.apply(document);
  const s = await GitpassOnboard.getSettings();
  for (const k of Object.keys(GitpassOnboard.DEFAULTS)) {
    $(k).checked = !!s[k];
    $(k).onchange = async () => { await GitpassOnboard.setSettings({ [k]: $(k).checked }); $('saved').textContent = 'Saved'; setTimeout(() => { $('saved').textContent = ''; }, 1200); };
  }
  try { const r = await GitpassOnboard.send({ type: 'onboard-shortcut' }); $('shortcut').textContent = r.shortcut || 'not set'; } catch (_) { $('shortcut').textContent = 'Cmd+Shift+X'; }
  $('change-shortcut').onclick = () => {
    const isFirefox = typeof browser !== 'undefined' || navigator.userAgent.includes('Firefox');
    if (isFirefox) $('saved').textContent = 'Firefox: about:addons → ⚙ → Manage Extension Shortcuts';
    else chrome.tabs.create({ url: 'chrome://extensions/shortcuts' });
  };
})();
