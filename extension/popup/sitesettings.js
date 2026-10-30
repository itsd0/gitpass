// Per-site settings shared by the popup "Site settings" panel, the options page and the
// content script (issue #341). Lists of registrable domains live in chrome.storage.local:
//   neverSave        - existing: never offer to save logins on the site
//   neverAutoSubmit  - existing: per-site exception to the global submit-after-fill opt-in
//   hiddenInlineMenu - new: do not attach the inline menu / field badges on the site
// Pure module; every helper also works with the popup mocks used by tests and popup.html.
'use strict';
(function (root) {
  const P = typeof GitpassPolicy !== 'undefined' ? GitpassPolicy : (typeof require !== 'undefined' ? require('../lib/policy.js') : null);

  /** Registrable domain of a URL, origin or hostname; '' when there is no web origin. */
  function domainOf(input) {
    const s = (input || '').trim();
    if (!s) return '';
    if (P && P.registrableDomain) return P.registrableDomain(s) || '';
    return s.replace(/^[a-z]+:\/\//i, '').replace(/^www\./, '').split('/')[0].toLowerCase();
  }

  const hasChromeStorage = () => {
    try { return !!(globalThis.chrome && chrome.storage && chrome.storage.local); } catch (_) { return false; }
  };

  function storageGet(key, mockVal) {
    return new Promise((res) => {
      if (!hasChromeStorage()) { res(Array.isArray(mockVal) ? mockVal.slice() : []); return; }
      try {
        chrome.storage.local.get(key, (r) => {
          const v = r && r[key];
          res(Array.isArray(v) ? v.slice() : []);
        });
      } catch (_) { res([]); }
    });
  }

  function storageSet(key, list, mockName) {
    return new Promise((res) => {
      if (!hasChromeStorage()) {
        if (mockName) root[mockName] = list.slice();
        res(list.slice());
        return;
      }
      try {
        chrome.storage.local.set({ [key]: list.slice() }, () => res(list.slice()));
      } catch (_) { res(list.slice()); }
    });
  }

  const has = (list, domain) => list.includes(domainOf(domain));

  async function setMembership(key, mockName, domain, member) {
    const dom = domainOf(domain);
    if (!dom) return [];
    const cur = await storageGet(key, mockName ? root[mockName] : undefined);
    const next = member ? Array.from(new Set([...cur, dom])) : cur.filter((d) => d !== dom);
    return storageSet(key, next, mockName);
  }

  const api = {
    domainOf,
    has,
    // Save prompts (neverSave uses the legacy pair of keys read by refreshNeverSave()).
    getNeverSave: () => storageGet('neverSave', root.__gitpassMockNeverSave),
    setSavePrompts: (domain, offer) => setMembership('neverSave', '__gitpassMockNeverSave', domain, !offer),
    // Submit-after-fill per-site exception (neverAutoSubmit; global opt-in lives in settings).
    getNeverAutoSubmit: () => storageGet('neverAutoSubmit', root.__gitpassMockNeverAutoSubmit),
    setAutoSubmit: (domain, allow) => setMembership('neverAutoSubmit', '__gitpassMockNeverAutoSubmit', domain, !allow),
    // Hidden inline menu / field badges (new list).
    getHiddenInlineMenu: () => storageGet('hiddenInlineMenu', root.__gitpassMockHiddenInlineMenu),
    setInlineMenuShown: (domain, shown) => setMembership('hiddenInlineMenu', '__gitpassMockHiddenInlineMenu', domain, !shown),
  };
  root.GitpassSiteSettings = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
