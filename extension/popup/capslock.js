// Caps Lock hint for password fields (issue #325): shows a small inline hint while the
// field has focus and Caps Lock is on. Pure module; wired up from popup.js.
'use strict';
(function (root) {
  function capsOn(e) {
    return !!(e && typeof e.getModifierState === 'function' && e.getModifierState('CapsLock'));
  }

  /** Wires keydown/keyup/focus/blur on `input` to toggle `hint.hidden`. Returns an unhook function. */
  function attachCapsLockHint(input, hint) {
    if (!input || !hint) return () => {};
    let focused = false;
    let on = false;
    const render = () => { hint.hidden = !(focused && on); };
    const key = (e) => { on = capsOn(e); render(); };
    const focus = (e) => { focused = true; on = capsOn(e); render(); };
    const blur = () => { focused = false; on = false; render(); };
    input.addEventListener('keydown', key);
    input.addEventListener('keyup', key);
    input.addEventListener('focus', focus);
    input.addEventListener('blur', blur);
    render();
    return () => {
      input.removeEventListener('keydown', key);
      input.removeEventListener('keyup', key);
      input.removeEventListener('focus', focus);
      input.removeEventListener('blur', blur);
    };
  }

  const api = { attachCapsLockHint, capsOn };
  root.GitpassCapsLock = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
