// gitpass background: the only component that talks to the native host.
/* global chrome, GitpassPolicy, importScripts */
'use strict';
if (typeof importScripts === 'function' && !globalThis.GitpassPolicy) importScripts('lib/policy.js');
// ws4 (in-page): page:* routing lives in bg-page.js
if (typeof importScripts === 'function' && !globalThis.GitpassPasswordRules) importScripts('lib/password-rules.js');
// Card helpers for the page:capture card branch in bg-page.js (also loaded by content.js).
if (typeof importScripts === 'function' && !globalThis.GitpassBanner) importScripts('lib/banner.js');
if (typeof importScripts === 'function' && !globalThis.GitpassPage) importScripts('bg-page.js');
// onboarding (welcome/options/commands); Firefox loads it via manifest.firefox.json background.scripts
if (typeof importScripts === 'function') importScripts('bg-onboard.js');
// v0.7 passkeys (approval window + host passkey_create/get)
if (typeof importScripts === 'function' && !globalThis.GitpassPasskey) importScripts('bg-passkey.js');
// ⌘⇧L / Ctrl+Shift+L "fill-login" command (#124)
if (typeof importScripts === 'function' && !globalThis.GitpassCommand) importScripts('bg-command.js');

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
      if (msg.event === 'lock' || msg.event === 'unlock' || msg.event === 'changed') chrome.runtime.sendMessage({ type: 'popup:event', event: msg.event }).catch(() => {});
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

// Auto-lock on screen lock / system sleep via chrome.idle
if (typeof chrome !== 'undefined' && chrome.idle && chrome.idle.onStateChanged) {
  chrome.idle.onStateChanged.addListener((state) => {
    if (state === 'locked') {
      host('lock').catch(() => {});
      chrome.runtime.sendMessage({ type: 'popup:event', event: 'lock', reason: 'screen' }).catch(() => {});
    } else if (state === 'idle') {
      host('status').catch(() => {});
    }
  });
}

async function activeTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab;
}

/** Fill creds into the top frame of a tab; the content script re-checks the origin. */
async function fillTab(tabId, origin, itemId, extra) {
  if (!GitpassPolicy.isFillableOrigin(origin)) throw new Error('refusing to fill: page is not https');
  const creds = await host('fill', { item_id: itemId, origin }); // host enforces origin match
  const via = extra && extra.via === 'command' ? { via: 'command' } : {};
  if (globalThis.GitpassPage) GitpassPage.rememberFilled(tabId, origin, itemId); // ids only: offers the one-time code on the next page
  await chrome.tabs.sendMessage(tabId, { type: 'gitpass-fill', origin, username: creds.username, password: creds.password, ...via }, { frameId: 0 });
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
      const a = {}; for (const k of ['vault_id', 'category', 'query', 'limit', 'offset']) if (msg[k] != null) a[k] = msg[k];
      return host('list_items', a);
    }
    case 'match_active': {
      const { origin } = await activeOrigin();
      if (!origin) return { items: [], origin: null };
      return { ...(await host('match', { origin })), origin };
    }
    case 'get_item': return host('get_item', { item_id: msg.item_id });
    case 'item_findings': return host('item_findings', { item_id: msg.item_id });
    case 'reveal': return host('reveal', { item_id: msg.item_id, field: msg.field, source: 'popup' });
    case 'totp': return host('totp', { item_id: msg.item_id });
    case 'generate': return host('generate', { len: msg.len, digits: !!msg.digits, symbols: !!msg.symbols, words: !!msg.words, pin: !!msg.pin });
    case 'create_item': {
      const it = msg.item && typeof msg.item === 'object' ? msg.item : null;
      if (!it || typeof it.category !== 'string' || it.category === 'passkey') throw Object.assign(new Error('bad item'), { code: 'bad_request' });
      return host('create_item', { item: it });
    }
    case 'update_login': {
      const args = { item_id: msg.item_id, source: 'popup' }; // extension pages only; content scripts can't reach popup:*
      for (const k of ['title', 'username', 'password', 'notes']) if (msg[k] != null) args[k] = String(msg[k]);
      if (Array.isArray(msg.urls)) args.urls = msg.urls.map(String);
      return host('update_login', args);
    }
    case 'update_password': return host('update_password', { item_id: msg.item_id, password: msg.password, username: msg.username });
    case 'save_login': return host('save_login', { origin: msg.origin, title: msg.title, username: msg.username, password: msg.password, vault_id: msg.vault_id });
    case 'set_favorite': return host('set_favorite', { item_id: msg.item_id, favorite: !!msg.favorite });
    case 'open_in_app': return host('open_in_app', msg.item_id ? { item_id: msg.item_id } : {});
    case 'can_fill_generated': return canFillGenerated();
    case 'fill_generated': return fillGenerated(msg.password);
    case 'open_fill': return openFill(msg);
  }
  throw new Error('unknown popup message');
}

/** Whether the active page has a focused password field (the generator's Fill action). */
async function canFillGenerated() {
  const { tab, origin } = await activeOrigin();
  if (!tab || !origin) return { canFill: false };
  try {
    const r = await chrome.tabs.sendMessage(tab.id, { type: 'gitpass-can-fill-generated', origin }, { frameId: 0 });
    return { canFill: !!(r && r.canFill) };
  } catch (_) {
    return { canFill: false }; // no content script on this page (chrome://, Web Store, ...)
  }
}

/** Fill a generated password into the focused password field. No vault item is involved. */
async function fillGenerated(password) {
  const { tab, origin } = await activeOrigin();
  if (!tab || !origin) throw Object.assign(new Error('page is not https'), { code: 'bad_request' });
  if (typeof password !== 'string' || !password || password.length > 1024) throw Object.assign(new Error('password is required'), { code: 'bad_request' });
  const r = await chrome.tabs.sendMessage(tab.id, { type: 'gitpass-fill-generated', origin, password }, { frameId: 0 });
  if (!r || !r.filled) throw Object.assign(new Error('no focused password field'), { code: 'bad_request' });
  return { filled: true };
}

/** Open & Fill: fill the active tab if it matches; for logins with a URL otherwise open that URL first. */async function openFill(msg) {
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
  if (msg.kind === 'card') await GitpassPage.fillPaymentFrames(tab.id, fields).catch(() => 0); // Stripe-style card iframes
  return true;
}

// ⌘⇧L / Ctrl+Shift+L: fill the best-matching login on the active tab without opening the popup (#124).
function fillLoginCommand() {
  return GitpassCommand.run({
    activeTab,
    host,
    fillTab,
    sendToTab: (tabId, msg) => chrome.tabs.sendMessage(tabId, msg, { frameId: 0 }),
  });
}
globalThis.gitpassFillLoginCommand = fillLoginCommand; // e2e drives the command from the service worker
if (chrome.commands && chrome.commands.onCommand) {
  chrome.commands.onCommand.addListener((cmd) => { if (cmd === GitpassCommand.COMMAND) fillLoginCommand().catch(() => {}); });
}

// Messages from the popup (extension pages) and content scripts.
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  (async () => {
    if (typeof msg.type === 'string' && msg.type.startsWith('passkey-ui:')) { if (!GitpassPasskey.isUiSender(sender)) throw new Error('unknown message'); return GitpassPasskey.fromUi(msg, host); }
    if (typeof msg.type === 'string' && msg.type.startsWith('confirm-ui:')) {
      if (!GitpassPage.isConfirmUiSender(sender)) throw new Error('unknown message');
      return GitpassPage.fromConfirmUi(msg, sender);
    }
    if (typeof msg.type === 'string' && msg.type.startsWith('passkey:') && sender.tab) return GitpassPasskey.fromPage(msg, sender, host);
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
