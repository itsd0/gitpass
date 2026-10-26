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
    const p = pending.get(msg.id);
    if (!p) return;
    pending.delete(msg.id);
    msg.ok ? p.resolve(msg.result) : p.reject(new Error(msg.error || 'host error'));
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

// Messages from the popup (extension pages) and content scripts.
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  (async () => {
    const fromExtensionPage = sender.id === chrome.runtime.id && !sender.tab;
    if (fromExtensionPage) {
      switch (msg.type) {
        case 'status': return host('status');
        case 'unlock': {
          // Secret Key goes popup -> background -> native host only; never stored, never sent to content scripts.
          const args = { password: msg.password };
          if (typeof msg.secret_key === 'string' && msg.secret_key.trim()) args.secret_key = msg.secret_key.trim();
          return host('unlock', args);
        }
        case 'lock': return host('lock');
        case 'search': return host('search', { query: msg.query });
        case 'totp': return host('totp', { item_id: msg.item_id });
        case 'generate': return host('generate', { len: msg.len });
        case 'matchActive': {
          const tab = await activeTab();
          const origin = tab && GitpassPolicy.originOf(tab.url);
          if (!origin || !GitpassPolicy.isFillableOrigin(origin)) return { items: [], origin: null };
          return { ...(await host('match', { url: origin })), origin };
        }
        case 'copyActive': {
          // Password goes only to the extension popup (for the clipboard), never to a page, and only for items matching the active tab.
          const tab = await activeTab();
          const origin = tab && GitpassPolicy.originOf(tab.url);
          if (!origin || !GitpassPolicy.isFillableOrigin(origin)) throw new Error('page is not https');
          const c = await host('fill', { item_id: msg.item_id, origin });
          return { password: c.password };
        }
        case 'fillActive': {
          const tab = await activeTab();
          return fillTab(tab.id, GitpassPolicy.originOf(tab.url), msg.item_id);
        }
      }
      throw new Error('unknown message');
    }
    // ---- ws4 (in-page): content-script messages are `page:*` and handled in bg-page.js (sender checks there) ----
    if (typeof msg.type === 'string' && msg.type.startsWith('page:')) return GitpassPage.handle(msg, sender, host);
    throw new Error('unknown message');
    // ---- end ws4 ----
  })().then((r) => sendResponse({ ok: true, result: r }), (e) => sendResponse({ ok: false, error: String(e.message || e), code: e.code || null }));
  return true; // async
});
