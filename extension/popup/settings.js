'use strict';
// ---------- user settings (options page) ----------
function popupSettings() {
  return new Promise((res) => {
    try { chrome.storage.local.get('settings', (r) => res(Object.assign({ autofill: true, savePrompts: true, autolockFollowsApp: true, colorPasswords: true, density: 'comfortable' }, (r && r.settings) || {}))); }
    catch (_) { res({ autofill: true, savePrompts: true, autolockFollowsApp: true, colorPasswords: true, density: 'comfortable' }); }
  });
}
