// ===== gitpass background: PAGE routing (workstream 4, `page:` messages from content.js) =====
// Loaded by background.js via importScripts (Chrome) / background.scripts (Firefox).
// Security rules enforced here:
//  - Only content scripts in the TOP frame of a tab may use page:* (sender.frameId === 0, sender.tab set).
//  - The page origin is always taken from the browser-provided sender, never from the message.
//  - Only https (or http loopback) origins; the native host re-checks the item URL against the origin on `fill`.
//  - Secrets are returned only in the response to the exact fill request; the content script re-checks
//    the origin and drops them right after filling. Captured logins for the save banner stay in this
//    service worker's memory (per tab, 3 min TTL) and are never sent back to the page.
/* global chrome, GitpassPolicy */
(function (root) {
  'use strict';
  const TTL_MS = 3 * 60 * 1000;
  const pendingSaves = new Map(); // tabId -> { origin, kind, username, password, item_id, prompt, ts }
  const lastUsername = new Map(); // tabId -> { origin, username, ts }  (multi-step logins: step 1 value)
  const PAGE_LISTABLE = new Set(['credit_card', 'identity']);

  function err(code, message) { const e = new Error(message || code); e.code = code; return e; }

  function senderOrigin(sender) {
    if (!sender || !sender.tab || typeof sender.tab.id !== 'number') throw err('denied', 'page: messages must come from a tab');
    if (sender.frameId !== 0) throw err('denied', 'only the top frame may use gitpass');
    const origin = GitpassPolicy.originOf(sender.url || sender.tab.url);
    if (!origin || !GitpassPolicy.isFillableOrigin(origin)) throw err('origin_mismatch', 'origin not fillable');
    return origin;
  }

  function fresh(map, tabId, origin, sameSiteOk) {
    const p = map.get(tabId);
    if (!p) return null;
    if (Date.now() - p.ts > TTL_MS) { map.delete(tabId); return null; }
    if (p.origin !== origin && !(sameSiteOk && GitpassPolicy.sameSite(p.origin, origin))) return null;
    return p;
  }

  async function matchItems(host, origin) {
    const r = await host('match', { origin });
    return (r && r.items) || [];
  }

  /** Decide save vs update for a captured login. Returns the banner descriptor or null. */
  async function decide(host, origin, cap) {
    let items;
    try { items = await matchItems(host, origin); } catch (e) { return { prompt: 'save', items: null, error: e.code || e.message }; }
    const logins = items.filter((i) => !i.category || i.category === 'login');
    const same = logins.filter((i) => (i.subtitle || '') === (cap.username || ''));
    for (const it of same) {
      try {
        const c = await host('fill', { item_id: it.id, origin });
        if (c && c.password === cap.password) return null; // already saved, nothing to do
        return { prompt: 'update', item_id: it.id, title: it.title };
      } catch (_) { /* fall through to save */ }
    }
    return { prompt: 'save' };
  }

  async function handle(msg, sender, host) {
    const origin = senderOrigin(sender);
    const tabId = sender.tab.id;
    const a = msg || {};
    switch (a.type) {
      case 'page:status': return host('status');
      case 'page:match': return { origin, items: await matchItems(host, origin) };
      case 'page:list': {
        if (!PAGE_LISTABLE.has(a.category)) throw err('bad_request', 'category not listable from a page');
        const r = await host('list_items', { category: a.category, limit: 20 });
        return { origin, items: ((r && r.items) || []).filter((i) => i.category === a.category) };
      }
      case 'page:fill': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        const c = await host('fill', { item_id: a.item_id, origin }); // host enforces the origin match
        return { origin, username: c.username, password: c.password, totp: c.totp || null };
      }
      case 'page:totp': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        const ok = (await matchItems(host, origin)).some((i) => i.id === a.item_id);
        if (!ok) throw err('origin_mismatch', 'item does not match this site');
        const t = await host('totp', { item_id: a.item_id });
        return { origin, code: t.code, remaining: t.remaining };
      }
      case 'page:fill_card':
      case 'page:fill_identity': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        if (!a.user_click) throw err('denied', 'card/identity fill needs an in-page click');
        const cmd = a.type === 'page:fill_card' ? 'fill_card' : 'fill_identity';
        const r = await host(cmd, { item_id: a.item_id, origin, source: 'inpage_click' });
        return { origin, fields: (r && (r.fields || r)) || {} };
      }
      case 'page:generate': {
        const r = await host('generate', { len: 20, digits: true, symbols: true });
        return { origin, password: r.password };
      }
      case 'page:request_unlock': return host('request_unlock', { reason: 'Fill on ' + new URL(origin).host });
      case 'page:open_app': return host('open_in_app', {});

      // ---- save-login / update-password banner ----
      case 'page:capture': {
        const kind = a.kind;
        const username = typeof a.username === 'string' ? a.username.slice(0, 512) : '';
        if (kind === 'username') { if (username) lastUsername.set(tabId, { origin, username, ts: Date.now() }); return { prompt: null }; }
        if (!['login', 'signup', 'change_password'].includes(kind)) throw err('bad_request', 'kind');
        if (typeof a.password !== 'string' || !a.password || a.password.length > 1024) throw err('bad_request', 'password');
        let user = username;
        if (!user) { const lu = fresh(lastUsername, tabId, origin, true); if (lu) user = lu.username; }
        const cap = { origin, kind, username: user, password: a.password, ts: Date.now() };
        const d = await decide(host, origin, cap);
        if (!d) { pendingSaves.delete(tabId); return { prompt: null }; }
        pendingSaves.set(tabId, { ...cap, ...d });
        return { prompt: d.prompt, username: user, title: d.title || null };
      }
      case 'page:pending': {
        const p = fresh(pendingSaves, tabId, origin, true);
        return p ? { prompt: p.prompt, username: p.username, title: p.title || null } : { prompt: null };
      }
      case 'page:save_decision': {
        const p = fresh(pendingSaves, tabId, origin, true);
        pendingSaves.delete(tabId);
        if (!p) throw err('not_found', 'nothing to save');
        if (a.action === 'dismiss') return { done: false };
        if (!a.user_click) throw err('denied', 'needs a click');
        if (p.prompt === 'update' && a.action === 'update') { await host('update_password', { item_id: p.item_id, password: p.password }); return { done: true, action: 'update' }; }
        if (a.action === 'save') {
          const title = new URL(p.origin).hostname.replace(/^www\./, '');
          const r = await host('save_login', { origin: p.origin, title, username: p.username, password: p.password });
          return { done: true, action: 'save', item_id: r && r.item_id };
        }
        throw err('bad_request', 'action');
      }
    }
    throw err('bad_request', 'unknown page message');
  }

  function forgetTab(tabId) { pendingSaves.delete(tabId); lastUsername.delete(tabId); }
  if (root.chrome && chrome.tabs && chrome.tabs.onRemoved) chrome.tabs.onRemoved.addListener(forgetTab);

  const api = { handle, forgetTab, _pendingSaves: pendingSaves };
  root.GitpassPage = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
// ===== end PAGE routing =====
