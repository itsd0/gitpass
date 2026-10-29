// Pure view logic for the popup (no DOM, no chrome.*) so it can be unit-tested in node.
(function (root) {
  'use strict';
  const REQUEST_TIMEOUT_MS = 8000;
  const SKELETON_DELAY_MS = 150;
  const UNLOCK_TIMEOUT_MS = 130000; // >= 130 s, exceeds bridge.rs UNLOCK_TIMEOUT (125 s)

  /** Determine the timeout for a message: user-interactive calls (Touch ID / prompt, app launch) get at least 130 s. */
  function timeoutForMessage(msg, explicitMs) {
    if (typeof explicitMs === "number") return explicitMs;
    if (msg && typeof msg.__timeoutMs === "number") return msg.__timeoutMs;
    const type = msg && msg.type;
    if (type === "popup:request_unlock" || type === "popup:open_in_app" || type === "popup:open_fill") {
      return UNLOCK_TIMEOUT_MS;
    }
    return REQUEST_TIMEOUT_MS;
  }

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
  const SORTS = ['updated', 'created', 'title'];
  function timeOf(v) { if (typeof v === 'number') return v; const n = Date.parse(v); return Number.isNaN(n) ? 0 : n; }
  /** Comparator for the sort menu: favorites first, then newest edit / newest creation / title A-Z. */
  function compareBy(sort) {
    const byTitle = (a, b) => String(a.title).localeCompare(String(b.title));
    const key = sort === 'created' ? 'created_at' : sort === 'updated' ? 'updated_at' : null;
    return (a, b) => (b.favorite - a.favorite) || (key ? (timeOf(b[key]) - timeOf(a[key])) : 0) || byTitle(a, b);
  }

  /** Apply filter + query, then sort (`sort`: updated | created | title; default title). `suggested` is the list returned by match for the active tab. */
  function filterItems(all, suggested, filter, query, sort = 'title') {
    let base;
    if (filter === 'suggested') base = suggested || [];
    else if (filter === 'favorites') base = all.filter((i) => i.favorite);
    else if (filter && filter.startsWith('cat:')) base = all.filter((i) => i.category === filter.slice(4));
    else if (filter && filter.startsWith('vault:')) base = all.filter((i) => i.vault_id === filter.slice(6));
    else base = all;
    // Searching from Suggestions searches everything, like 1Password.
    if (query && filter === 'suggested') base = all;
    return base.filter((i) => matchesQuery(i, query))
      .sort(compareBy(SORTS.includes(sort) ? sort : 'title'));
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

  /** Map a keydown to an action. ctx: {inEditable, inForm, hasSelection, panel, query, hasTotp} */
  function keyAction(e, ctx = {}) {
    const mod = Boolean(e.metaKey || e.ctrlKey);
    if (e.key === 'Escape') {
      if (ctx.panel) return 'closePanel';
      if (ctx.query) return 'clearSearch';
      return 'close';
    }
    if (e.key === '?' && !mod && !ctx.inEditable && !ctx.hasSelection && (!ctx.panel || ctx.panel === 'shortcuts')) {
      return 'toggleShortcuts';
    }
    if (ctx.panel) return null;
    if (e.key === 'ArrowDown') return 'down';
    if (e.key === 'ArrowUp') return 'up';
    if (e.key === 'Enter' && !mod && !ctx.inForm) return 'fill';
    if (mod && e.shiftKey && (e.key === 'u' || e.key === 'U') && !ctx.hasSelection) return 'copyUsername';
    if (mod && e.shiftKey && (e.key === 'c' || e.key === 'C') && !ctx.hasSelection) return ctx.hasTotp ? 'copyTotp' : 'copyPassword';
    if (mod && (e.key === 'c' || e.key === 'C') && !ctx.hasSelection) return 'copyPassword';
    if (mod && !e.shiftKey && (e.key === 'o' || e.key === 'O') && !ctx.hasSelection) return 'openWebsite';
    if (mod && e.shiftKey && (e.key === 'g' || e.key === 'G') && !ctx.hasSelection) return 'generator';
    return null;
  }

  /** Return an array of sorted, non-overlapping [start, end] ranges matching query terms. */
  function highlightRanges(text, query) {
    if (!text || !query) return [];
    const str = String(text);
    const q = String(query).trim();
    if (!q) return [];
    const lower = str.toLowerCase();
    const terms = q.toLowerCase().split(/\s+/).filter(Boolean);
    const ranges = [];
    for (const term of terms) {
      let idx = 0;
      while ((idx = lower.indexOf(term, idx)) !== -1) {
        ranges.push([idx, idx + term.length]);
        idx += 1;
      }
    }
    if (!ranges.length) return [];
    ranges.sort((a, b) => a[0] - b[0] || b[1] - a[1]);
    const merged = [ranges[0]];
    for (let i = 1; i < ranges.length; i++) {
      const cur = ranges[i];
      const prev = merged[merged.length - 1];
      if (cur[0] <= prev[1]) {
        prev[1] = Math.max(prev[1], cur[1]);
      } else {
        merged.push(cur);
      }
    }
    return merged;
  }

  /** Split string into an array of { text, match } segments. */
  function highlightSegments(text, query) {
    const str = String(text == null ? '' : text);
    const ranges = highlightRanges(str, query);
    if (!ranges.length) return [{ text: str, match: false }];
    const segs = [];
    let last = 0;
    for (const [start, end] of ranges) {
      if (start > last) segs.push({ text: str.slice(last, start), match: false });
      segs.push({ text: str.slice(start, end), match: true });
      last = end;
    }
    if (last < str.length) segs.push({ text: str.slice(last), match: false });
    return segs;
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

  /** Fields show the preview the host computed after decryption (f.masked: card last 4, API secret first4••••last4); otherwise fully masked. */
  function mask(kind, masked) { return masked || (kind === 'card_number' ? '•••• •••• •••• ••••' : '••••••••••'); }

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

  /** Check if a URL has an allowed scheme (http or https with non-empty hostname). */
  function isAllowedUrl(u) {
    if (!u || typeof u !== 'string') return false;
    try {
      const parsed = new URL(u);
      return (parsed.protocol === 'http:' || parsed.protocol === 'https:') && Boolean(parsed.hostname);
    } catch (_) {
      return false;
    }
  }

  /** Extract the username from an item detail or summary. */
  function itemUsername(detail, summary) {
    if (detail && Array.isArray(detail.fields)) {
      const f = detail.fields.find((x) => x && (x.name === 'username' || x.label === 'username'));
      if (f && f.value) return String(f.value).trim();
    }
    if (detail && detail.username) return String(detail.username).trim();
    if (summary && summary.username) return String(summary.username).trim();
    const cat = (detail && detail.category) || (summary && summary.category);
    if (cat === 'login') {
      const sub = (detail && detail.subtitle) || (summary && summary.subtitle);
      if (sub && sub !== '—') return String(sub).trim();
    }
    return '';
  }

  /** Build the secondary text for a row; when query matches the domain rather than subtitle, shows "subtitle · domain". */
  function rowSubtitle(it, query) {
    if (!it) return '—';
    const host = it.url ? hostOfUrl(it.url) : '';
    const q = String(query || '').trim();
    if (it.subtitle && host && q && matchesQuery({ url: it.url }, q) && !matchesQuery({ subtitle: it.subtitle }, q)) {
      return it.subtitle + ' · ' + host;
    }
    return it.subtitle || host || '—';
  }

  /** Extract up to `max` recently used items, sorted newest first. */
  function getRecentlyUsed(all, max = 5) {
    if (!Array.isArray(all)) return [];
    return all
      .filter((i) => i && i.last_used && i.last_used > 0)
      .sort((a, b) => b.last_used - a.last_used)
      .slice(0, max);
  }

  /** Severity token for a Watchtower finding kind: danger or warn. */
  function findingSeverity(kind) {
    if (kind === 'weak' || kind === 'reused' || kind === 'pwned' || kind === 'expired') return 'danger';
    return 'warn';
  }

  /** Display label for a Watchtower finding chip. */
  function findingLabel(kind, meta, tFn) {
    const t = typeof tFn === 'function' ? tFn : (k, d) => d;
    switch (kind) {
      case 'weak':
        return t('watchtowerWeak', 'Weak password');
      case 'reused':
        return meta && meta.reused_count
          ? t('watchtowerReusedN', 'Reused on $1 items', [meta.reused_count])
          : t('watchtowerReused', 'Reused password');
      case 'pwned':
        return t('watchtowerPwned', 'In a data breach');
      case 'old':
        if (meta && typeof meta.old_days === 'number') {
          if (meta.old_days >= 365) {
            const yrs = Math.floor(meta.old_days / 365);
            return yrs === 1
              ? t('watchtowerOld1Yr', 'Not changed in 1 year')
              : t('watchtowerOldNYrs', 'Not changed in $1 years', [yrs]);
          }
          return t('watchtowerOldNDays', 'Not changed in $1 days', [meta.old_days]);
        }
        return t('watchtowerOld', 'Old password');
      case 'insecure_url':
        return t('watchtowerInsecureUrl', 'Insecure website (HTTP)');
      case 'missing_totp':
        return t('watchtowerMissingTotp', '2FA not configured');
      case 'totp_available':
        return t('watchtowerTotpAvailable', '2FA available on site');
      case 'passkey_available':
        return t('watchtowerPasskeyAvailable', 'Passkey available');
      case 'expired':
        return t('watchtowerExpired', 'Expired');
      case 'expiring_soon':
        return t('watchtowerExpiringSoon', 'Expiring soon');
      case 'duplicate':
        return t('watchtowerDuplicate', 'Duplicate login');
      default:
        return String(kind);
    }
  }

  /** Whether an item findings object contains any active findings. */
  function hasWatchtowerFindings(findings) {
    return Boolean(findings && Array.isArray(findings.kinds) && findings.kinds.length > 0);
  }

  /**
   * Run `fn` over `items` with at most `limit` calls in flight. The first call runs alone so a
   * host-side cache (e.g. the Watchtower audit) is warm before the rest fan out.
   */
  async function mapLimit(items, limit, fn) {
    const list = Array.from(items || []);
    if (!list.length) return [];
    const out = new Array(list.length);
    out[0] = await fn(list[0], 0);
    let next = 1;
    const n = Math.max(1, Math.min(limit | 0 || 1, list.length - 1));
    const worker = async () => {
      while (next < list.length) {
        const i = next++;
        out[i] = await fn(list[i], i);
      }
    };
    await Promise.all(Array.from({ length: n }, worker));
    return out;
  }

  /** Watchtower findings worth caching: a failed lookup (null/undefined) stays uncached so it is retried. */
  function cacheableFindings(res) {
    return Boolean(res && Array.isArray(res.kinds));
  }

  const api = { mapLimit, cacheableFindings, CATEGORIES, CAT_LABEL, CAT_KEY, REQUEST_TIMEOUT_MS, SKELETON_DELAY_MS, UNLOCK_TIMEOUT_MS, timeoutForMessage, withTimeout, buildFilters, filterItems, SORTS, emptyState, matchesQuery, virtualWindow, scrollToRow, moveSel, keyAction, ringOffset, formatValue, mask, fillKind, primarySecretField, viewFor, hostOfUrl, isAllowedUrl, isHttpUrl: isAllowedUrl, itemUsername, highlightRanges, highlightSegments, rowSubtitle, findingSeverity, findingLabel, hasWatchtowerFindings, getRecentlyUsed };
  root.GitpassView = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
