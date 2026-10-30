// Shared helpers for welcome/options pages.
/* global chrome */
'use strict';
(function (root) {
  let port = null, rid = 0; const waiting = new Map();
  function getPort() {
    if (port) return port;
    port = chrome.runtime.connect({ name: 'gitpass-onboard' });
    port.onMessage.addListener((r) => { const w = waiting.get(r.rid); if (!w) return; waiting.delete(r.rid); r.ok ? w.res(r.result) : w.rej(new Error(r.error)); });
    port.onDisconnect.addListener(() => { void chrome.runtime.lastError; for (const w of waiting.values()) w.rej(new Error('disconnected (is bg-onboard.js loaded?)')); waiting.clear(); port = null; });
    return port;
  }
  function send(msg) {
    return new Promise((res, rej) => {
      const id = ++rid; waiting.set(id, { res, rej });
      setTimeout(() => { if (waiting.delete(id)) rej(new Error('timeout (is bg-onboard.js loaded?)')); }, 8000);
      try { getPort().postMessage({ ...msg, rid: id }); } catch (e) { waiting.delete(id); rej(e); }
    });
  }
  const DEFAULTS = { autolockFollowsApp: true, autofill: true, savePrompts: true, autoFillTotp: false, showBadge: true, autoSubmitAfterFill: false };
  function getSettings() { return new Promise((res) => chrome.storage.local.get('settings', (r) => res(Object.assign({}, DEFAULTS, (r && r.settings) || {})))); }
  function setSettings(patch) { return getSettings().then((s) => new Promise((res) => { const n = Object.assign(s, patch); chrome.storage.local.set({ settings: n }, () => res(n)); })); }
  function getNeverSave() {
    return new Promise((res) => {
      chrome.storage.local.get(['neverSave', 'neverSaveSites'], (r) => {
        const list = (r && (r.neverSave || r.neverSaveSites)) || [];
        res(Array.isArray(list) ? list : []);
      });
    });
  }
  function setNeverSave(list) {
    return new Promise((res) => {
      chrome.storage.local.set({ neverSave: list }, () => res(list));
    });
  }
  function removeNeverSave(domain) {
    const dom = (domain || '').trim().toLowerCase();
    return getNeverSave().then((list) => {
      const filtered = list.filter((d) => (d || '').toLowerCase() !== dom);
      return setNeverSave(filtered);
    });
  }
  // Per-site "Never auto-submit here" list for issue #231. Same shape as neverSave, separate key.
  function getNeverAutoSubmit() {
    return new Promise((res) => {
      chrome.storage.local.get(['neverAutoSubmit'], (r) => {
        const list = (r && r.neverAutoSubmit) || [];
        res(Array.isArray(list) ? list : []);
      });
    });
  }
  function setNeverAutoSubmit(list) {
    return new Promise((res) => {
      chrome.storage.local.set({ neverAutoSubmit: list }, () => res(list));
    });
  }
  // Per-site "Hide inline menu here" list for issue #341. Same shape as neverAutoSubmit, separate key.
  function getHiddenInlineMenu() {
    return new Promise((res) => {
      chrome.storage.local.get(['hiddenInlineMenu'], (r) => {
        const list = (r && r.hiddenInlineMenu) || [];
        res(Array.isArray(list) ? list : []);
      });
    });
  }
  function setHiddenInlineMenu(list) {
    return new Promise((res) => {
      chrome.storage.local.set({ hiddenInlineMenu: list }, () => res(list));
    });
  }
  root.GitpassOnboard = { send, getSettings, setSettings, getNeverSave, setNeverSave, removeNeverSave, getNeverAutoSubmit, setNeverAutoSubmit, getHiddenInlineMenu, setHiddenInlineMenu, DEFAULTS };
})(globalThis);
