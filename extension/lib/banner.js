// Pure helpers for the in-page save/update banner (content.js). No DOM and no chrome.* here so the
// title/vault rules and the Tab/Esc decisions can be unit tested in jsdom.
(function (root) {
  'use strict';
  const MAX_TITLE = 200;

  /** Trimmed title from page-side input; anything that is not a string becomes ''. */
  function titleValue(v) {
    return typeof v === 'string' ? v.trim() : '';
  }

  /** '' | 'required' | 'too_long' — the banner shows a message and the background re-checks. */
  function titleError(v) {
    const s = titleValue(v);
    if (!s) return 'required';
    if (s.length > MAX_TITLE) return 'too_long';
    return '';
  }

  /** Title passed to page:save_decision: trimmed, capped, or the hostname fallback. */
  function titleForSave(v, fallback) {
    return titleValue(v).slice(0, MAX_TITLE) || fallback || '';
  }

  /** Only id/name are ever rendered or sent back; ignore anything else the host may add. */
  function vaultList(vaults) {
    return (Array.isArray(vaults) ? vaults : []).filter((v) => v && typeof v.id === 'string' && v.id && typeof v.name === 'string' && v.name)
      .map((v) => ({ id: v.id, name: v.name }));
  }

  /** The chosen vault if it is still in the list, else the first (host order puts the default first). */
  function selectVault(vaults, id) {
    const list = vaultList(vaults);
    if (!list.length) return '';
    if (id && list.some((v) => v.id === id)) return id;
    return list[0].id;
  }

  /** Only id/title/username are ever rendered or sent back; ignore anything else. */
  function accountList(accounts) {
    return (Array.isArray(accounts) ? accounts : [])
      .filter((a) => a && typeof a.id === 'string' && a.id)
      .map((a) => ({
        id: a.id,
        title: typeof a.title === 'string' ? a.title : '',
        username: typeof a.username === 'string' ? a.username : '',
      }));
  }

  /** The chosen account if it is in the list, else fallbackId if in list, else the first. */
  function selectAccount(accounts, selectedId, fallbackId) {
    const list = accountList(accounts);
    if (!list.length) return '';
    if (selectedId && list.some((a) => a.id === selectedId)) return selectedId;
    if (fallbackId && list.some((a) => a.id === fallbackId)) return fallbackId;
    return list[0].id;
  }

  /** Format account option text: "title · username" or "title · (no username)" */
  function formatAccountOption(account, fallbackTitle, noUserText) {
    const title = (account && account.title ? account.title : fallbackTitle) || '';
    const user = (account && account.username ? account.username : noUserText) || '';
    return title && user ? `${title} · ${user}` : title || user;
  }

  /** Focus target for Tab / Shift+Tab at the edges of the banner; null lets the browser move normally. */
  function trapTab(key, shift, index, count) {
    if (key !== 'Tab' || count < 1) return null;
    if (index < 0) return 0;
    if (shift) return index === 0 ? count - 1 : null;
    return index === count - 1 ? 0 : null;
  }

  function isEscape(key) {
    return key === 'Escape';
  }

  // ---------- save-card helpers (issue #196) ----------
  // Pure card helpers shared by the content script (banner text) and the background
  // (capture sanitising). The CVV/CVC is never an input here: sanitizeCardCapture
  // builds a fresh object with exactly { number, name, expiry }, so a capture message
  // carrying a CVV — or any other stray field — cannot smuggle it into the vault.

  /** Digits only (spaces and dashes stripped); anything else becomes ''. */
  function cardDigits(s) {
    return String(s == null ? '' : s).replace(/[\s-]/g, '');
  }

  /** Luhn check on a digits-only string. */
  function cardLuhn(n) {
    if (!/^\d{12,19}$/.test(n || '')) return false;
    let sum = 0, alt = false;
    for (let i = n.length - 1; i >= 0; i--) { let d = n.charCodeAt(i) - 48; if (alt) { d *= 2; if (d > 9) d -= 9; } sum += d; alt = !alt; }
    return sum % 10 === 0;
  }

  /** Card brand from the number prefix, '' when unknown. */
  function cardBrand(n) {
    const d = cardDigits(n);
    if (/^4/.test(d)) return 'Visa';
    if (/^3[47]/.test(d)) return 'Amex';
    if (/^(5[1-5]|222[1-9]|22[3-9]\d|2[3-6]\d\d|27[01]\d|2720)/.test(d)) return 'Mastercard';
    if (/^(6011|65|64[4-9])/.test(d)) return 'Discover';
    return '';
  }

  function cardLast4(n) {
    const d = cardDigits(n);
    return d.length >= 4 ? d.slice(-4) : '';
  }

  /** '•••• 4242' — the only card-number rendering the banner ever shows. */
  function cardMasked(n) {
    const last4 = cardLast4(n);
    return last4 ? '•••• ' + last4 : '';
  }

  /** Default banner title: 'Visa •••• 4242', or '•••• 4242' for an unknown brand. */
  function cardTitleDefault(n) {
    const masked = cardMasked(n);
    if (!masked) return '';
    const brand = cardBrand(n);
    return brand ? brand + ' ' + masked : masked;
  }

  const CARD_MONTHS = { jan: 1, january: 1, feb: 2, february: 2, mar: 3, march: 3, apr: 4, april: 4,
    may: 5, jun: 6, june: 6, jul: 7, july: 7, aug: 8, august: 8, sep: 9, sept: 9, september: 9,
    oct: 10, october: 10, nov: 11, november: 11, dec: 12, december: 12 };

  /** 'MM/YY', 'MM/YYYY', 'MMYY', 'MM-YY' and 'MM YY' -> 'MM/YY'; anything else -> ''. */
  function normalizeCardExpiry(v) {
    if (v == null) return '';
    const s = String(v).trim();
    const m = /^(\d{1,2})\s*[/\-. ]?\s*(\d{2}|\d{4})$/.exec(s);
    if (!m) return '';
    const mm = Number(m[1]);
    if (mm < 1 || mm > 12) return '';
    return String(mm).padStart(2, '0') + '/' + m[2].slice(-2);
  }

  /** Separate month/year fields (e.g. two <select>s) -> 'MM/YY'; '' when unusable. */
  function normalizeCardExpiryParts(month, year) {
    if (month == null || year == null) return '';
    let mm = null;
    const ms = String(month).trim().toLowerCase();
    if (/^\d{1,2}$/.test(ms)) { const n = Number(ms); if (n >= 1 && n <= 12) mm = n; }
    else if (CARD_MONTHS[ms]) mm = CARD_MONTHS[ms];
    if (mm == null) return '';
    const ys = String(year).trim();
    if (!/^\d{2}$/.test(ys) && !/^\d{4}$/.test(ys)) return '';
    return String(mm).padStart(2, '0') + '/' + ys.slice(-2);
  }

  /**
   * Validate and sanitise a page:capture card message. Returns null unless the
   * number is digits-only, 12–19 digits and Luhn-valid. The result carries only
   * number/name/expiry: a CVV in the message is dropped, never stored or logged.
   */
  function sanitizeCardCapture(a) {
    if (!a || typeof a !== 'object') return null;
    const number = cardDigits(a.number);
    if (!cardLuhn(number)) return null;
    const name = typeof a.name === 'string' ? a.name.trim().slice(0, 200) : '';
    const expiry = normalizeCardExpiry(a.expiry);
    return { number, name, expiry };
  }

  const api = { MAX_TITLE, titleValue, titleError, titleForSave, vaultList, selectVault,
    accountList, selectAccount, formatAccountOption, trapTab, isEscape,
    cardDigits, cardLuhn, cardBrand, cardLast4, cardMasked, cardTitleDefault,
    normalizeCardExpiry, normalizeCardExpiryParts, sanitizeCardCapture };
  root.GitpassBanner = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
