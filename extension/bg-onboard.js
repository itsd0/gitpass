// gitpass onboarding/background add-on. Loaded by background.js via importScripts('bg-onboard.js')
// (Chrome) or listed in manifest.firefox.json background.scripts (Firefox).
/* global chrome */
'use strict';
(function () {
  const HOST = 'com.gitpass.host';
  const DEFAULTS = { autolockFollowsApp: true, autofill: true, savePrompts: true };
  const api = globalThis.browser || chrome;

  // Open the welcome page on first install only.
  chrome.runtime.onInstalled.addListener((d) => {
    if (d.reason === 'install') chrome.tabs.create({ url: chrome.runtime.getURL('welcome/welcome.html') });
    chrome.storage.local.get('settings', (r) => {
      chrome.storage.local.set({ settings: Object.assign({}, DEFAULTS, (r && r.settings) || {}) });
    });
  });

  // Cmd/Ctrl+Shift+X: "_execute_action" opens the popup natively; this handles any extra command.
  if (chrome.commands && chrome.commands.onCommand) {
    chrome.commands.onCommand.addListener((cmd) => {
      if (cmd === 'open-popup' && chrome.action && chrome.action.openPopup) chrome.action.openPopup().catch(() => {});
    });
  }

  // One-shot host probe for the welcome/options pages (independent of background.js's port).
  function probe() {
    return new Promise((resolve) => {
      let done = false;
      const t = setTimeout(() => { if (!done) { done = true; resolve({ host: false, error: 'timeout' }); } }, 5000);
      try {
        chrome.runtime.sendNativeMessage(HOST, { id: 1, cmd: 'status' }, (resp) => {
          if (done) return; done = true; clearTimeout(t);
          const err = chrome.runtime.lastError;
          if (err || !resp) return resolve({ host: false, error: err ? err.message : 'no response' });
          if (!resp.ok) return resolve({ host: true, app: resp.error !== 'app_not_running', error: resp.error });
          resolve({ host: true, app: resp.result.app_running !== false, unlocked: !!resp.result.unlocked, status: resp.result });
        });
      } catch (e) { done = true; clearTimeout(t); resolve({ host: false, error: String(e.message || e) }); }
    });
  }

  // Own port channel ("gitpass-onboard") so background.js's onMessage router (which treats any
  // tab sender as a content script) never answers these first.
  async function handle(msg) {
    switch (msg && msg.type) {
      case 'onboard-probe': return probe();
      case 'onboard-open-app':
        return new Promise((res) => chrome.runtime.sendNativeMessage(HOST, { id: 2, cmd: 'open_in_app' }, (r) => { void chrome.runtime.lastError; res(r || null); }));
      case 'onboard-shortcut': {
        const cmds = chrome.commands ? await chrome.commands.getAll() : [];
        const c = cmds.find((x) => x.name === '_execute_action' || x.name === 'open-popup');
        return { shortcut: (c && c.shortcut) || '' };
      }
      case 'onboard-pinned': {
        const s = chrome.action && chrome.action.getUserSettings ? await chrome.action.getUserSettings() : null;
        return { pinned: s ? !!s.isOnToolbar : null };
      }
    }
    throw new Error('unknown onboard message');
  }
  chrome.runtime.onConnect.addListener((port) => {
    if (port.name !== 'gitpass-onboard') return;
    const s = port.sender || {};
    if (s.id !== chrome.runtime.id || !String(s.url || '').startsWith(chrome.runtime.getURL(''))) { port.disconnect(); return; }
    port.onMessage.addListener((m) => {
      handle(m).then((r) => port.postMessage({ rid: m.rid, ok: true, result: r }), (e) => port.postMessage({ rid: m.rid, ok: false, error: String(e.message || e) }));
    });
  });
  void api;
})();
