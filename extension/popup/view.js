// Pure view logic for the popup (no DOM, no chrome.*) so it can be unit-tested in node.
(function (root) {
  'use strict';
  const REQUEST_TIMEOUT_MS = 8000;
  const SKELETON_DELAY_MS = 150;

  /** Wrap a promise in a timeout; rejects with code: 'timeout' if ms expires. */
  function withTimeout(promise, timeoutMs = REQUEST_TIMEOUT_MS, timeoutMsg) {
    if (!timeoutMs || timeoutMs <= 0) return Promise.resolve(promise);
    return new Promise((res, rej) => {
      let settled = false;
      const timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        const err = new Error(timeoutMsg || 'Request timed out.');
        err.code = 'timeout';
        rej(err);
      }, timeoutMs);
      Promise.resolve(promise).then(
        (val) => { if (!settled) { settled = true; clearTimeout(timer); res(val); } },
        (err) => { if (!settled) { settled = true; clearTimeout(timer); rej(err); } }
      );
    });
  }

  const CATEGORIES = ['login', 'secure_note', 'credit_card', 'identity', 'api_credential', 'document', 'ssh_key', 'wifi', 'passkey'];
  const CAT_LABEL = {
    login: 'Logins', secure_note: 'Secure Notes', credit_card: 'Credit Cards', identity: 'Identities',
    api_credential: 'API Credentials', document: 'Documents', ssh_key: 'SSH Keys', wifi: 'Wi-Fi', passkey: 'Passkeys',
  };
  const CAT_KEY = {
    login: 'catLogin', secure_note: 'catNote', credit_card: 'catCard', identity: 'catIdentity',
    api_credential: 'catApi', document: 'catDocument', ssh_key: 'catSsh', wifi: 'catWifi', passkey: 'catPasskey',
  };

  /** Filter dropdown entries: suggestions, all, favorites, one per category, one per vault. */
  function buildFilters(vaults) {
    const f = [
      { id: 'suggested', key: 'filterSuggested', label: 'Suggestions for this site' },
      { id: 'all', key: 'filterAll', label: 'All Items' },
      { id: 'favorites', key: 'filterFavorites', label: 'Favorites' },
    ];
    for (const c of CATEGORIES) f.push({ id: 'cat:' + c, key: CAT_KEY[c], label: CAT_LABEL[c], group: 'category' });
    for (const v of vaults || []) f.push({ id: 'vault:' + v.id, label: v.name, group: 'vault', count: v.item_count });
    return f;
  }

  function matchesQuery(it, q) {
    if (!q) return true;
    const s = q.toLowerCase();
    return [it.title, it.subtitle, it.url, it.vault_name].some((x) => x && String(x).toLowerCase().includes(s));
  }

  /** Apply filter + query. `suggested` is the list returned by match for the active tab. */
  function filterItems(all, suggested, filter, query) {
    let base;
    if (filter === 'suggested') base = suggested || [];
    else if (filter === 'favorites') base = all.filter((i) => i.favorite);
    else if (filter && filter.startsWith('cat:')) base = all.filter((i) => i.category === filter.slice(4));
    else if (filter && filter.startsWith('vault:')) base = all.filter((i) => i.vault_id === filter.slice(6));
    else base = all;
    // Searching from Suggestions searches everything, like 1Password.
    if (query && filter === 'suggested') base = all;
    return base.filter((i) => matchesQuery(i, query))
      .sort((a, b) => (b.favorite - a.favorite) || String(a.title).localeCompare(String(b.title)));
  }

  /** Which empty state to show when the list has no rows. ctx: { query, filter, origin, count, excepted } */
  function emptyState(ctx = {}) {
    const q = String(ctx.query || '').trim();
    if (q) return { kind: 'search', query: q };
    if (ctx.filter === 'suggested') {
      const host = hostOfUrl(ctx.origin);
      if (!host) return { kind: 'nofill' };
      const out = { kind: 'site', host };
      if (ctx.excepted) out.excepted = true;
      return out;
    }
    if (ctx.filter === 'all' && !ctx.count) return { kind: 'vault' };
    return { kind: 'none' };
  }

  /** Fixed-row-height virtual window. */
  function virtualWindow(scrollTop, viewport, rowH, count, overscan = 4) {
    const start = Math.max(0, Math.floor(scrollTop / rowH) - overscan);
    const end = Math.min(count, Math.ceil((scrollTop + viewport) / rowH) + overscan);
    return { start, end, padTop: start * rowH, padBottom: Math.max(0, (count - end) * rowH) };
  }

  /** scrollTop needed to make row `idx` visible. */
  function scrollToRow(idx, scrollTop, viewport, rowH) {
    const top = idx * rowH, bot = top + rowH;
    if (top < scrollTop) return top;
    if (bot > scrollTop + viewport) return bot - viewport;
    return scrollTop;
  }

  function moveSel(idx, delta, count) {
    if (!count) return -1;
    if (idx < 0) return delta > 0 ? 0 : count - 1;
    return Math.min(count - 1, Math.max(0, idx + delta));
  }

  /** Map a keydown to an action. ctx: {inEditable, hasSelection, panel} */
  function keyAction(e, ctx = {}) {
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === 'Escape') return ctx.panel ? 'closePanel' : 'close';
    if (ctx.panel) return null;
    if (e.key === 'ArrowDown') return 'down';
    if (e.key === 'ArrowUp') return 'up';
    if (e.key === 'Enter' && !mod && !ctx.inForm) return 'fill';
    if (mod && e.shiftKey && (e.key === 'c' || e.key === 'C') && !ctx.hasSelection) return ctx.hasTotp ? 'copyTotp' : 'copyPassword';
    if (mod && (e.key === 'c' || e.key === 'C') && !ctx.hasSelection) return 'copyPassword';
    return null;
  }

  /** TOTP ring: stroke-dashoffset for an SVG circle of circumference C. */
  function ringOffset(remaining, period, C) {
    const p = Math.max(0, Math.min(1, remaining / (period || 30)));
    return +(C * (1 - p)).toFixed(3);
  }

  function formatValue(kind, value) {
    if (value == null) return '';
    const v = String(value);
    if (kind === 'totp') return (v.length === 6 || v.length === 8) ? v.slice(0, v.length / 2) + ' ' + v.slice(v.length / 2) : v;
    if (kind === 'card_number') return v.replace(/\s+/g, '').replace(/(.{4})(?=.)/g, '$1 ');
    return v;
  }

  /** Card numbers show the last 4 the host computed after decryption (f.masked); everything else is fully masked. */
  function mask(kind, masked) { return kind === 'card_number' ? (masked || '•••• •••• •••• ••••') : '••••••••••'; }

  /** Which fill command applies to a category (or null if not fillable). */
  function fillKind(category) {
    if (category === 'login') return 'login';
    if (category === 'credit_card') return 'card';
    if (category === 'identity') return 'identity';
    return null;
  }

  /** The field Cmd+C copies: password for logins, number for cards, else first concealed field. */
  function primarySecretField(detail) {
    const fs = (detail && detail.fields) || [];
    return fs.find((f) => f.kind === 'password') || fs.find((f) => f.kind === 'card_number') || fs.find((f) => f.concealed) || null;
  }

  /** Decide the top-level view from a status result or error. */
  function viewFor(status, err) {
    if (err) {
      const m = String((err && (err.code || err.message)) || err);
      if (/app_not_running/.test(m)) return 'appdown';
      return 'nohost';
    }
    if (!status) return 'nohost';
    if (status.unlocked) return 'unlocked';
    return status.app_running === false ? 'appdown' : 'locked';
  }

  function hostOfUrl(u) { try { return new URL(u).hostname.replace(/^www\./, ''); } catch (_) { return ''; } }

  const api = { CATEGORIES, CAT_LABEL, CAT_KEY, REQUEST_TIMEOUT_MS, SKELETON_DELAY_MS, withTimeout, buildFilters, filterItems, emptyState, matchesQuery, virtualWindow, scrollToRow, moveSel, keyAction, ringOffset, formatValue, mask, fillKind, primarySecretField, viewFor, hostOfUrl };
  root.GitpassView = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
