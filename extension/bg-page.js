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
  const TTL_MS = 5 * 60 * 1000;
  const OFFER_TTL_MS = 2 * 60 * 1000; // how long a filled login may offer its one-time code (#181)
  const pendingSaves = new Map(); // tabId -> { origin, kind, username, password, item_id, prompt, ts }
  const lastUsername = new Map(); // tabId -> { origin, username, ts }  (multi-step logins: step 1 value, 5 min TTL)
  const lastFilled = new Map(); // tabId -> { origin, item_id, ts }  (just-filled login; offers its one-time code on the next page)
  const lastSaved = new Map(); // tabId -> { origin, item_id, ts }  ("Open in gitpass" after a banner save)
  const inflightDecisions = new Map(); // tabId -> promise settled when the write ends (no replay, see waitInflight)
  const PAGE_LISTABLE = new Set(['credit_card', 'identity']);
  let neverSaveFallback = [];

  function err(code, message) { const e = new Error(message || code); e.code = code; return e; }

  function getNeverSave() {
    return new Promise((res) => {
      if (!root.chrome || !chrome.storage || !chrome.storage.local) {
        return res((api && api._neverSaveFallback) || neverSaveFallback);
      }
      chrome.storage.local.get(['neverSave', 'neverSaveSites'], (r) => {
        const list = (r && (r.neverSave || r.neverSaveSites)) || [];
        res(Array.isArray(list) ? list : []);
      });
    });
  }

  function setNeverSave(list) {
    return new Promise((res) => {
      if (!root.chrome || !chrome.storage || !chrome.storage.local) {
        if (api) api._neverSaveFallback = list;
        neverSaveFallback = list;
        return res(list);
      }
      chrome.storage.local.set({ neverSave: list }, () => res(list));
    });
  }

  async function addNeverSave(domain) {
    const dom = (domain || '').trim().toLowerCase();
    if (!dom) return;
    const list = await getNeverSave();
    if (!list.some((d) => (d || '').toLowerCase() === dom)) {
      list.push(dom);
      await setNeverSave(list);
    }
  }

  async function removeNeverSave(domain) {
    const dom = (domain || '').trim().toLowerCase();
    if (!dom) return;
    const list = await getNeverSave();
    const next = list.filter((d) => (d || '').toLowerCase() !== dom);
    await setNeverSave(next);
  }

  async function isExceptedSite(origin) {
    const list = await getNeverSave();
    return GitpassPolicy.isNeverSave(origin, list);
  }

  function senderOrigin(sender) {
    if (!sender || !sender.tab || typeof sender.tab.id !== 'number') throw err('denied', 'page: messages must come from a tab');
    if (sender.frameId !== 0) throw err('denied', 'only the top frame may use gitpass');
    const origin = GitpassPolicy.originOf(sender.url || sender.tab.url);
    if (!origin || !GitpassPolicy.isFillableOrigin(origin)) throw err('origin_mismatch', 'origin not fillable');
    return origin;
  }

  function fresh(map, tabId, origin, sameSiteOk, ttl) {
    const p = map.get(tabId);
    if (!p) return null;
    if (Date.now() - p.ts > (ttl || TTL_MS)) { map.delete(tabId); return null; }
    if (p.origin !== origin && !(sameSiteOk && GitpassPolicy.sameSite(p.origin, origin))) return null;
    return p;
  }

  async function matchItems(host, origin) {
    const r = await host('match', { origin });
    return (r && r.items) || [];
  }

  /** id/name only; a failed call (locked, app restarting) leaves the banner on the default vault. */
  async function vaultChoices(host) {
    try {
      const r = await host('list_vaults');
      return ((r && r.vaults) || [])
        .map((v) => ({ id: v && v.id, name: v && v.name }))
        .filter((v) => typeof v.id === 'string' && v.id && typeof v.name === 'string' && v.name);
    } catch (_) {
      return [];
    }
  }

  function saveTitle(v, origin) {
    const fallback = new URL(origin).hostname.replace(/^www\./, '');
    if (v == null) return fallback;
    if (typeof v !== 'string') throw err('bad_request', 'title');
    const title = v.trim();
    if (!title) return fallback; // cleared field: same hostname fallback as save_login
    if (title.length > 200) throw err('bad_request', 'title is too long');
    return title;
  }

  async function saveVault(host, v) {
    if (v == null || v === '') return undefined;
    if (typeof v !== 'string') throw err('bad_request', 'vault_id');
    // Strict call: locked/app_not_running must reach the banner so it can offer unlock, not read as a bad vault.
    const r = await host('list_vaults');
    if (!((r && r.vaults) || []).some((x) => x && x.id === v)) throw err('not_found', 'vault not found');
    return v;
  }

  /** Remember (id only, never a secret) which login gitpass just filled in this tab. */
  function rememberFilled(tabId, origin, itemId) {
    if (typeof tabId === 'number' && origin && typeof itemId === 'string' && itemId) lastFilled.set(tabId, { origin, item_id: itemId, ts: Date.now() });
  }

  function rememberSaved(tabId, origin, itemId) {
    if (itemId) lastSaved.set(tabId, { origin, item_id: itemId, ts: Date.now() });
  }

  /** A content script booting after a redirect must see the outcome of the save that is still writing. */
  async function waitInflight(tabId) {
    const pending = inflightDecisions.get(tabId);
    if (pending) await pending.catch(() => {});
  }

  /** Decide save vs update for a captured login. Returns the banner descriptor or null. */
  async function decide(host, origin, cap) {
    let items;
    try { items = await matchItems(host, origin); } catch (e) { return { prompt: 'save', items: null, error: e.code || e.message }; }
    const logins = items.filter((i) => !i.category || i.category === 'login');
    const same = logins.filter((i) => (i.subtitle || '').trim().toLowerCase() === (cap.username || '').trim().toLowerCase());
    for (const it of same) {
      try {
        const c = await host('fill', { item_id: it.id, origin });
        if (c && c.password === cap.password && (!cap.username || c.username === cap.username)) return null; // already saved, nothing to do
        return { prompt: 'update', item_id: it.id, title: it.title, hasUser: !!(it.subtitle || '').trim() };
      } catch (_) { /* fall through to next */ }
    }
    // Offer "Update existing" only when that cannot overwrite a different account: the captured
    // username is empty, or the single existing login has no username yet. Otherwise save new.
    const capUser = (cap.username || '').trim();
    const blank = logins.filter((i) => !(i.subtitle || '').trim());
    const target = !capUser ? (logins.length === 1 ? logins[0] : null) : (blank.length === 1 ? blank[0] : null);
    if (target) {
      const existing = target;
      try {
        const c = await host('fill', { item_id: existing.id, origin });
        if (c && c.password === cap.password && (!cap.username || c.username === cap.username)) return null;
        return { prompt: 'update', item_id: existing.id, title: existing.title, hasUser: !!(existing.subtitle || '').trim() };
      } catch (_) {}
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
        rememberFilled(tabId, origin, a.item_id);
        return { origin, username: c.username, password: c.password, totp: c.totp || null };
      }
      case 'page:totp': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        const ok = (await matchItems(host, origin)).some((i) => i.id === a.item_id);
        if (!ok) throw err('origin_mismatch', 'item does not match this site');
        const t = await host('totp', { item_id: a.item_id });
        const lf = lastFilled.get(tabId);
        if (lf && lf.item_id === a.item_id) lastFilled.delete(tabId); // the code is used: stop offering it
        return { origin, code: t.code, remaining: t.remaining };
      }
      case 'page:totp_offer': {
        // The login gitpass filled in this tab within OFFER_TTL_MS on the same registrable domain, and only
        // if it still matches this page's origin and has a one-time code. Returns ids and a title, no code.
        const lf = fresh(lastFilled, tabId, origin, true, OFFER_TTL_MS);
        if (!lf) return { origin, offer: null };
        const it = (await matchItems(host, origin)).find((i) => i.id === lf.item_id && i.has_totp && (!i.category || i.category === 'login'));
        if (!it) return { origin, offer: null };
        return { origin, offer: { item_id: it.id, title: typeof it.title === 'string' ? it.title : '', expires_in_ms: Math.max(0, OFFER_TTL_MS - (Date.now() - lf.ts)) } };
      }
      case 'page:fill_card':
      case 'page:fill_identity': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        if (!a.user_click) throw err('denied', 'card/identity fill needs an in-page click');
        const cmd = a.type === 'page:fill_card' ? 'fill_card' : 'fill_identity';
        const r = await host(cmd, { item_id: a.item_id, origin, source: 'inpage_click' });
        const fields = (r && (r.fields || r)) || {};
        if (cmd === 'fill_card') await fillPaymentFrames(tabId, fields).catch(() => 0);
        return { origin, fields };
      }
      case 'page:generate': {
        const r = await host('generate', { len: 20, digits: true, symbols: true });
        return { origin, password: r.password };
      }
      case 'page:request_unlock': return host('request_unlock', { reason: 'Fill on ' + new URL(origin).host });
      case 'page:open_app': {
        // Only the item id kept from this tab's own successful save is forwarded; content never sends one.
        const saved = a.last_save ? fresh(lastSaved, tabId, origin, true) : null;
        return host('open_in_app', saved ? { item_id: saved.item_id } : {});
      }

      // ---- save-login / update-password banner ----
      case 'page:capture': {
        const kind = a.kind;
        const username = typeof a.username === 'string' ? a.username.slice(0, 512) : '';
        if (kind === 'username') { if (username) lastUsername.set(tabId, { origin, username, ts: Date.now() }); return { prompt: null }; }
        if (!['login', 'signup', 'change_password'].includes(kind)) throw err('bad_request', 'kind');
        if (typeof a.password !== 'string' || !a.password || a.password.length > 1024) throw err('bad_request', 'password');
        await waitInflight(tabId); // process this capture after the save that is still writing
        if (await isExceptedSite(origin)) {
          pendingSaves.delete(tabId);
          return { prompt: null };
        }
        let user = username;
        if (!user) { const lu = fresh(lastUsername, tabId, origin, true); if (lu && lu.username) user = lu.username; }
        else { lastUsername.set(tabId, { origin, username: user, ts: Date.now() }); }
        const cap = { origin, kind, username: user, password: a.password, ts: Date.now() };
        const d = await decide(host, origin, cap);
        if (!d) { pendingSaves.delete(tabId); return { prompt: null }; }
        pendingSaves.set(tabId, { ...cap, ...d });
        return { prompt: d.prompt, username: user, title: d.title || null, vaults: await vaultChoices(host) };
      }
      case 'page:pending': {
        await waitInflight(tabId); // a redirect during Saving… still gets the success/failure outcome
        if (await isExceptedSite(origin)) {
          pendingSaves.delete(tabId);
          return { prompt: null };
        }
        const p = fresh(pendingSaves, tabId, origin, true);
        return p ? { prompt: p.prompt, username: p.username, title: p.title || null, vaults: await vaultChoices(host) } : { prompt: null };
      }
      case 'page:save_decision': {
        const p = fresh(pendingSaves, tabId, origin, true);
        if (inflightDecisions.has(tabId)) throw err('bad_request', 'already saving'); // no dismiss/never mid-write
        if (a.action === 'never') {
          pendingSaves.delete(tabId);
          if (!a.user_click) throw err('denied', 'needs a click');
          const dom = GitpassPolicy.registrableDomain(origin);
          if (dom) await addNeverSave(dom);
          return { done: true, action: 'never' };
        }
        if (!p) throw err('not_found', 'nothing to save');
        if (a.action === 'dismiss') { pendingSaves.delete(tabId); return { done: false }; }
        if (!a.user_click) throw err('denied', 'needs a click');
        let settle;
        inflightDecisions.set(tabId, new Promise((res) => { settle = res; }));
        try {
          const user = p.username || '';
          if (a.action === 'update') {
            if (p.prompt !== 'update') throw err('bad_request', 'action');
            const upArgs = { item_id: p.item_id, password: p.password }; // never an item id from the content side
            if (user && !p.hasUser) upArgs.username = user;
            await host('update_password', upArgs);
            if (pendingSaves.get(tabId) === p) pendingSaves.delete(tabId); // never drop a newer capture
            rememberSaved(tabId, origin, p.item_id);
            return { done: true, action: 'update' };
          }
          // save (new prompt) and save_new (second account from an update prompt). Neither carries an
          // item id, origin or password from the page side: they come from pendingSaves.
          if (a.action === 'save' || a.action === 'save_new') {
            if ((a.action === 'save') !== (p.prompt === 'save')) throw err('bad_request', 'action');
            const title = saveTitle(a.title, p.origin);
            const vault_id = await saveVault(host, a.vault_id);
            const args = { origin: p.origin, title, username: user, password: p.password };
            if (vault_id) args.vault_id = vault_id;
            const r = await host('save_login', args);
            // Only a completed decision consumes this capture; errors keep it for retry, and a capture
            // that replaced it while we were writing is left alone.
            if (pendingSaves.get(tabId) === p) pendingSaves.delete(tabId);
            rememberSaved(tabId, origin, r && r.item_id);
            return { done: true, action: 'save', item_id: r && r.item_id };
          }
          throw err('bad_request', 'action');
        } finally {
          inflightDecisions.delete(tabId);
          settle();
        }
      }
    }
    throw err('bad_request', 'unknown page message');
  }

  function forgetTab(tabId) { pendingSaves.delete(tabId); lastUsername.delete(tabId); lastSaved.delete(tabId); lastFilled.delete(tabId); inflightDecisions.delete(tabId); }
  if (root.chrome && chrome.tabs && chrome.tabs.onRemoved) chrome.tabs.onRemoved.addListener(forgetTab);

  /** Fill a card into the tab's payment-processor iframes (Stripe Elements & co., see GitpassPolicy.
   *  isPaymentFrameOrigin), so one pick in the top frame fills number / expiry / CVC living in separate
   *  frames. Step 1 asks every frame only for its origin (no data); step 2 sends the card only to allow-listed
   *  child frames, and the injected code re-checks its own origin before touching anything (the frame may
   *  have navigated in between). Returns the number of fields filled. */
  async function fillPaymentFrames(tabId, fields) {
    const S = root.chrome && chrome.scripting;
    if (!S || typeof tabId !== 'number') return 0;
    const probe = await S.executeScript({ target: { tabId, allFrames: true }, func: () => location.origin }).catch(() => []);
    const frameIds = (probe || []).filter((x) => x && x.frameId !== 0 && GitpassPolicy.isPaymentFrameOrigin(x.result)).map((x) => x.frameId);
    if (!frameIds.length) return 0;
    await S.executeScript({ target: { tabId, frameIds }, files: ['lib/policy.js', 'lib/forms.js'] });
    const res = await S.executeScript({
      target: { tabId, frameIds },
      func: (map) => (globalThis.GitpassPolicy && GitpassPolicy.isPaymentFrameOrigin(location.origin) && window !== top
        ? GitpassForms.fillCardDocument(document, map) : 0),
      args: [fields],
    });
    return (res || []).reduce((n, x) => n + (Number(x && x.result) || 0), 0);
  }

  const api = {
    handle,
    forgetTab,
    rememberFilled,
    fillPaymentFrames,
    OFFER_TTL_MS,
    _lastFilled: lastFilled,
    _pendingSaves: pendingSaves,
    _neverSaveFallback: neverSaveFallback,
    _getNeverSave: getNeverSave,
    _setNeverSave: setNeverSave,
    _addNeverSave: addNeverSave,
    _removeNeverSave: removeNeverSave,
    _isExceptedSite: isExceptedSite,
  };
  root.GitpassPage = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
// ===== end PAGE routing =====
