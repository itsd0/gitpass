// Shared, pure policy helpers (used by background, content script and node tests).
(function (root) {
  'use strict';
  const LOOPBACK = new Set(['localhost', '127.0.0.1', '[::1]']);

  /** True if a page origin may be filled at all: https, or http on loopback. */
  function isFillableOrigin(origin) {
    let u;
    try { u = new URL(origin); } catch { return false; }
    if (u.protocol === 'https:') return !!u.hostname;
    if (u.protocol === 'http:') return LOOPBACK.has(u.hostname) || u.hostname.endsWith('.localhost');
    return false;
  }

  /** Origin string (scheme://host[:port]) of a URL, or null. */
  function originOf(url) {
    try { const u = new URL(url); return u.origin === 'null' ? null : u.origin; } catch { return null; }
  }

  function isVisible(el) {
    if (!el || el.disabled || el.readOnly) return false;
    const r = el.getBoundingClientRect ? el.getBoundingClientRect() : { width: 1, height: 1 };
    return r.width > 0 && r.height > 0;
  }

  /** Find login fields: [{password, username|null}] for visible password inputs. */
  function findLoginFields(doc) {
    const out = [];
    const pws = Array.from(doc.querySelectorAll('input[type="password"]')).filter(isVisible);
    for (const pw of pws) {
      const scope = pw.form || doc;
      const inputs = Array.from(scope.querySelectorAll('input')).filter(isVisible);
      const idx = inputs.indexOf(pw);
      let user = null;
      for (let i = idx - 1; i >= 0; i--) {
        const t = (inputs[i].type || 'text').toLowerCase();
        if (['text', 'email', 'tel', ''].includes(t)) { user = inputs[i]; break; }
      }
      out.push({ password: pw, username: user });
    }
    return out;
  }

  /** Rough "same site" check (last two host labels, scheme must match). Used only to carry a pending
   *  save-login prompt across a post-login redirect (e.g. login.example.com -> www.example.com); never for fills. */
  function sameSite(a, b) {
    try {
      const x = new URL(a), y = new URL(b);
      if (x.protocol !== y.protocol) return false;
      const tail = (h) => h.split('.').slice(-2).join('.');
      return x.hostname === y.hostname || tail(x.hostname) === tail(y.hostname);
    } catch { return false; }
  }

  const api = { isFillableOrigin, originOf, findLoginFields, isVisible, sameSite };
  root.GitpassPolicy = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
