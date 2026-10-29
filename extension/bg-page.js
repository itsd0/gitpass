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
  const SHORT_TTL_MS = 60 * 1000; // 60 s for crossSiteApprovals, addUrlApprovals, searchActive, and pendingConfirms
  const OFFER_TTL_MS = 2 * 60 * 1000; // how long a filled login may offer its one-time code (#181)
  const pendingSaves = new Map(); // tabId -> { origin, kind, username, password, item_id, prompt, ts }
  const lastUsername = new Map(); // tabId -> { origin, username, ts }  (multi-step logins: step 1 value, 5 min TTL)
  const lastFilled = new Map(); // tabId -> { origin, item_id, ts }  (just-filled login; offers its one-time code on the next page)
  const lastSaved = new Map(); // tabId -> { origin, item_id, ts }  ("Open in gitpass" after a banner save)
  const inflightDecisions = new Map(); // tabId -> promise settled when the write ends (no replay, see waitInflight)
  const searchActive = new Map(); // tabId -> { origin, ts } (active while search mode is open)
  const crossSiteApprovals = new Map(); // `${tabId}:${origin}:${itemId}` -> { ts } (single-use, short-lived)
  const addUrlApprovals = new Map(); // `${tabId}:${origin}:${itemId}` -> { ts } (short-lived post-fill approval)
  const pendingConfirms = new Map(); // reqId -> { reqId, tabId, origin, itemId, pageHost, itemHost, title, resolve, reject, winId, ts }
  let confirmSeq = 0;
  const CONFIRM_URL = () => (root.chrome && chrome.runtime && chrome.runtime.getURL ? chrome.runtime.getURL('popup/confirm.html') : 'chrome-extension://mock/popup/confirm.html');
  function isConfirmUiSender(sender) {
    const extId = root.chrome && chrome.runtime && chrome.runtime.id;
    if (!sender || typeof sender.url !== 'string') return false;
    if (extId && sender.id !== extId) return false;
    if (sender.tab && typeof sender.tab.url === 'string' && !sender.tab.url.startsWith(CONFIRM_URL())) return false;
    return sender.url.startsWith(CONFIRM_URL());
  }
  function generateConfirmReqId() {
    if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
      return crypto.randomUUID();
    }
    if (typeof crypto !== 'undefined' && typeof crypto.getRandomValues === 'function') {
      const buf = new Uint8Array(16);
      crypto.getRandomValues(buf);
      return Array.from(buf, (b) => b.toString(16).padStart(2, '0')).join('');
    }
    return 'c' + (++confirmSeq) + '-' + Date.now() + '-' + Math.random().toString(36).slice(2);
  }
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

  /**
   * Card duplicate check (#196): a card with the same last 4 digits AND expiry
   * already in the vault suppresses the prompt. Summaries carry only the masked
   * subtitle ('•••• 4242'), so expiry comes from get_item detail (a non-secret
   * month_year field). A list/get failure (locked, app restarting) never
   * suppresses: the banner still shows and the save surfaces the real error.
   * Without an expiry on the capture there is nothing to match against, so no
   * suppression either — last 4 alone is not a duplicate.
   */
  async function hasDuplicateCard(host, cap) {
    if (!cap.expiry) return false;
    const last4 = cap.number.slice(-4);
    let items;
    try {
      const r = await host('list_items', { category: 'credit_card', limit: 500 });
      items = (r && r.items) || [];
    } catch (_) { return false; }
    for (const it of items) {
      if (!(it.subtitle || '').endsWith(last4)) continue;
      try {
        const rec = await host('get_item', { item_id: it.id });
        const fields = (rec && rec.item && rec.item.fields) || [];
        const ex = fields.find((f) => f.name === 'expiry');
        const CB = root.GitpassBanner || (typeof require === 'function' ? require('./lib/banner.js') : null);
        const norm = CB ? CB.normalizeCardExpiry(ex && ex.value) : '';
        if (norm && norm === cap.expiry) return true;
      } catch (_) { /* fall through: a card we cannot read is not a proven duplicate */ }
    }
    return false;
  }

  async function handle(msg, sender, host) {
    const origin = senderOrigin(sender);
    const tabId = sender.tab.id;
    const a = msg || {};
    switch (a.type) {
      case 'page:status': return host('status');
      case 'page:site_icon': {
        const h = String(a.host || '').trim().toLowerCase();
        if (!h) throw err('bad_request', 'empty host');
        const bgCache = globalThis.GitpassSiteIconCache;
        if (bgCache && bgCache.has(h)) {
          const cached = bgCache.get(h);
          if (!cached) throw err('not_found', 'icon not cached');
          return { data_url: cached };
        }
        try {
          const res = await host('site_icon', { host: h });
          if (res && res.data_url) {
            if (bgCache) bgCache.set(h, res.data_url);
            return res;
          }
        } catch (e) {
          if (e && (e.code === 'not_found' || String(e.message).includes('not_found') || String(e.message).includes('not cached'))) {
            if (bgCache) bgCache.set(h, null);
          }
          throw e;
        }
        throw err('not_found', 'icon not cached');
      }
      case 'page:match': return { origin, items: await matchItems(host, origin) };
      case 'page:search_enable': {
        if (!a.user_click) throw err('denied', 'search needs a user click');
        searchActive.set(tabId, { origin, ts: Date.now() });
        return { origin, ok: true };
      }
      case 'page:search_disable': {
        searchActive.delete(tabId);
        return { origin, ok: true };
      }
      case 'page:search': {
        const sa = fresh(searchActive, tabId, origin, false, SHORT_TTL_MS);
        if (!sa) throw err('denied', 'search not active for this tab');
        const q = typeof a.query === 'string' ? a.query : '';
        const r = await host('list_items', { category: 'login', query: q, limit: 100 });
        const items = ((r && r.items) || [])
          .filter((i) => !i.category || i.category === 'login')
          .map((i) => ({
            id: i.id,
            category: 'login',
            title: i.title || '',
            subtitle: i.subtitle || '',
            url: i.url || '',
            favicon_host: i.favicon_host || '',
            has_totp: !!i.has_totp,
            favorite: !!i.favorite,
          }));
        return { origin, items };
      }
      case 'page:request_cross_site_confirm': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        const rec = await host('get_item', { item_id: a.item_id });
        const it = rec && rec.item;
        if (!it) throw err('not_found', 'item not found');
        const pageHost = GitpassPolicy.registrableDomain(origin) || new URL(origin).hostname;
        let itemHost = it.url ? GitpassPolicy.registrableDomain(it.url) : null;
        if (!itemHost && it.url) {
          try { itemHost = new URL(it.url).hostname; } catch (_) {}
        }
        itemHost = itemHost || it.title || 'another site';

        const reqId = generateConfirmReqId();
        return new Promise((resolve, reject) => {
          let timer = null;
          const cleanup = () => {
            if (timer) { clearTimeout(timer); timer = null; }
            pendingConfirms.delete(reqId);
          };

          timer = setTimeout(() => {
            const c = pendingConfirms.get(reqId);
            if (c) {
              c.cleanup();
              c.reject(err('cancelled', 'request expired'));
            }
          }, SHORT_TTL_MS);

          pendingConfirms.set(reqId, {
            reqId,
            tabId,
            origin,
            itemId: a.item_id,
            pageHost,
            itemHost,
            title: it.title || '',
            resolve,
            reject,
            cleanup,
            ts: Date.now(),
          });

          if (root.chrome && chrome.windows && chrome.windows.create) {
            chrome.windows.create({
              url: chrome.runtime.getURL(`popup/confirm.html?req=${encodeURIComponent(reqId)}`),
              type: 'popup',
              width: 380,
              height: 240,
              focused: true,
            }).then((w) => {
              const c = pendingConfirms.get(reqId);
              if (c) {
                c.winId = w.id;
              } else {
                try { if (chrome.windows && chrome.windows.remove) chrome.windows.remove(w.id).catch(() => {}); } catch (_) {}
              }
            }).catch((e) => {
              cleanup();
              reject(err('denied', String(e.message || e)));
            });
          } else {
            cleanup();
            reject(err('denied', 'No UI prompt mechanism available'));
          }
        });
      }
      case 'page:list': {
        if (!PAGE_LISTABLE.has(a.category)) throw err('bad_request', 'category not listable from a page');
        const r = await host('list_items', { category: a.category, limit: 20 });
        return { origin, items: ((r && r.items) || []).filter((i) => i.category === a.category) };
      }
      case 'page:fill': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        const approvalKey = `${tabId}:${origin}:${a.item_id}`;
        const approval = crossSiteApprovals.get(approvalKey);
        const isApproved = approval && (Date.now() - approval.ts < SHORT_TTL_MS);
        crossSiteApprovals.delete(approvalKey); // strictly single-use

        const fillArgs = { item_id: a.item_id, origin };
        if (isApproved) {
          fillArgs.user_confirm = true;
        }
        const c = await host('fill', fillArgs); // host enforces the origin match unless user_confirm
        rememberFilled(tabId, origin, a.item_id);
        if (isApproved) {
          addUrlApprovals.set(approvalKey, { ts: Date.now() });
        }
        return { origin, username: c.username, password: c.password, totp: c.totp || null };
      }
      case 'page:add_url': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        if (!a.user_click) throw err('denied', 'add_url needs a click');
        const approvalKey = `${tabId}:${origin}:${a.item_id}`;
        const approval = addUrlApprovals.get(approvalKey);
        if (!approval || (Date.now() - approval.ts > SHORT_TTL_MS)) {
          addUrlApprovals.delete(approvalKey);
          throw err('denied', 'add_url is only allowed after a confirmed cross-site fill of this item');
        }
        addUrlApprovals.delete(approvalKey);
        await host('add_url', { item_id: a.item_id, origin, url: origin });
        return { origin, ok: true };
      }
      case 'page:totp': {
        if (typeof a.item_id !== 'string') throw err('bad_request', 'item_id');
        const approvalKey = `${tabId}:${origin}:${a.item_id}`;
        const approval = crossSiteApprovals.get(approvalKey);
        const isApproved = approval && (Date.now() - approval.ts < SHORT_TTL_MS);
        crossSiteApprovals.delete(approvalKey); // strictly single-use
        const isMatched = (await matchItems(host, origin)).some((i) => i.id === a.item_id);
        if (!isMatched && !isApproved) throw err('origin_mismatch', 'item does not match this site');
        if (isApproved) {
          addUrlApprovals.set(approvalKey, { ts: Date.now() });
        }
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
        // Optional site rules (#188): parsed constraints only, validated and clamped here before the host sees them.
        const PR = root.GitpassPasswordRules || (typeof require === 'function' ? require('./lib/password-rules.js') : null);
        if (a.rules == null || !PR) {
          const r = await host('generate', { len: 20, digits: true, symbols: true });
          return { origin, password: r.password };
        }
        const s = PR.sanitize(a.rules);
        if (!s.ok) throw err('bad_request', 'password rules ' + s.reason);
        const r = await host('generate', { len: s.len, rules: { classes: s.classes } });
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
        // ---- save-card banner (#196) ----
        // The content script sends only { number, name, expiry }; sanitizeCardCapture
        // rebuilds exactly those three fields (dropping a CVV or anything else a
        // compromised page might stuff into the message) and re-checks Luhn here,
        // so the background never trusts the page's word for a valid card.
        if (kind === 'card') {
          const CB = root.GitpassBanner || (typeof require === 'function' ? require('./lib/banner.js') : null);
          if (!CB) throw err('bad_request', 'card helpers unavailable');
          const cap = CB.sanitizeCardCapture(a);
          if (!cap) throw err('bad_request', 'card');
          await waitInflight(tabId); // process this capture after the save that is still writing
          if (await isExceptedSite(origin)) {
            pendingSaves.delete(tabId);
            return { prompt: null };
          }
          if (await hasDuplicateCard(host, cap)) {
            pendingSaves.delete(tabId);
            return { prompt: null };
          }
          const masked = CB.cardMasked(cap.number);
          const brand = CB.cardBrand(cap.number);
          const title = CB.cardTitleDefault(cap.number);
          pendingSaves.set(tabId, { origin, kind: 'card', ...cap, prompt: 'save_card', masked, brand, title, ts: Date.now() });
          return { prompt: 'save_card', masked, brand, title, vaults: await vaultChoices(host) };
        }
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
        return p ? { prompt: p.prompt, username: p.username, title: p.title || null, masked: p.masked || null, brand: p.brand || null, vaults: await vaultChoices(host) } : { prompt: null };
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
          // save_card (#196): a card capture becomes a credit_card item through the
          // host's generic create_item path — the same ItemInput shape the popup's
          // New item form sends. Only number/cardholder/expiry are ever in
          // pendingSaves, so a CVV cannot reach the vault through this route.
          if (p.kind === 'card') {
            if (a.action !== 'save' || p.prompt !== 'save_card') throw err('bad_request', 'action');
            const title = saveTitle(a.title, p.origin);
            const vault_id = await saveVault(host, a.vault_id);
            const fields = [{ id: 'number', label: 'number', kind: 'concealed', value: p.number }];
            if (p.name) fields.push({ id: 'cardholder', label: 'cardholder name', kind: 'text', value: p.name });
            if (p.expiry) fields.push({ id: 'expiry', label: 'expiry date', kind: 'month_year', value: p.expiry });
            const item = { category: 'credit_card', title, url: p.origin, sections: [{ id: 'main', title: null, fields }] };
            if (vault_id) item.vaultId = vault_id;
            const r = await host('create_item', { item });
            if (pendingSaves.get(tabId) === p) pendingSaves.delete(tabId);
            rememberSaved(tabId, origin, r && r.item_id);
            return { done: true, action: 'save', item_id: r && r.item_id };
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

  function fromConfirmUi(msg, sender) {
    if (sender && !isConfirmUiSender(sender)) throw err('denied', 'unauthorized sender');
    const a = msg || {};
    if (typeof a.req !== 'string' || !a.req) throw err('bad_request', 'missing req');
    if (sender && typeof sender.url === 'string') {
      try {
        const u = new URL(sender.url);
        if (u.searchParams.get('req') !== a.req) {
          throw err('denied', 'mismatched request id');
        }
      } catch (e) {
        if (e && e.code) throw e;
        throw err('denied', 'invalid sender url');
      }
    }
    if (a.type === 'confirm-ui:request') {
      const c = pendingConfirms.get(a.req);
      if (!c || (Date.now() - c.ts > SHORT_TTL_MS)) {
        if (c) {
          if (c.cleanup) c.cleanup(); else pendingConfirms.delete(a.req);
          c.reject(err('cancelled', 'request expired'));
        }
        throw err('not_found', 'request expired');
      }
      return { pageHost: c.pageHost, itemHost: c.itemHost, title: c.title };
    }
    if (a.type === 'confirm-ui:decide') {
      const c = pendingConfirms.get(a.req);
      if (!c || (Date.now() - c.ts > SHORT_TTL_MS)) {
        if (c) {
          if (c.cleanup) c.cleanup(); else pendingConfirms.delete(a.req);
          c.reject(err('cancelled', 'request expired'));
        }
        throw err('not_found', 'request expired');
      }
      if (c.cleanup) c.cleanup(); else pendingConfirms.delete(a.req);
      if (a.approved) {
        crossSiteApprovals.set(`${c.tabId}:${c.origin}:${c.itemId}`, { ts: Date.now() });
        c.resolve({ approved: true });
        return { approved: true };
      } else {
        c.reject(err('cancelled', 'The user cancelled'));
        return { approved: false };
      }
    }
    throw err('bad_request', 'unknown confirm-ui message');
  }

  function forgetTab(tabId) {
    pendingSaves.delete(tabId);
    lastUsername.delete(tabId);
    lastSaved.delete(tabId);
    lastFilled.delete(tabId);
    inflightDecisions.delete(tabId);
    searchActive.delete(tabId);
    for (const k of crossSiteApprovals.keys()) {
      if (k.startsWith(`${tabId}:`)) crossSiteApprovals.delete(k);
    }
    for (const k of addUrlApprovals.keys()) {
      if (k.startsWith(`${tabId}:`)) addUrlApprovals.delete(k);
    }
    for (const [id, c] of pendingConfirms.entries()) {
      if (c.tabId === tabId) {
        if (c.cleanup) c.cleanup(); else pendingConfirms.delete(id);
        c.reject(err('cancelled', 'tab closed'));
      }
    }
  }
  if (root.chrome && chrome.tabs && chrome.tabs.onRemoved) chrome.tabs.onRemoved.addListener(forgetTab);
  if (root.chrome && chrome.windows && chrome.windows.onRemoved) {
    chrome.windows.onRemoved.addListener((winId) => {
      for (const [id, c] of pendingConfirms.entries()) {
        if (c.winId === winId) {
          if (c.cleanup) c.cleanup(); else pendingConfirms.delete(id);
          c.reject(err('cancelled', 'Confirmation window closed'));
        }
      }
    });
  }

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
    fromConfirmUi,
    isConfirmUiSender,
    _searchActive: searchActive,
    _crossSiteApprovals: crossSiteApprovals,
    _addUrlApprovals: addUrlApprovals,
    _pendingConfirms: pendingConfirms,
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
