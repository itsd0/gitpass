// ===== gitpass background: right-click context menu (issue #213) =====
// Loaded by background.js via importScripts (Chrome) / background.scripts (Firefox).
// The menu is rebuilt for the active tab on install, startup, tab activation,
// navigation completion and vault lock/unlock/change events (background.js calls
// rebuild() when the host reports one of those events).
//
// Menu contents (never a secret in a title: only item titles and the login
// username/subtitle are used; passwords, card numbers and other field values
// are never read here, let alone shown):
//   - locked vault: one top-level "Unlock gitpass…" entry.
//   - unlocked: a "gitpass" parent with a "Fill login ›" submenu (up to 5
//     host-matched logins for the tab's origin, then "Search gitpass…"),
//     "Suggest strong password", "Fill card ›" / "Fill identity ›" submenus
//     (only when the vault has such items), and "Open gitpass".
// Clicks are routed through the background's existing fill paths, so the host
// still enforces the origin match for logins and the popup/in-page trust
// boundary for card/identity fills. The fill is always addressed to the frame
// that was right-clicked (info.frameId, default 0) and never redirected to
// another frame: the content script is intentionally top-frame-only, so a
// click inside a subframe simply has no receiver and nothing is filled
// anywhere else. Filling the top frame instead would risk putting credentials
// into a different frame's form, so failing closed is the correct behaviour.
// "Search gitpass…" and "Open gitpass" open the action popup (the same call
// bg-onboard.js already uses); "Unlock gitpass…" asks the host to unlock.
// "Suggest strong password" asks the clicked frame's content script to run
// the existing in-page suggest path (passwordrules -> page:generate ->
// fillNewPassword), so the filled password flows through the usual
// save-banner capture when the form is submitted.
// /* global chrome */
(function (root) {
  'use strict';

  const MAX_ITEMS = 5;
  const DOC_PATTERNS = ['https://*/*', 'http://localhost/*', 'http://127.0.0.1/*'];
  const ID_ROOT = 'ctx-root';
  const ID_FILL_LOGIN = 'ctx-fill-login';
  const ID_FILL_CARD = 'ctx-fill-card';
  const ID_FILL_IDENTITY = 'ctx-fill-identity';
  const ID_SEARCH = 'ctx-search';
  const ID_SUGGEST = 'ctx-suggest';
  const ID_OPEN = 'ctx-open';
  const ID_UNLOCK = 'ctx-unlock';

  function trunc(s, n) {
    const v = String(s == null ? '' : s).trim();
    return v.length > n ? v.slice(0, n - 1) + '…' : v;
  }

  /** Menu title for a login: title plus username, never a secret. */
  function loginTitle(it) {
    const title = trunc((it && it.title) || '', 60) || '(untitled)';
    const user = trunc((it && (it.subtitle || it.username)) || '', 40);
    return user ? title + ' (' + user + ')' : title;
  }

  /** Menu title for a card/identity: title only, never a number or field value. */
  function itemTitle(it) {
    return trunc((it && it.title) || '', 60) || '(untitled)';
  }

  function isLockedError(e) {
    if (!e) return false;
    if (e.code === 'locked') return true;
    return /\blocked\b/.test(String(e.message || e));
  }

  function cleanItems(items) {
    return ((items) || []).filter((i) => i && typeof i.id === 'string').slice(0, MAX_ITEMS);
  }

  /**
   * Pure: menu item descriptors for a state (see loadState) and a t(key, fallback)
   * translator. Locked -> exactly one Unlock entry; unavailable (host not
   * reachable, app not running) -> the parent plus Open only, so no fill or
   * suggest entry is offered that could not work.
   */
  function buildMenu(state, t) {
    const tt = typeof t === 'function' ? t : (k, fb) => fb;
    const s = state || {};
    if (s.locked) {
      return [{ id: ID_UNLOCK, title: tt('ctxUnlockGitpass', 'Unlock gitpass…'), contexts: ['editable', 'page'], documentUrlPatterns: DOC_PATTERNS }];
    }
    const root = { id: ID_ROOT, title: tt('ctxGitpass', 'gitpass'), contexts: ['editable', 'page'], documentUrlPatterns: DOC_PATTERNS };
    const child = (id, title, contexts) => ({ id, title, contexts, parentId: ID_ROOT, documentUrlPatterns: DOC_PATTERNS });
    const out = [root];
    if (s.unavailable) {
      out.push(child(ID_OPEN, tt('ctxOpenGitpass', 'Open gitpass'), ['page']));
      return out;
    }
    const logins = cleanItems(s.logins);
    if (logins.length) {
      out.push(child(ID_FILL_LOGIN, tt('ctxFillLogin', 'Fill login'), ['editable']));
      for (const it of logins) out.push(child('login:' + it.id, loginTitle(it), ['editable']));
    }
    out.push(child(ID_SEARCH, tt('ctxSearchGitpass', 'Search gitpass…'), ['editable']));
    out.push(child(ID_SUGGEST, tt('ctxSuggestPassword', 'Suggest strong password'), ['editable']));
    const cards = cleanItems(s.cards);
    if (cards.length) {
      out.push(child(ID_FILL_CARD, tt('ctxFillCard', 'Fill card'), ['editable']));
      for (const it of cards) out.push(child('card:' + it.id, itemTitle(it), ['editable']));
    }
    const identities = cleanItems(s.identities);
    if (identities.length) {
      out.push(child(ID_FILL_IDENTITY, tt('ctxFillIdentity', 'Fill identity'), ['editable']));
      for (const it of identities) out.push(child('identity:' + it.id, itemTitle(it), ['editable']));
    }
    out.push(child(ID_OPEN, tt('ctxOpenGitpass', 'Open gitpass'), ['page']));
    return out;
  }

  /** Parse a click's menuItemId into { kind: 'login'|'card'|'identity', itemId } or null. */
  function parseMenuId(id) {
    if (typeof id !== 'string') return null;
    const m = /^(login|card|identity):(.+)$/.exec(id);
    return m ? { kind: m[1], itemId: m[2] } : null;
  }

  /**
   * Load the menu state for the active tab. deps: { activeTab(), host(cmd, args),
   * originOf(url), isFillableOrigin(origin) }. A non-fillable active tab (or no
   * tab) yields `unavailable`: the documentUrlPatterns on every item keep the
   * menu off pages where filling is impossible anyway.
   */
  async function loadState(deps) {
    const empty = { locked: false, unavailable: true, origin: null, logins: [], cards: [], identities: [] };
    let tab = null;
    try { tab = await deps.activeTab(); } catch (_) { return empty; }
    const origin = tab && deps.originOf(tab.url);
    if (!origin || !deps.isFillableOrigin(origin)) return empty;
    let st;
    try { st = await deps.host('status'); } catch (e) { return isLockedError(e) ? { locked: true, origin, logins: [], cards: [], identities: [] } : empty; }
    if (st && st.unlocked === false) return { locked: true, origin, logins: [], cards: [], identities: [] };
    let logins = [];
    try {
      const m = await deps.host('match', { origin });
      logins = ((m && m.items) || []).filter((i) => i && typeof i.id === 'string' && (!i.category || i.category === 'login'));
    } catch (e) { if (isLockedError(e)) return { locked: true, origin, logins: [], cards: [], identities: [] }; }
    let cards = [], identities = [];
    try { const r = await deps.host('list_items', { category: 'credit_card', limit: MAX_ITEMS }); cards = (r && r.items) || []; } catch (_) {}
    try { const r = await deps.host('list_items', { category: 'identity', limit: MAX_ITEMS }); identities = (r && r.items) || []; } catch (_) {}
    return { locked: false, unavailable: false, origin, logins, cards, identities };
  }

  async function removeAll(cm) {
    await new Promise((resolve) => {
      try {
        const r = cm.removeAll(() => resolve());
        if (r && typeof r.then === 'function') r.then(resolve, resolve);
      } catch (_) { resolve(); }
    });
  }

  /** Rebuild the whole menu from the active tab's state. deps: loadState deps + { chrome, t }. */
  async function rebuild(deps) {
    const cm = deps.chrome && deps.chrome.contextMenus;
    if (!cm) return;
    let state;
    try { state = await loadState(deps); } catch (_) { state = { unavailable: true, logins: [], cards: [], identities: [] }; }
    await removeAll(cm);
    for (const it of buildMenu(state, deps.t)) {
      try { const r = cm.create(it); if (r && typeof r.then === 'function') await r; } catch (_) { /* one bad item must not kill the rest */ }
    }
  }

  /** Tab + fillable origin + clicked frame for a fill/suggest click, else null. */
  async function fillContext(info, tab, deps) {
    let t = tab;
    if (!t || typeof t.id !== 'number') {
      try { t = await deps.activeTab(); } catch (_) { return null; }
    }
    if (!t || typeof t.id !== 'number') return null;
    const origin = deps.originOf(t.url);
    if (!origin || !deps.isFillableOrigin(origin)) return null;
    const frameId = info && Number.isInteger(info.frameId) ? info.frameId : 0;
    return { tabId: t.id, origin, frameId };
  }

  /**
   * Route one context-menu click. deps: { activeTab, host, fillTab, sendToTab,
   * fillPaymentFrames, openPopup, originOf, isFillableOrigin }. Never throws
   * for a menu click itself; fill failures surface as { action, error } so the
   * caller can log them instead of filling some other frame. Returns a small
   * decision object (for tests); the background caller ignores it.
   */
  async function handleClick(info, tab, deps) {
    const id = info && info.menuItemId;
    if (typeof id !== 'string') return { action: 'ignore' };
    if (id === ID_UNLOCK) {
      await deps.host('request_unlock', { reason: 'Unlock gitpass browser extension' });
      return { action: 'unlock' };
    }
    if (id === ID_OPEN || id === ID_SEARCH) {
      await deps.openPopup();
      return { action: 'open' };
    }
    if (id === ID_SUGGEST) {
      const ctx = await fillContext(info, tab, deps);
      if (!ctx) return { action: 'ignore' };
      let r = null;
      try { r = await deps.sendToTab(ctx.tabId, { type: 'gitpass-suggest-password', origin: ctx.origin }, ctx.frameId); } catch (_) { r = null; }
      return { action: 'suggest', filled: !!(r && r.filled), frameId: ctx.frameId };
    }
    const parsed = parseMenuId(id);
    if (!parsed) return { action: 'ignore' };
    const ctx = await fillContext(info, tab, deps);
    if (!ctx) return { action: 'ignore' };
    try {
      if (parsed.kind === 'login') {
        await deps.fillTab(ctx.tabId, ctx.origin, parsed.itemId, { frameId: ctx.frameId });
      } else {
        // Card/identity fills need a trusted UI gesture: the context-menu click
        // is one, and it arrives through the background (never through a page),
        // so it uses the same 'popup' source the popup Fill path uses. The host
        // still checks the item's category; the content script re-checks the
        // origin before writing anything.
        const fields = await deps.host(parsed.kind === 'card' ? 'fill_card' : 'fill_identity', { item_id: parsed.itemId, source: 'popup' });
        await deps.sendToTab(ctx.tabId, { type: 'gitpass-fill-fields', kind: parsed.kind, origin: ctx.origin, fields }, ctx.frameId);
        if (parsed.kind === 'card' && deps.fillPaymentFrames) await deps.fillPaymentFrames(ctx.tabId, fields).catch(() => 0);
      }
    } catch (e) {
      return { action: 'error', kind: parsed.kind, frameId: ctx.frameId, error: String((e && e.message) || e) };
    }
    return { action: 'fill', kind: parsed.kind, frameId: ctx.frameId };
  }

  /**
   * Wire the menu into a browser. deps: rebuild deps + handleClick deps, with
   * `chrome`. Returns { rebuild, handleClick } for the background to reuse
   * (e.g. on host lock/unlock/change events). Returns null without
   * chrome.contextMenus (unsupported browser build).
   */
  function install(deps) {
    const ch = deps.chrome;
    if (!ch || !ch.contextMenus) return null;
    const api = {
      rebuild: () => rebuild(deps).catch(() => {}),
      handleClick: (info, tab) => handleClick(info, tab, deps).catch(() => {}),
    };
    if (ch.contextMenus.onClicked) ch.contextMenus.onClicked.addListener((info, tab) => { api.handleClick(info, tab); });
    if (ch.runtime) {
      if (ch.runtime.onInstalled) ch.runtime.onInstalled.addListener(() => { api.rebuild(); });
      if (ch.runtime.onStartup) ch.runtime.onStartup.addListener(() => { api.rebuild(); });
    }
    if (ch.tabs) {
      if (ch.tabs.onActivated) ch.tabs.onActivated.addListener(() => { api.rebuild(); });
      if (ch.tabs.onUpdated) ch.tabs.onUpdated.addListener((_tabId, changeInfo, tab) => { if (changeInfo && changeInfo.status === 'complete' && tab && tab.active) api.rebuild(); });
    }
    api.rebuild();
    return api;
  }

  const api = { buildMenu, parseMenuId, loadState, rebuild, handleClick, install, MAX_ITEMS, IDS: { root: ID_ROOT, fillLogin: ID_FILL_LOGIN, fillCard: ID_FILL_CARD, fillIdentity: ID_FILL_IDENTITY, search: ID_SEARCH, suggest: ID_SUGGEST, open: ID_OPEN, unlock: ID_UNLOCK } };
  root.GitpassContextMenu = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
// ===== end right-click context menu =====
