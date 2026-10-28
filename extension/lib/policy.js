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

  const MULTI_PART_TLDS = new Set([
    'co.uk', 'org.uk', 'gov.uk', 'ac.uk', 'net.uk', 'me.uk',
    'com.au', 'net.au', 'org.au', 'edu.au', 'gov.au',
    'co.jp', 'ne.jp', 'or.jp', 'go.jp', 'ac.jp',
    'co.nz', 'net.nz', 'org.nz', 'govt.nz',
    'com.br', 'net.br', 'org.br',
    'com.sg', 'edu.sg', 'gov.sg',
    'co.za', 'org.za',
    'com.tw', 'org.tw', 'gov.tw',
    'co.in', 'net.in', 'org.in', 'gen.in', 'firm.in', 'ind.in',
    'com.cn', 'net.cn', 'org.cn', 'gov.cn',
    'co.kr', 'ne.kr', 'or.kr', 're.kr',
  ]);

  /** Registrable domain (eTLD+1) of a URL, origin or hostname string.
   *  Normalises subdomains: login.example.com -> example.com.
   *  Loopback hosts (localhost, 127.0.0.1, [::1]) are returned as-is. */
  function registrableDomain(urlOrHost) {
    if (!urlOrHost) return '';
    let host = '';
    try {
      const u = new URL(urlOrHost.includes('://') ? urlOrHost : 'https://' + urlOrHost);
      host = (u.hostname || '').toLowerCase();
    } catch (_) {
      host = String(urlOrHost).toLowerCase().trim().split('/')[0].split(':')[0];
    }
    if (!host) return '';
    if (LOOPBACK.has(host) || host.endsWith('.localhost')) return host;
    if (/^\d{1,3}(\.\d{1,3}){3}$/.test(host) || host.includes(':')) return host;
    const parts = host.split('.');
    if (parts.length <= 2) return host;
    const lastTwo = parts.slice(-2).join('.');
    if (MULTI_PART_TLDS.has(lastTwo) && parts.length >= 3) {
      return parts.slice(-3).join('.');
    }
    const tld = parts[parts.length - 1];
    const sld = parts[parts.length - 2];
    if (tld.length === 2 && ['co', 'com', 'org', 'net', 'edu', 'gov'].includes(sld) && parts.length >= 3) {
      return parts.slice(-3).join('.');
    }
    return parts.slice(-2).join('.');
  }

  /** Check if a URL or origin's registrable domain is in the neverSave list. */
  function isNeverSave(urlOrOrigin, list) {
    if (!list || !Array.isArray(list) || !list.length) return false;
    const dom = registrableDomain(urlOrOrigin);
    if (!dom) return false;
    const lower = dom.toLowerCase();
    return list.some((item) => (item || '').trim().toLowerCase() === lower);
  }

  /** Rough "same site" check (registrable domain, scheme must match). Used only to carry a pending
   *  save-login prompt across a post-login redirect (e.g. login.example.com -> www.example.com); never for fills. */
  function sameSite(a, b) {
    try {
      const x = new URL(a), y = new URL(b);
      if (x.protocol !== y.protocol) return false;
      const domA = registrableDomain(x.hostname);
      const domB = registrableDomain(y.hostname);
      return domA === domB || x.hostname === y.hostname;
    } catch { return false; }
  }

  /** Hosted card-field iframes of payment processors (Stripe Elements, Braintree Hosted Fields, Adyen Secured
   *  Fields). A card picked in the TOP frame may be filled into these child frames only: they are where the
   *  page is sending the card anyway. Any other cross-origin frame never receives card data. */
  const PAYMENT_FRAME_HOSTS = ['js.stripe.com', 'assets.braintreegateway.com', 'checkoutshopper-live.adyen.com', 'checkoutshopper-test.adyen.com'];
  function isPaymentFrameOrigin(origin) {
    try { const u = new URL(origin); return u.protocol === 'https:' && !u.port && PAYMENT_FRAME_HOSTS.includes(u.hostname); } catch (_) { return false; }
  }
  const api = { isFillableOrigin, originOf, findLoginFields, isVisible, sameSite, registrableDomain, isNeverSave, isPaymentFrameOrigin, PAYMENT_FRAME_HOSTS };
  root.GitpassPolicy = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
