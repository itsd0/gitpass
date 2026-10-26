// gitpass background: the only component that talks to the native host.
/* global chrome, GitpassPolicy, importScripts */
'use strict';
if (typeof importScripts === 'function' && !globalThis.GitpassPolicy) importScripts('lib/policy.js');
// ws4 (in-page): page:* routing lives in bg-page.js
if (typeof importScripts === 'function' && !globalThis.GitpassPage) importScripts('bg-page.js');

const HOST = 'com.gitpass.host';
let port = null;
let nextId = 1;
const pending = new Map();

function connect() {
  if (port) return port;
  port = chrome.runtime.connectNative(HOST);
  port.onMessage.addListener((msg) => {
    // App lock/unlock events (forwarded by the host) -> open popups. No popup open is fine.
    if (msg && msg.event && msg.id == null) {
      if (msg.event === 'lock' || msg.event === 'unlock') chrome.runtime.sendMessage({ type: 'popup:event', event: msg.event }).catch(() => {});
      return;
    }
    const p = pending.get(msg.id);
    if (!p) return;
    pending.delete(msg.id);
    msg.ok ? p.resolve(msg.result) : p.reject(Object.assign(new Error(msg.message || msg.error || 'host error'), { code: msg.error }));
  });
  port.onDisconnect.addListener(() => {
    const err = new Error((chrome.runtime.lastError && chrome.runtime.lastError.message) || 'native host disconnected');
    for (const p of pending.values()) p.reject(err);
    pending.clear();
    port = null;
  });
  return port;
}

function host(cmd, args = {}) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    try { connect().postMessage({ id, cmd, ...args }); } catch (e) { pending.delete(id); reject(e); }
  });
}

async function activeTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab;
}

/** Fill creds into the top frame of a tab; the content script re-checks the origin. */
async function fillTab(tabId, origin, itemId) {
  if (!GitpassPolicy.isFillableOrigin(origin)) throw new Error('refusing to fill: page is not https');
  const creds = await host('fill', { item_id: itemId, origin }); // host enforces origin match
  await chrome.tabs.sendMessage(tabId, { type: 'gitpass-fill', origin, username: creds.username, password: creds.password }, { frameId: 0 });
  return true;
}

async function activeOrigin() {
  const tab = await activeTab();
  const origin = tab && GitpassPolicy.originOf(tab.url);
  return { tab, origin: origin && GitpassPolicy.isFillableOrigin(origin) ? origin : null };
}

function waitTabComplete(tabId, timeoutMs = 15000) {
  return new Promise((resolve, reject) => {
    const to = setTimeout(() => { chrome.tabs.onUpdated.removeListener(l); reject(new Error('page load timed out')); }, timeoutMs);
    function l(id, info, tab) { if (id === tabId && info.status === 'complete') { clearTimeout(to); chrome.tabs.onUpdated.removeListener(l); resolve(tab); } }
    chrome.tabs.onUpdated.addListener(l);
  });
}

/** Popup-facing routes. Every popup message type is `popup:<cmd>` (in-page uses `page:`). */
async function popupRoute(msg) {
  const cmd = msg.type.slice('popup:'.length);
  switch (cmd) {
    case 'status': return host('status');
    case 'request_unlock': return host('request_unlock', { reason: String(msg.reason || 'Unlock gitpass browser extension') });
    case 'unlock': { // standalone fallback (app not running): password + Secret Key, memory only in the host
      const args = { password: msg.password };
      if (typeof msg.secret_key === 'string' && msg.secret_key.trim()) args.secret_key = msg.secret_key.trim();
      return host('unlock', args);
    }
    case 'lock': return host('lock');
    case 'list_vaults': return host('list_vaults');
    case 'list_items': {
      const a = {}; for (const k of ['vault_id', 'category', 'query', 'limit']) if (msg[k] != null) a[k] = msg[k];
      return host('list_items', a);
    }
    case 'match_active': {
      const { origin } = await activeOrigin();
      if (!origin) return { items: [], origin: null };
      return { ...(await host('match', { origin })), origin };
    }
    case 'get_item': return host('get_item', { item_id: msg.item_id });
    case 'reveal': return host('reveal', { item_id: msg.item_id, field: msg.field, source: 'popup' });
    case 'totp': return host('totp', { item_id: msg.item_id });
    case 'generate': return host('generate', { len: msg.len, digits: !!msg.digits, symbols: !!msg.symbols });
    case 'save_login': return host('save_login', { origin: msg.origin, title: msg.title, username: msg.username, password: msg.password, vault_id: msg.vault_id });
    case 'set_favorite': return host('set_favorite', { item_id: msg.item_id, favorite: !!msg.favorite }); // NOTE: not in contract v0.6 command list
    case 'open_in_app': return host('open_in_app', msg.item_id ? { item_id: msg.item_id } : {});
    case 'open_fill': return openFill(msg);
  }
  throw new Error('unknown popup message');
}

/** Open & Fill: fill the active tab if it matches; for logins with a URL otherwise open that URL first. */
async function openFill(msg) {
  let { tab, origin } = await activeOrigin();
  if (msg.kind === 'login') {
    const want = msg.url && GitpassPolicy.originOf(msg.url);
    if (!origin || (want && origin !== want)) {
      if (!want || !GitpassPolicy.isFillableOrigin(want)) throw new Error('page is not https');
      // Let the host decide eTLD+1 matching; only navigate when the current page doesn't match.
      const m = origin ? await host('match', { origin }).catch(() => ({ items: [] })) : { items: [] };
      if (!m.items.some((i) => i.id === msg.item_id)) {
        tab = await chrome.tabs.create({ url: msg.url, active: true });
        tab = await waitTabComplete(tab.id);
        origin = GitpassPolicy.originOf(tab.url);
      }
    }
    return fillTab(tab.id, origin, msg.item_id);
  }
  if (!origin) throw new Error('page is not https');
  const fields = await host(msg.kind === 'card' ? 'fill_card' : 'fill_identity', { item_id: msg.item_id, source: 'popup' });
  // Content side (workstream 4) handles `gitpass-fill-fields`.
  await chrome.tabs.sendMessage(tab.id, { type: 'gitpass-fill-fields', kind: msg.kind, origin, fields }, { frameId: 0 });
  return true;
}

// Messages from the popup (extension pages) and content scripts.
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  (async () => {
    const fromExtensionPage = sender.id === chrome.runtime.id && !sender.tab;
    if (fromExtensionPage) {
      if (typeof msg.type === 'string' && msg.type.startsWith('popup:')) return popupRoute(msg);
      throw new Error('unknown message');
    }
    // ---- ws4 (in-page): content-script messages are `page:*` and handled in bg-page.js (sender checks there) ----
    if (typeof msg.type === 'string' && msg.type.startsWith('page:')) return GitpassPage.handle(msg, sender, host);
    throw new Error('unknown message');
    // ---- end ws4 ----
  })().then((r) => sendResponse({ ok: true, result: r }), (e) => sendResponse({ ok: false, error: String(e.message || e), code: e.code || null }));
  return true; // async
});
