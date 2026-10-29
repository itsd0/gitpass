// ===== gitpass background: toolbar icon state + match-count badge (issue #218) =====
// Loaded by background.js via importScripts (Chrome) / background.scripts (Firefox).
// The toolbar button shows: a muted icon + tooltip when gitpass is locked or the app is not running, and,
// when unlocked, a small badge with the number of logins for the active tab's origin (1-9, then "9+").
// Privacy: only a count is ever shown. Titles, usernames and secrets never reach the badge or tooltip.
// State is set per tab (chrome.action ... { tabId }) and cleared for every tab when the vault locks.
/* global chrome, GitpassPolicy */
(function (root) {
  'use strict';
  // Darker brand orange (same token as popup light theme): white text keeps >= 4.5:1 contrast in light and dark toolbars.
  const BADGE_COLOR = '#C2410C';
  const BADGE_TEXT_COLOR = '#FFFFFF';
  const ICON_NORMAL = 'icons/icon128.png';
  const ICON_MUTED = 'icons/icon128-locked.png';
  const DEBOUNCE_MS = 150;

  /** '' for no matches, '1'..'9', then '9+'. */
  function badgeText(count) {
    const n = Number(count);
    if (!Number.isFinite(n) || n < 1) return '';
    return n > 9 ? '9+' : String(Math.floor(n));
  }

  /**
   * Pure: state -> what the toolbar button shows.
   * state: { status: 'unlocked' | 'locked' | 'offline', enabled: bool, fillable: bool, count: number }
   * t(key, fallback) resolves the (localized) tooltip. Returns { text, color, title, iconVariant }.
   */
  function compute(state, t) {
    const tr = typeof t === 'function' ? t : (_k, d) => d;
    const s = state || {};
    if (s.status === 'locked') return { text: '', color: BADGE_COLOR, title: tr('statusAppLocked', 'gitpass is locked'), iconVariant: 'muted' };
    if (s.status !== 'unlocked') return { text: '', color: BADGE_COLOR, title: tr('badgeAppNotRunning', 'gitpass app not running'), iconVariant: 'muted' };
    const text = s.enabled !== false && s.fillable ? badgeText(s.count) : '';
    return { text, color: BADGE_COLOR, title: 'gitpass', iconVariant: 'normal' };
  }

  /** Host `status` answer (or the error thrown) -> 'unlocked' | 'locked' | 'offline'. */
  function statusOf(result, error) {
    if (error || !result) return 'offline';
    return result.unlocked ? 'unlocked' : 'locked';
  }

  /** Number of logins in a host `match` result (other categories are not fillable from the toolbar). */
  function loginCount(result) {
    const items = (result && result.items) || [];
    return items.filter((i) => i && (!i.category || i.category === 'login')).length;
  }

  /**
   * Controller. deps: { host(cmd,args), action (chrome.action), tabs (chrome.tabs), getEnabled(): Promise<bool>,
   * t(key, fallback), setTimeout, clearTimeout }. Nothing here throws to the caller.
   */
  function createController(deps) {
    const timers = new Map();
    const st = deps.setTimeout || setTimeout;
    const ct = deps.clearTimeout || clearTimeout;
    const quiet = (p) => { try { return Promise.resolve(p).catch(() => {}); } catch (_) { return Promise.resolve(); } };

    function paint(view, tabId) {
      const target = typeof tabId === 'number' ? { tabId } : {};
      const a = deps.action;
      return Promise.all([
        quiet(a.setBadgeText({ ...target, text: view.text })),
        quiet(a.setBadgeBackgroundColor({ ...target, color: view.color })),
        a.setBadgeTextColor ? quiet(a.setBadgeTextColor({ ...target, color: BADGE_TEXT_COLOR })) : null,
        quiet(a.setTitle({ ...target, title: view.title })),
        quiet(a.setIcon({ ...target, path: { 128: view.iconVariant === 'muted' ? ICON_MUTED : ICON_NORMAL } })),
      ]);
    }

    async function hostStatus() {
      try { return statusOf(await deps.host('status')); } catch (e) { return statusOf(null, e); }
    }

    /** Recompute and paint one tab. */
    async function update(tabId) {
      if (typeof tabId !== 'number') return null;
      let tab = null;
      try { tab = await deps.tabs.get(tabId); } catch (_) { return null; }
      const status = await hostStatus();
      const origin = tab && typeof tab.url === 'string' ? GitpassPolicy.originOf(tab.url) : null;
      const fillable = !!origin && GitpassPolicy.isFillableOrigin(origin);
      const enabled = await Promise.resolve(deps.getEnabled()).catch(() => true);
      let count = 0;
      if (status === 'unlocked' && enabled && fillable) {
        try { count = loginCount(await deps.host('match', { origin })); } catch (_) { count = 0; }
      }
      const view = compute({ status, enabled, fillable, count }, deps.t);
      await paint(view, tabId);
      return view;
    }

    /** Debounced update so rapid tab switching does not flood the host. */
    function schedule(tabId) {
      if (typeof tabId !== 'number') return;
      if (timers.has(tabId)) ct(timers.get(tabId));
      timers.set(tabId, st(() => { timers.delete(tabId); update(tabId).catch(() => {}); }, DEBOUNCE_MS));
    }

    async function activeTabId() {
      try { const [tab] = await deps.tabs.query({ active: true, currentWindow: true }); return tab && tab.id; } catch (_) { return undefined; }
    }

    /** Lock (or app gone): clear every tab's count at once, then repaint the active tab. */
    async function clearAll(status) {
      const view = compute({ status: status || 'locked' }, deps.t);
      await paint(view);
      let tabs = [];
      try { tabs = await deps.tabs.query({}); } catch (_) { /* ignore */ }
      await Promise.all(tabs.filter((tb) => typeof tb.id === 'number').map((tb) => paint(view, tb.id)));
      return view;
    }

    function onHostEvent(event) {
      if (event === 'lock') { clearAll('locked').catch(() => {}); return; }
      if (event === 'unlock' || event === 'changed') activeTabId().then(schedule);
    }

    function refreshActive() { return activeTabId().then(schedule); }

    return { update, schedule, clearAll, onHostEvent, refreshActive };
  }

  /** Wire the controller to the browser. Called once from background.js. */
  function install(deps) {
    const ctl = createController({
      host: deps.host,
      action: chrome.action,
      tabs: chrome.tabs,
      getEnabled: () => new Promise((res) => chrome.storage.local.get('settings', (r) => res(!((r && r.settings) && r.settings.showBadge === false)))),
      t: (key, fallback) => { try { return chrome.i18n.getMessage(key) || fallback; } catch (_) { return fallback; } },
    });
    if (!chrome.action) return ctl;
    chrome.tabs.onActivated.addListener((info) => ctl.schedule(info.tabId));
    chrome.tabs.onUpdated.addListener((tabId, info) => { if (info.status === 'complete') ctl.schedule(tabId); });
    if (chrome.storage && chrome.storage.onChanged) chrome.storage.onChanged.addListener((changes, area) => { if (area === 'local' && changes.settings) ctl.refreshActive(); });
    if (chrome.runtime.onStartup) chrome.runtime.onStartup.addListener(() => ctl.refreshActive());
    chrome.runtime.onInstalled.addListener(() => ctl.refreshActive());
    ctl.refreshActive();
    return ctl;
  }

  const api = { BADGE_COLOR, BADGE_TEXT_COLOR, ICON_NORMAL, ICON_MUTED, badgeText, compute, statusOf, loginCount, createController, install };
  root.GitpassBadge = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
