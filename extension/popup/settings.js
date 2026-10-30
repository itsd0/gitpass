'use strict';
// ---------- user settings (options page) ----------
function popupSettings() {
  return new Promise((res) => {
    try { chrome.storage.local.get('settings', (r) => res(Object.assign({ autofill: true, savePrompts: true, autolockFollowsApp: true }, (r && r.settings) || {}))); }
    catch (_) { res({ autofill: true, savePrompts: true, autolockFollowsApp: true }); }
  });
}
