// Pure helpers for the popup password generator (issue #68). Generation itself stays in the host.
'use strict';
(function (root) {
  const MODES = {
    random: { min: 8, max: 64, def: 20 },
    memorable: { min: 3, max: 15, def: 5 },
    pin: { min: 4, max: 12, def: 6 },
  };
  const DEFAULTS = { mode: 'random', length: 20, digits: true, symbols: true };

  function mode(m) { return Object.prototype.hasOwnProperty.call(MODES, m) ? m : 'random'; }

  function clampLength(m, n) {
    const lim = MODES[mode(m)];
    const v = Math.round(Number(n));
    return Number.isFinite(v) ? Math.min(lim.max, Math.max(lim.min, v)) : lim.def;
  }

  /** Stored settings only: mode/length/toggles. The generated value is never part of this shape. */
  function safeSettings(s) {
    const m = mode(s && s.mode);
    return { mode: m, length: clampLength(m, s && s.length), digits: !s || s.digits !== false, symbols: !s || s.symbols !== false };
  }

  /** Rough 0-100 meter score for display only. */
  function strengthScore(pw) {
    if (!pw) return 0;
    let pool = 0;
    if (/[a-z]/.test(pw)) pool += 26;
    if (/[A-Z]/.test(pw)) pool += 26;
    if (/[0-9]/.test(pw)) pool += 10;
    if (/[^a-zA-Z0-9]/.test(pw)) pool += 33;
    let score = Math.min(100, Math.round((pw.length * Math.log2(Math.max(pool, 2))) / 128 * 100));
    if (new Set(pw).size <= 2) score = Math.min(score, 10);
    return score;
  }

  function strengthKey(score) { return score < 40 ? 'weak' : score < 70 ? 'fair' : score < 90 ? 'good' : 'excellent'; }

  const api = { MODES, DEFAULTS, mode, clampLength, safeSettings, strengthScore, strengthKey };
  root.GitpassGenerator = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
