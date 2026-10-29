// Site password rules for the "Suggest strong password" row (#188). Pure logic, no extension APIs, so node
// tests can load it. The content script reads `passwordrules` (Apple's syntax) and `minlength` / `maxlength`
// from the field and sends only the parsed constraints (never page HTML); the background validates and
// clamps them with `sanitize` before the host generates anything.
(function (root) {
  'use strict';

  const MIN_LEN = 8, MAX_LEN = 64, DEFAULT_LEN = 20, MAX_ENTRIES = 16, MAX_CHARS = 32;
  const KINDS = ['lower', 'upper', 'digit', 'special'];

  /** Keep printable ASCII symbols only (no letters, digits or spaces), de-duplicated, at most 32. */
  function cleanSymbols(s) {
    if (typeof s !== 'string') return '';
    const seen = new Set();
    for (const ch of s) if (/^[\x21-\x2f\x3a-\x40\x5b-\x60\x7b-\x7e]$/.test(ch)) seen.add(ch);
    return Array.from(seen).slice(0, MAX_CHARS).join('');
  }

  function toLen(v) {
    const n = typeof v === 'string' ? parseInt(v, 10) : v;
    return Number.isInteger(n) && n >= 0 && n <= 1024 ? n : undefined;
  }

  /**
   * Parse Apple's `passwordrules` syntax. Returns { minlength?, maxlength?, entries: [{ kind, required, chars? }] };
   * malformed or unknown parts are ignored. `ascii-printable` / `unicode` allow every class; a custom `[...]`
   * set becomes a `special` entry limited to its symbol characters.
   */
  function parse(str) {
    const out = { entries: [] };
    if (typeof str !== 'string') return out;
    for (const part of str.slice(0, 1024).split(';')) {
      const i = part.indexOf(':');
      if (i < 0) continue;
      const key = part.slice(0, i).trim().toLowerCase();
      const val = part.slice(i + 1).trim();
      if (key === 'minlength' || key === 'maxlength') {
        const n = toLen(val);
        if (n !== undefined) out[key] = n;
      } else if (key === 'required' || key === 'allowed') {
        const required = key === 'required';
        for (const tok of val.match(/\[[^\]]*\]|[A-Za-z-]+/g) || []) {
          const name = tok.toLowerCase();
          if (KINDS.includes(name)) out.entries.push({ kind: name, required });
          else if (name === 'ascii-printable' || name === 'unicode') for (const k of KINDS) out.entries.push({ kind: k, required: false });
          else if (tok[0] === '[') {
            const chars = cleanSymbols(tok.slice(1, -1));
            if (chars) out.entries.push({ kind: 'special', required, chars });
          }
          if (out.entries.length >= MAX_ENTRIES) return out;
        }
      }
    }
    return out;
  }

  /**
   * Rules for a new-password field: its `passwordrules` (then the form's) plus its own `minlength` /
   * `maxlength`. Returns null when the page states nothing (the default 20 characters apply).
   */
  function fromField(field) {
    if (!field || typeof field.getAttribute !== 'function') return null;
    const attr = field.getAttribute('passwordrules') || (field.form && field.form.getAttribute && field.form.getAttribute('passwordrules')) || '';
    const rules = parse(attr);
    const min = toLen(field.getAttribute('minlength')), max = toLen(field.getAttribute('maxlength'));
    if (min !== undefined && (rules.minlength === undefined || min > rules.minlength)) rules.minlength = min;
    if (max !== undefined && max > 0 && (rules.maxlength === undefined || max < rules.maxlength)) rules.maxlength = max;
    if (rules.minlength === undefined && rules.maxlength === undefined && !rules.entries.length) return null;
    return rules;
  }

  /**
   * Validate and clamp untrusted constraints. Returns { ok: true, len, classes } where classes is the host's
   * `rules.classes` list, or { ok: false, reason: 'too_short' | 'unsatisfiable' } when no strong password fits.
   * With no usable rules the default applies: 20 characters, letters plus required digits and symbols.
   */
  function sanitize(raw) {
    const r = raw && typeof raw === 'object' ? raw : {};
    const min = toLen(r.minlength), max = toLen(r.maxlength);
    const lo = Math.max(MIN_LEN, min === undefined ? 0 : min);
    const hi = Math.min(MAX_LEN, max === undefined || max === 0 ? MAX_LEN : max);
    if (hi < MIN_LEN) return { ok: false, reason: 'too_short' };
    if (lo > hi) return { ok: false, reason: 'unsatisfiable' };
    const len = Math.min(hi, Math.max(lo, DEFAULT_LEN));

    const seen = new Set();
    let classes = [];
    for (const e of (Array.isArray(r.entries) ? r.entries : []).slice(0, MAX_ENTRIES)) {
      if (!e || !KINDS.includes(e.kind)) continue;
      const c = { kind: e.kind, required: e.required === true };
      if (e.kind === 'special' && e.chars !== undefined) {
        const chars = cleanSymbols(e.chars);
        if (!chars) continue;
        c.chars = chars;
      }
      const key = c.kind + '|' + (c.chars || '') + '|' + c.required;
      if (!seen.has(key)) { seen.add(key); classes.push(c); }
    }
    if (!classes.length) {
      classes = [{ kind: 'lower', required: false }, { kind: 'upper', required: false },
        { kind: 'digit', required: true }, { kind: 'special', required: true }];
    } else if (!classes.some((c) => !c.required)) {
      // Only `required:` lines: Apple's default for `allowed` is every printable ASCII class.
      for (const k of KINDS) if (!classes.some((c) => c.kind === k && !c.chars)) classes.push({ kind: k, required: false });
    }
    if (classes.filter((c) => c.required).length > len) return { ok: false, reason: 'unsatisfiable' };
    return { ok: true, len, classes };
  }

  /** Short facts for the row subtitle: { len, noSymbols, limitedSymbols } from a sanitized result. */
  function summary(s) {
    if (!s || !s.ok) return null;
    const specials = s.classes.filter((c) => c.kind === 'special');
    return {
      len: s.len,
      noSymbols: !specials.length,
      limitedSymbols: specials.length > 0 && specials.every((c) => c.chars),
    };
  }

  const api = { parse, fromField, sanitize, summary, cleanSymbols, MIN_LEN, MAX_LEN, DEFAULT_LEN };
  root.GitpassPasswordRules = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
