// Pure helpers for the popup password generator (issue #68). Generation itself stays in the host.
'use strict';
(function (root) {
  const MODES = {
    random: { min: 8, max: 64, def: 20 },
    memorable: { min: 3, max: 15, def: 5 },
    pin: { min: 4, max: 12, def: 6 },
  };
  const WORD_SEPARATORS = ['-', '.', '_', ' '];
  const DEFAULTS = { mode: 'random', length: 20, digits: true, symbols: true, separator: '-', capitalize: false };

  function mode(m) { return Object.prototype.hasOwnProperty.call(MODES, m) ? m : 'random'; }

  function clampLength(m, n) {
    const lim = MODES[mode(m)];
    const v = Math.round(Number(n));
    return Number.isFinite(v) ? Math.min(lim.max, Math.max(lim.min, v)) : lim.def;
  }

  /** Stored settings only: mode/length/toggles/separator/capitalize. The generated value is never part of this shape. */
  function safeSettings(s) {
    const m = mode(s && s.mode);
    const sep = s && WORD_SEPARATORS.includes(s.separator) ? s.separator : '-';
    return {
      mode: m,
      length: clampLength(m, s && s.length),
      digits: !s || s.digits !== false,
      symbols: !s || s.symbols !== false,
      separator: sep,
      capitalize: Boolean(s && s.capitalize),
    };
  }

  /** EFF large wordlist size. Per-word entropy is log2(7776) ≈ 12.9 bits. */
  const EFF_WORDLIST_SIZE = 7776;

  /**
   * Rough 0-100 meter score for display only.
   * `words` (memorable mode) scores n * log2(7776), matching the desktop generator.
   * Character-pool entropy would overstate a passphrase. Omit `words` for random and PIN.
   */
  function strengthScore(pw, words) {
    if (!pw) return 0;
    const n = Number(words);
    let score;
    if (Number.isInteger(n) && n >= 3 && n <= 20) {
      score = Math.min(100, Math.round((n * Math.log2(EFF_WORDLIST_SIZE) * 100) / 128));
    } else {
      let pool = 0;
      if (/[a-z]/.test(pw)) pool += 26;
      if (/[A-Z]/.test(pw)) pool += 26;
      if (/[0-9]/.test(pw)) pool += 10;
      if (/[^a-zA-Z0-9]/.test(pw)) pool += 33;
      score = Math.min(100, Math.round((pw.length * Math.log2(Math.max(pool, 2))) / 128 * 100));
    }
    if (new Set(pw).size <= 2) score = Math.min(score, 10);
    return score;
  }

  function strengthKey(score) { return score < 40 ? 'weak' : score < 70 ? 'fair' : score < 90 ? 'good' : 'excellent'; }

  /** Recent list (issue #226): masked value = bullets plus the last 4 characters; short values stay fully masked. */
  const BULLETS = '\u2022'.repeat(8);
  function maskValue(v) {
    const s = String(v == null ? '' : v);
    return s.length > 8 ? BULLETS + s.slice(-4) : BULLETS;
  }

  /** Relative age as {key, n}: key is a locale key (genAgoNow/Min/Hour/Day), n the count. Clock skew counts as "now". */
  function ageParts(createdAtSecs, nowSecs) {
    const d = Math.max(0, Math.floor(Number(nowSecs) - Number(createdAtSecs)) || 0);
    if (d < 60) return { key: 'genAgoNow', n: 0 };
    if (d < 3600) return { key: 'genAgoMin', n: Math.floor(d / 60) };
    if (d < 86400) return { key: 'genAgoHour', n: Math.floor(d / 3600) };
    return { key: 'genAgoDay', n: Math.floor(d / 86400) };
  }

  /** Fill is offered only when the entry's recorded host is the active tab's host (a leading www. is ignored). */
  function hostMatches(entryHost, tabOrigin) {
    if (!entryHost || !tabOrigin) return false;
    let h;
    try { h = new URL(tabOrigin).hostname; } catch (_) { return false; }
    const norm = (x) => String(x).toLowerCase().replace(/^www\./, '');
    return norm(h) === norm(entryHost);
  }

  /** Normalise the host reply: at most `limit` entries with string values, newest first. */
  function cleanHistory(r, limit) {
    const enabled = !r || r.enabled !== false;
    const list = Array.isArray(r && r.entries) ? r.entries.filter((e) => e && typeof e.value === 'string' && e.id != null) : [];
    list.sort((a, b) => (Number(b.created_at) || 0) - (Number(a.created_at) || 0));
    return { enabled, entries: enabled ? list.slice(0, limit || 10) : [] };
  }

  const api = { MODES, DEFAULTS, WORD_SEPARATORS, mode, clampLength, safeSettings, strengthScore, strengthKey, maskValue, ageParts, hostMatches, cleanHistory };
  root.GitpassGenerator = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
