// ===== gitpass background: keyboard "fill-login" command (⌘⇧L / Ctrl+Shift+L, issue #124) =====
// Loaded by background.js via importScripts (Chrome) / background.scripts (Firefox).
// Fills the best-matching login on the active tab without opening the popup:
//  - exactly one login matches the tab's origin -> fill it through the same path as the popup's Fill
//    (background fillTab -> host `fill` with the origin -> content script re-checks location.origin);
//  - several match -> ask the content script to open its inline menu so the user picks one;
//  - no match, locked, or app not running -> ask the content script to show a short in-page notice.
// The origin always comes from the browser's tab URL. Non-http(s) pages (chrome://, the Web Store) are
// ignored. No new host commands; never opens a gitpass window or the popup.
/* global chrome, GitpassPolicy */
(function (root) {
  'use strict';
  const COMMAND = 'fill-login';
  const NOTICE_CODES = new Set(['locked', 'app_not_running', 'no_match', 'no_form', 'failed']);

  function errorCode(e) {
    const code = e && e.code;
    if (code === 'locked' || code === 'app_not_running') return code;
    const m = String((e && e.message) || e || '');
    if (/\blocked\b/.test(m)) return 'locked';
    if (/app_not_running|disconnect|native (messaging )?host/i.test(m)) return 'app_not_running';
    return 'failed';
  }

  /** Pure: what the command does for a host `match` result, or for the error the host answered. */
  function decide(result, error) {
    if (error) return { action: 'notice', code: errorCode(error) };
    const logins = ((result && result.items) || []).filter((i) => i && typeof i.id === 'string' && (!i.category || i.category === 'login'));
    if (logins.length === 1) return { action: 'fill', item_id: logins[0].id };
    if (logins.length > 1) return { action: 'menu', count: logins.length };
    return { action: 'notice', code: 'no_match' };
  }

  // Chrome blocks extensions on its Web Store pages; skip them up front rather than asking the host.
  const STORE = /^https:\/\/(chromewebstore\.google\.com|chrome\.google\.com\/webstore)(\/|$)/;

  /** Origin of a tab when the command may act on it, else null (chrome://, Web Store, file:, plain http). */
  function tabOrigin(tab) {
    if (!tab || typeof tab.id !== 'number' || typeof tab.url !== 'string' || STORE.test(tab.url)) return null;
    const origin = GitpassPolicy.originOf(tab.url);
    return origin && GitpassPolicy.isFillableOrigin(origin) ? origin : null;
  }

  /**
   * Run the command. deps: { activeTab(), host(cmd, args), fillTab(tabId, origin, itemId, extra),
   * sendToTab(tabId, msg) }. Resolves to the decision taken (for tests and e2e); never throws.
   */
  async function run(deps) {
    let tab;
    try { tab = await deps.activeTab(); } catch (_) { return { action: 'ignore' }; }
    const origin = tabOrigin(tab);
    if (!origin) return { action: 'ignore' };
    const toTab = (msg) => Promise.resolve().then(() => deps.sendToTab(tab.id, { ...msg, origin })).catch(() => {}); // no content script: nothing to show
    let d;
    try { d = decide(await deps.host('match', { origin })); } catch (e) { d = decide(null, e); }
    if (d.action === 'fill') {
      try { await deps.fillTab(tab.id, origin, d.item_id, { via: 'command' }); }
      catch (e) { d = decide(null, e); await toTab({ type: 'gitpass-command-notice', code: d.code }); }
    } else if (d.action === 'menu') {
      await toTab({ type: 'gitpass-command-menu' });
    } else {
      await toTab({ type: 'gitpass-command-notice', code: d.code });
    }
    return d;
  }

  const api = { COMMAND, NOTICE_CODES, decide, run, tabOrigin, errorCode };
  root.GitpassCommand = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
// ===== end fill-login command =====
