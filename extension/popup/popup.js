/* global chrome, GitpassI18n, GitpassPopupIcons, GitpassView, GitpassNewItem */
'use strict';
// gitpass popup v0.6: 2-pane UI. All host calls go through background.js as `popup:*` messages.
const $ = (id) => document.getElementById(id);
const t = (k, f, subs) => GitpassI18n.t(k, f, subs);
const V = typeof GitpassView !== 'undefined' ? GitpassView : (typeof require !== 'undefined' ? require('./view.js') : null);
const { UI, categoryIcon } = typeof GitpassPopupIcons !== 'undefined' ? GitpassPopupIcons : (typeof require !== 'undefined' ? require('./icons.js') : { UI: {} });
const ROW_H = 52;
const ROW_H_COMPACT = 36;
// Row height is density-aware (#397): the virtualized list's spacer padding and
// scroll-into-view math must match the real CSS row height.
function rowHeight() {
  const list = $('list');
  return (list && list.classList.contains('compact')) ? ROW_H_COMPACT : ROW_H;
}

// ---------- password generator (issue #68) ----------
const G = typeof GitpassGenerator !== 'undefined' ? GitpassGenerator : (typeof require !== 'undefined' ? require('./generator.js') : null);
const GEN_MODES = G.MODES;
const GEN_DEFAULTS = G.DEFAULTS;
const WORD_SEPARATORS = G.WORD_SEPARATORS;
const genMode = G.mode;
const clampGenLength = G.clampLength;
const safeGenSettings = G.safeSettings;
const strengthScore = G.strengthScore;
const strengthKey = G.strengthKey;

/** Last used mode/length/toggles (extension storage); the generated value itself is never stored. */
function generatorSettings() {
  if (!globalThis.chrome || !chrome.storage || !chrome.storage.local) return Promise.resolve(safeGenSettings({ ...GEN_DEFAULTS, ...(window.__gitpassMockGenSettings || {}) }));
  return new Promise((res) => {
    try {
      chrome.storage.local.get('generator', (r) => res(safeGenSettings({ ...GEN_DEFAULTS, ...((r && r.generator) || {}) })));
    } catch (_) { res(safeGenSettings(GEN_DEFAULTS)); }
  });
}

function saveGeneratorSettings(s) {
  const clean = safeGenSettings(s);
  if (!globalThis.chrome || !chrome.storage || !chrome.storage.local) { window.__gitpassMockGenSettings = clean; return Promise.resolve(clean); }
  return new Promise((res) => {
    try { chrome.storage.local.set({ generator: clean }, () => res(clean)); } catch (_) { res(clean); }
  });
}

function send(msg, timeoutMs) {
  const ms = typeof timeoutMs === 'number'
    ? timeoutMs
    : (msg && typeof msg.__timeoutMs === 'number'
        ? msg.__timeoutMs
        : (V && V.timeoutForMessage ? V.timeoutForMessage(msg) : (
            (msg && (msg.type === 'popup:request_unlock' || msg.type === 'popup:open_in_app' || msg.type === 'popup:open_fill'))
              ? 130000
              : (V ? V.REQUEST_TIMEOUT_MS : 8000)
          )));
  const p = window.__gitpassMock ? window.__gitpassMock(msg) : new Promise((res, rej) => chrome.runtime.sendMessage(msg, (r) => {
    if (chrome.runtime.lastError) return rej(new Error(chrome.runtime.lastError.message));
    if (r && r.ok) return res(r.result);
    const e = new Error((r && r.error) || 'error'); e.code = r && r.code; rej(e);
  }));
  return V && V.withTimeout ? V.withTimeout(p, ms, t('errTimeout', 'Request timed out.')) : p;
}
function el(tag, cls, text) { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; }
function iconBtn(svg, title, onclick) {
  const b = el('button', 'icon-btn'); b.type = 'button'; b.innerHTML = svg; b.title = title;
  b.onclick = (e) => { e.stopPropagation(); Promise.resolve(onclick(b)).catch(showErr); }; return b;
}
let toastT = null;
function toast(msg, ms = 1600) {
  const e = $('toast'); if (!e) return;
  clearTimeout(toastT);
  e.textContent = msg;
  e.classList.remove('hidden');
  toastT = setTimeout(() => { e.classList.add('hidden'); e.textContent = ''; }, ms);
}
/** Toast with one action button (Undo after moving an item to Recently Deleted). */
function toastAction(message, label, onAction, ms = 6000) {
  const e = $('toast'); if (!e) return;
  clearTimeout(toastT);
  e.textContent = '';
  const b = el('button', 'toast-action', label);
  b.type = 'button';
  b.onclick = async () => {
    if (b.disabled) return;
    b.disabled = true;
    clearTimeout(toastT);
    e.classList.add('hidden');
    e.textContent = '';
    try { await onAction(); } catch (err) { showErr(err); }
  };
  e.append(el('span', 'toast-msg', message), document.createTextNode(' · '), b);
  e.classList.remove('hidden');
  toastT = setTimeout(() => { e.classList.add('hidden'); e.textContent = ''; }, ms);
}
// Host errors carry a contract code (locked/denied/not_found/origin_mismatch/bad_request/app_not_running/internal).
function errText(e) {
  const c = e && e.code;
  const M = {
    locked: t('errLocked', 'gitpass is locked. Unlock it to continue.'),
    denied: t('errDenied', 'gitpass refused this request.'),
    not_found: t('errNotFound', 'Item not found.'),
    origin_mismatch: t('errOrigin', 'This item does not match the current site.'),
    app_not_running: t('errAppDown', 'The gitpass app is not running.'),
    timeout: t('errTimeout', 'Request timed out.'),
  };
  return (c && M[c]) || String((e && e.message) || e);
}
function showErr(e) { if (e && e.code === 'locked') { refresh(); return; } toast(errText(e)); }
let clipTimer = null;
let lastCopied = null;

async function clearClipboardIfOurs() {
  if (!lastCopied) return false;
  const token = lastCopied;
  lastCopied = null;
  if (clipTimer) { clearTimeout(clipTimer); clipTimer = null; }
  if (window.__gitpassMock) {
    if (window.__gitpassMockClipboard === token) {
      window.__gitpassMockClipboard = '';
      return true;
    }
    return false;
  }
  try {
    if (navigator.clipboard && navigator.clipboard.readText) {
      const cur = await navigator.clipboard.readText();
      if (cur === token) {
        await navigator.clipboard.writeText('');
        return true;
      }
    }
  } catch (_) {}
  return false;
}

async function copy(text, label, clearSecs = 30) {
  lastCopied = text;
  if (clipTimer) clearTimeout(clipTimer);
  if (window.__gitpassMock) {
    window.__gitpassMockClipboard = text;
    toast(label || t('copied', 'Copied'));
  } else {
    await navigator.clipboard.writeText(text);
    toast(label || t('copied', 'Copied'));
  }
  if (clearSecs > 0) {
    clipTimer = setTimeout(() => {
      clearClipboardIfOurs();
    }, clearSecs * 1000);
  }
}
globalThis.copy = copy;
globalThis.clearClipboardIfOurs = clearClipboardIfOurs;
function show(view) {
  if (view === 'locked') clearClipboardIfOurs();
  for (const v of ['nohost', 'locked', 'unlocked']) { const e = $(v); if (e) e.classList.toggle('hidden', v !== view); }
}

// ---------- state ----------
const S = { all: [], suggested: [], origin: null, vaults: [], filter: 'suggested', sort: 'updated', query: '', rows: [], sel: -1, detail: null, revealed: {}, findingsCache: {}, iconCache: new Map(), iconPending: new Set(), totpCache: {}, totpFetching: {}, totpGen: 0, totpTimer: null, listTotpTimer: null, panel: null, view: null, neverSave: [], edit: null };

async function refreshNeverSave() {
  if (window.__gitpassMockNeverSave) {
    S.neverSave = window.__gitpassMockNeverSave;
    return S.neverSave;
  }
  return new Promise((res) => {
    try {
      if (globalThis.chrome && chrome.storage && chrome.storage.local) {
        chrome.storage.local.get(['neverSave', 'neverSaveSites'], (r) => {
          const list = (r && (r.neverSave || r.neverSaveSites)) || [];
          S.neverSave = Array.isArray(list) ? list : [];
          res(S.neverSave);
        });
      } else {
        S.neverSave = S.neverSave || [];
        res(S.neverSave);
      }
    } catch (_) {
      S.neverSave = S.neverSave || [];
      res(S.neverSave);
    }
  });
}

async function removeNeverSave(domain) {
  const dom = (domain || '').trim().toLowerCase();
  if (window.__gitpassMockNeverSave) {
    window.__gitpassMockNeverSave = window.__gitpassMockNeverSave.filter((d) => (d || '').toLowerCase() !== dom);
    S.neverSave = window.__gitpassMockNeverSave;
    return;
  }
  return new Promise((res) => {
    try {
      if (globalThis.chrome && chrome.storage && chrome.storage.local) {
        chrome.storage.local.get(['neverSave', 'neverSaveSites'], (r) => {
          const list = (r && (r.neverSave || r.neverSaveSites)) || [];
          const next = list.filter((d) => (d || '').toLowerCase() !== dom);
          chrome.storage.local.set({ neverSave: next }, () => {
            S.neverSave = next;
            res();
          });
        });
      } else {
        S.neverSave = (S.neverSave || []).filter((d) => (d || '').toLowerCase() !== dom);
        res();
      }
    } catch (_) {
      res();
    }
  });
}

function isSiteExcepted() {
  if (!S.origin || !S.neverSave || !S.neverSave.length) return false;
  return GitpassPolicy.isNeverSave(S.origin, S.neverSave);
}

// ---------- top-level ----------
// ---------- Watchtower shield severity dot (issue #390) ----------
// Background-only: never blocks the item list, never fires while locked, and a failed
// audit_summary just leaves the shield plain. The dot re-checks after live vault changes
// and after the Watchtower panel closes.
let wtBadgeGen = 0;
function refreshWatchtowerBadge() {
  const btn = $('watchtowerBtn');
  if (!btn || S.view !== 'unlocked-loaded') return;
  const gen = ++wtBadgeGen;
  send({ type: 'popup:audit_summary' }).then((res) => {
    if (gen !== wtBadgeGen || S.view !== 'unlocked-loaded') return; // stale or locked meanwhile
    const findings = (res && Array.isArray(res.findings)) ? res.findings : [];
    let worst = null; // 'danger' | 'warn' | null
    const ids = new Set();
    for (const f of findings) {
      const sev = V.findingSeverity(f && f.kind);
      if (sev === 'danger') worst = 'danger';
      else if (!worst) worst = 'warn';
      for (const it of (f && Array.isArray(f.items) ? f.items : [])) ids.add(it && it.id);
    }
    btn.classList.toggle('wt-dot-danger', worst === 'danger');
    btn.classList.toggle('wt-dot-warn', worst === 'warn' && !btn.classList.contains('wt-dot-danger'));
    if (worst) {
      const n = ids.size || findings.reduce((a, f) => a + (Number(f && f.count) || 0), 0);
      const label = t('watchtowerAttention', 'Watchtower: $1 items need attention', [n]);
      btn.setAttribute('aria-label', label);
      btn.title = label;
    } else {
      btn.setAttribute('aria-label', t('watchtower', 'Watchtower'));
      btn.title = t('watchtower', 'Watchtower');
    }
  }).catch(() => { /* host unreachable or audit failed: plain shield, no error surface */ });
}
function clearWatchtowerBadge() {
  wtBadgeGen++;
  const btn = $('watchtowerBtn');
  if (!btn) return;
  btn.classList.remove('wt-dot-danger', 'wt-dot-warn');
  btn.setAttribute('aria-label', t('watchtower', 'Watchtower'));
  btn.title = t('watchtower', 'Watchtower');
}

// List density (#397): 'comfortable' (default) or 'compact', stored in the shared
// extension settings. Applied as a class on the list container; storage changes
// apply instantly without reopening the popup.
function applyDensity(d) {
  const list = $('list');
  if (!list) return;
  const compact = d === 'compact';
  if (list.classList.contains('compact') === compact) return;
  list.classList.toggle('compact', compact);
  // Row height changed: re-render so the virtualized window and spacer padding
  // are recomputed in the new units.
  if (S.rows.length) renderList();
}
function loadDensity() {
  if (typeof window !== 'undefined' && window.__gitpassMockDensity) { applyDensity(window.__gitpassMockDensity); return Promise.resolve(); }
  return popupSettings().then((st) => applyDensity(st && st.density)).catch(() => {});
}
if (globalThis.chrome && chrome.storage && chrome.storage.onChanged) {
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area !== 'local' || !changes.settings) return;
    const d = changes.settings.newValue && changes.settings.newValue.density;
    applyDensity(d);
  });
}

async function refresh() {
  let st = null, err = null;
  try { st = await send({ type: 'popup:status' }); } catch (e) { err = e; }
  const view = V.viewFor(st, err);
  S.view = view;
  if (view === 'nohost') { clearWatchtowerBadge(); return showNoHost(err); }
  if (view === 'locked' || view === 'appdown') { clearWatchtowerBadge(); return showLocked(view, st); }
  show('unlocked');
  if (st && st.account) {
    const nm = String(st.account.name || ''); // never show a raw account UUID
    $('acctName').textContent = (!nm || /^[0-9a-f]{8}-[0-9a-f]{4}-/i.test(nm)) ? 'gitpass' : nm;
    $('acct').title = st.account.email || '';
  }
  loadDensity();
  await loadData();
  refreshWatchtowerBadge();
  if (!document.querySelector('.inline-err')) $('q').focus();
}

function showNoHost(e) {
  stopListTotp(); clearTotpCache();
  closeLargeType(false);
  $('hostErr').textContent = String((e && e.message) || '');
  $('hostCmd').textContent = 'cd ~/gitpass && cargo build --release -p gitpass-host\n"$HOME/Library/Application Support/gitpass/bin/gitpass-host" install --browser chrome --extension-id ' + ((globalThis.chrome && chrome.runtime && chrome.runtime.id) || '<id>');
  show('nohost');
}

let throttleTimer = null;
function startThrottleCountdown(secs) {
  if (throttleTimer) clearInterval(throttleTimer);
  let left = Math.max(1, secs);
  const update = () => {
    const btn = $('unlockBtn');
    if (left <= 0) {
      clearInterval(throttleTimer);
      throttleTimer = null;
      if (btn) btn.disabled = false;
      $('unlockErr').textContent = '';
      return;
    }
    if (btn) btn.disabled = true;
    $('unlockErr').textContent = t('unlockThrottled', 'Too many attempts. Try again in $1 s', [left]);
    left--;
  };
  update();
  throttleTimer = setInterval(update, 1000);
}

function showLocked(view, st) {
  stopTotp(); stopListTotp(); clearTotpCache();
  closeLargeType(false);
  if (S.iconCache) S.iconCache.clear();
  if (S.iconPending) S.iconPending.clear();
  S.revealed = {}; S.edit = null; S.findingsCache = {}; $('detail').textContent = '';
  show('locked');
  $('unlockErr').textContent = '';
  $('unlockForm').classList.add('hidden'); $('primaryBox').classList.remove('hidden');
  const appDown = view === 'appdown';
  $('touchIc').innerHTML = appDown ? UI.external : UI.fingerprint;
  $('touchLbl').textContent = appDown ? t('openAppToUnlock', 'Open gitpass app to unlock') : t('unlockTouchId', 'Unlock with Touch ID');
  $('usePw').textContent = appDown ? t('unlockWithPassword', 'Unlock with password') : t('usePassword', 'Use password');
  $('lockedSub').textContent = appDown ? t('appNotRunning', 'The gitpass app is not running.') : ((st && st.account && st.account.email) || '');
  // Secret Key is only needed in standalone mode (app not running).
  $('skRow').classList.toggle('hidden', !appDown || !!(st && st.secret_key_file));
  if (st && st.throttled_secs && st.throttled_secs > 0) {
    startThrottleCountdown(st.throttled_secs);
  }
  $('touchBtn').focus();
}

$('touchBtn').onclick = async () => {
  const b = $('touchBtn'); b.disabled = true; $('unlockErr').textContent = '';
  try {
    if (S.view === 'appdown') { await send({ type: 'popup:open_in_app' }); $('lockedSub').textContent = t('waitingForApp', 'Waiting for gitpass…'); setTimeout(refresh, 2500); return; }
    const r = await send({ type: 'popup:request_unlock', reason: t('unlockReason', 'Unlock the gitpass browser extension') });
    if (r && r.unlocked) await refresh(); else $('unlockErr').textContent = t('unlockCancelled', 'Unlock was cancelled');
  } catch (e) {
    if (e.code === 'app_not_running' || /app_not_running/.test(e.message)) return showLocked('appdown');
    $('unlockErr').textContent = String(e.message || e);
  } finally { b.disabled = false; }
};
$('usePw').onclick = () => { $('primaryBox').classList.add('hidden'); $('unlockForm').classList.remove('hidden'); $('pw').focus(); };
$('backTouch').onclick = () => { $('unlockForm').classList.add('hidden'); $('primaryBox').classList.remove('hidden'); };
if (typeof GitpassCapsLock !== 'undefined') GitpassCapsLock.attachCapsLockHint($('pw'), $('capsHint'));
$('unlockForm').onsubmit = async (e) => {
  e.preventDefault();
  const btn = $('unlockBtn'); if (btn.disabled) return;
  const msg = { type: 'popup:unlock', password: $('pw').value };
  const sk = $('sk').value.trim(); if (sk) msg.secret_key = sk;
  if (!msg.password) { $('unlockErr').textContent = t('pwRequired', 'Enter your master password'); return; }
  const sp = btn.querySelector('.spinner');
  btn.disabled = true; if (sp) sp.classList.remove('hidden');
  try {
    if (throttleTimer) { clearInterval(throttleTimer); throttleTimer = null; }
    await send(msg); $('pw').value = ''; $('sk').value = ''; await refresh();
  } catch (err) {
    if (err.code === 'throttled' || /throttled/i.test(err.message)) {
      const secs = err.seconds_remaining || err.retry_after_secs || (err.message && parseInt(err.message.match(/(\d+)\s*s?/)?.[1] || '2', 10)) || 2;
      startThrottleCountdown(secs);
    } else {
      $('unlockErr').textContent = (err.code === 'denied' || /unlock failed|denied/i.test(err.message)) ? t('wrongCreds', 'Wrong master password or Secret Key') : String(err.message);
      $('pw').select();
    }
  } finally {
    if (!throttleTimer) btn.disabled = false;
    if (sp) sp.classList.add('hidden');
  }
};
document.querySelectorAll('.eye').forEach((b) => {
  b.innerHTML = UI.eye;
  b.addEventListener('click', () => { const i = $(b.dataset.for); const on = i.type === 'password'; i.type = on ? 'text' : 'password'; b.innerHTML = on ? UI.eyeOff : UI.eye; });
});
$('retry').onclick = refresh;
$('lock').onclick = async () => { clearClipboardIfOurs(); await send({ type: 'popup:lock' }).catch(showErr); refresh(); };

// ---------- data + list ----------
// The host caps one reply at 1 MiB, so fetch every item in pages (#165: the popup must list all items).
const LIST_PAGE = 500;
async function listAllItems() {
  let items = [], offset = 0;
  for (let guard = 0; guard < 1000; guard++) {
    const r = await send({ type: 'popup:list_items', limit: LIST_PAGE, offset });
    items = items.concat((r && r.items) || []);
    const next = r && r.next_offset;
    if (typeof next !== 'number' || next <= offset) break;
    offset = next;
  }
  return { items };
}

async function loadData() {
  startListLoading();
  let listError = null;
  let vaults = { vaults: [] };
  let all = null;
  let match = { items: [], origin: null };
  let archivedRes = { items: [] };
  let deletedRes = { items: [] };

  try {
    const results = await Promise.all([
      send({ type: 'popup:list_vaults' }).catch(() => ({ vaults: [] })),
      listAllItems(),
      send({ type: 'popup:match_active' }).catch(() => ({ items: [], origin: null })),
      send({ type: 'popup:list_archived' }).catch(() => ({ items: [] })),
      send({ type: 'popup:list_deleted' }).catch(() => ({ items: [] })),
    ]);
    vaults = results[0];
    all = results[1];
    match = results[2];
    archivedRes = results[3];
    deletedRes = results[4];
  } catch (err) {
    listError = err;
  } finally {
    stopListLoading();
  }

  if (listError) {
    if (listError.code === 'nohost' || /disconnected|cannot connect/i.test(listError.message)) {
      return showNoHost(listError);
    }
    renderListError(listError, () => loadData());
    const pane = $('detail');
    if (pane) {
      pane.textContent = '';
      pane.removeAttribute('aria-busy');
      pane.removeAttribute('aria-label');
      pane.append(el('div', 'empty', t('selectItem', 'Select an item')));
    }
    S.detail = null;
    return;
  }

  S.vaults = vaults.vaults || [];
  S.all = (all && all.items) || [];
  S.suggested = (match && match.items) || [];
  S.origin = (match && match.origin) || null;
  S.archived = (archivedRes && archivedRes.items) || [];
  S.deleted = (deletedRes && deletedRes.items) || [];
  await refreshNeverSave();
  await loadSort();
  renderSort();

  const want = window.__gitpassMockParams && window.__gitpassMockParams.get('filter');
  if (want) S.filter = want;
  else if (S.view !== 'unlocked-loaded' && !S.suggested.length && S.filter === 'suggested') S.filter = 'all';
  S.view = 'unlocked-loaded';
  renderFilter();
  applyFilter(true);
  const pre = window.__gitpassMockParams && window.__gitpassMockParams.get('item');
  if (pre) {
    const i = S.rows.findIndex((r) => r.id === pre);
    if (i >= 0) select(i);
  }
}

let vaultMenuOpen = false;
function closeVaultMenu() {
  vaultMenuOpen = false;
  const m = $('vaultMenu');
  if (m) { m.classList.add('hidden'); m.textContent = ''; }
  const chip = document.querySelector('#filterChips [data-filter-id="vaults"]');
  if (chip) chip.setAttribute('aria-expanded', 'false');
}
let tagMenuOpen = false;
function closeTagMenu() {
  tagMenuOpen = false;
  const m = $('tagMenu');
  if (m) { m.classList.add('hidden'); m.textContent = ''; }
  const chip = document.querySelector('#filterChips [data-filter-id="tags"]');
  if (chip) chip.setAttribute('aria-expanded', 'false');
}
document.addEventListener('click', (e) => {
  if (!vaultMenuOpen && !tagMenuOpen) return;
  if (e.target && e.target.closest && e.target.closest('.filter-chips-row')) return;
  if (vaultMenuOpen) closeVaultMenu();
  if (tagMenuOpen) closeTagMenu();
});
function renderFilter() {
  const wrap = $('filterChips'); if (!wrap) return;
  const prev = document.activeElement;
  const hadChipFocus = !!(prev && prev.classList && prev.classList.contains('filter-chip'));
  const focusId = hadChipFocus ? prev.dataset.filterId : null;
  wrap.textContent = '';
  closeVaultMenu();
  closeTagMenu();
  const chips = V.buildFilterChips(S.all, S.vaults, {
    origin: S.origin,
    currentFilter: S.filter,
    archived: S.archived,
    deleted: S.deleted,
    archiveCount: (S.archived || []).length,
    deletedCount: (S.deleted || []).length,
  });
  const setRoving = (focused) => {
    for (const c of wrap.querySelectorAll('.filter-chip')) c.tabIndex = c === focused ? 0 : -1;
  };
  for (const f of chips) {
    const active = f.kind === 'vaults' ? S.filter.startsWith('vault:') : f.kind === 'tags' ? S.filter.startsWith('tag:') : S.filter === f.id;
    const b = el('button', 'filter-chip'); b.type = 'button';
    b.dataset.filterId = f.id;
    b.setAttribute('role', 'tab');
    b.setAttribute('aria-selected', active ? 'true' : 'false');
    b.tabIndex = active ? 0 : -1;
    if (f.category && typeof categoryIcon === 'function') {
      const ic = el('span', 'ic'); ic.innerHTML = categoryIcon(f.category, 14); b.append(ic);
      b.append(document.createTextNode(t(f.key, f.label)));
    } else if (f.id === 'suggested' && f.host) {
      b.textContent = t(f.key, f.label) + ' · ' + f.host;
    } else if (f.kind === 'vaults') {
      b.textContent = t(f.key, f.label) + ' ▾';
      b.setAttribute('aria-haspopup', 'menu');
      b.setAttribute('aria-expanded', 'false');
    } else if (f.kind === 'tags') {
      b.textContent = t(f.key, f.label) + ' ▾';
      b.setAttribute('aria-haspopup', 'menu');
      b.setAttribute('aria-expanded', 'false');
    } else if (f.kind === 'tag') {
      b.textContent = f.label + ' ×';
      b.title = t('clearTag', 'Clear tag filter');
      b.setAttribute('aria-label', t('clearTag', 'Clear tag filter'));
    } else {
      let label = f.key ? t(f.key, f.label) : f.label;
      if (typeof f.count === 'number' && (f.id === 'archive' || f.id === 'deleted')) {
        label += ' (' + f.count + ')';
      }
      b.textContent = label;
    }
    b.onclick = () => {
      if (f.kind === 'vaults') {
        vaultMenuOpen = !vaultMenuOpen;
        b.setAttribute('aria-expanded', vaultMenuOpen ? 'true' : 'false');
        const m = $('vaultMenu'); m.textContent = '';
        if (vaultMenuOpen) {
          for (const v of f.vaults) {
            const item = el('button', null, v.label); item.type = 'button';
            item.setAttribute('role', 'menuitem');
            item.setAttribute('aria-selected', S.filter === v.id ? 'true' : 'false');
            item.onclick = (ev) => { ev.stopPropagation(); S.filter = v.id; renderFilter(); applyFilter(true); const vc = document.querySelector('#filterChips [data-filter-id="vaults"]'); if (vc) vc.focus(); };
            item.onkeydown = (ev) => { if (ev.key === 'Escape') { ev.stopPropagation(); closeVaultMenu(); const vc = document.querySelector('#filterChips [data-filter-id="vaults"]'); if (vc) vc.focus(); } };
            m.append(item);
          }
          m.classList.remove('hidden');
        } else { m.classList.add('hidden'); }
        return;
      }
      if (f.kind === 'tags') {
        tagMenuOpen = !tagMenuOpen;
        b.setAttribute('aria-expanded', tagMenuOpen ? 'true' : 'false');
        const m = $('tagMenu'); m.textContent = '';
        if (tagMenuOpen) {
          const input = el('input'); input.type = 'search'; input.className = 'tag-filter-input';
          input.placeholder = t('filterTagsPh', 'Filter tags…');
          input.setAttribute('aria-label', t('filterTagsPh', 'Filter tags…'));
          const list = el('div', 'tag-list');
          const renderTagList = () => {
            list.textContent = '';
            const q = input.value.trim().toLowerCase();
            const shown = (f.tags || []).filter((tg) => !q || tg.tag.toLowerCase().includes(q));
            if (!shown.length) { list.append(el('div', 'tag-empty', t('noItems', 'No items'))); return; }
            for (const tg of shown) {
              const item = el('button', null, tg.tag + ' (' + tg.count + ')'); item.type = 'button';
              item.setAttribute('role', 'menuitem');
              item.setAttribute('aria-selected', S.filter === 'tag:' + tg.tag ? 'true' : 'false');
              item.onclick = (ev) => { ev.stopPropagation(); S.filter = 'tag:' + tg.tag; renderFilter(); applyFilter(true); const tc = document.querySelector('#filterChips [data-filter-id="tags"]'); if (tc) tc.focus(); };
              item.onkeydown = (ev) => { if (ev.key === 'Escape') { ev.stopPropagation(); closeTagMenu(); const tc = document.querySelector('#filterChips [data-filter-id="tags"]'); if (tc) tc.focus(); } };
              list.append(item);
            }
          };
          input.oninput = renderTagList;
          input.onkeydown = (ev) => {
            if (ev.key === 'Escape') { ev.stopPropagation(); closeTagMenu(); b.focus(); }
            else if (ev.key === 'Enter') {
              const first = list.querySelector('button');
              if (first) { ev.preventDefault(); first.click(); }
            } else if (ev.key === 'ArrowDown') {
              const first = list.querySelector('button');
              if (first) { ev.preventDefault(); first.focus(); }
            }
          };
          renderTagList();
          m.append(input, list);
          m.classList.remove('hidden');
          input.focus();
        } else { m.classList.add('hidden'); }
        return;
      }
      if (f.kind === 'tag') {
        S.filter = 'all'; renderFilter(); applyFilter(true);
        return;
      }
      S.filter = f.id; renderFilter(); applyFilter(true);
    };
    b.onkeydown = (e) => {
      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft' || e.key === 'Home' || e.key === 'End') {
        e.preventDefault();
        const all = [...wrap.querySelectorAll('.filter-chip')];
        const next = all[V.chipNavIndex(all.indexOf(b), e.key, all.length)];
        if (next) { setRoving(next); next.focus(); }
      } else if (e.key === 'Escape' && (vaultMenuOpen || tagMenuOpen)) {
        e.stopPropagation(); if (vaultMenuOpen) closeVaultMenu(); if (tagMenuOpen) closeTagMenu(); b.focus();
      }
    };
    wrap.append(b);
  }
  if (focusId) {
    const restore = wrap.querySelector(`[data-filter-id="${focusId}"]`);
    if (restore) { setRoving(restore); restore.focus(); }
  }
  const noteEl = $('siteExceptedNote');
  if (noteEl) {
    const showNote = S.filter === 'suggested' && isSiteExcepted();
    noteEl.classList.toggle('hidden', !showNote);
  }
  const ssBtn = $('siteSettingsBtn');
  if (ssBtn) {
    const hasSite = !!(S.origin && typeof GitpassPolicy !== 'undefined' && GitpassPolicy.isFillableOrigin(S.origin));
    ssBtn.classList.toggle('hidden', !hasSite);
  }
}

// Sort menu (#165): newest edit first by default; the choice is kept in extension storage.
const SORT_LABELS = { updated: ['sortUpdated', 'Last edited'], created: ['sortCreated', 'Date created'], title: ['sortTitle', 'Title'] };
function renderSort() {
  const s = $('sort'); if (!s) return;
  s.textContent = '';
  s.setAttribute('aria-label', t('sortBy', 'Sort by'));
  for (const k of V.SORTS) { const o = el('option', null, t(SORT_LABELS[k][0], SORT_LABELS[k][1])); o.value = k; s.append(o); }
  s.value = S.sort;
}
function loadSort() {
  if (!globalThis.chrome || !chrome.storage || !chrome.storage.local) return Promise.resolve();
  return new Promise((res) => {
    try { chrome.storage.local.get('popupSort', (r) => { const v = r && r.popupSort; if (V.SORTS.includes(v)) S.sort = v; res(); }); } catch (_) { res(); }
  });
}
if ($('sort')) $('sort').onchange = () => {
  S.sort = $('sort').value;
  try { if (globalThis.chrome && chrome.storage && chrome.storage.local) chrome.storage.local.set({ popupSort: S.sort }); } catch (_) { /* ignore */ }
  applyFilter(false);
};
const undoExceptedBtn = $('undoExcepted');
if (undoExceptedBtn) {
  undoExceptedBtn.onclick = async () => {
    await removeNeverSave(GitpassPolicy.registrableDomain(S.origin));
    toast(t('saved', 'Saved'));
    renderFilter();
    applyFilter(false);
  };
}

function createMonogramSpan(title, size = 20) {
  const span = el('span', 'monogram');
  span.setAttribute('aria-hidden', 'true');
  const letter = V && V.getMonogram ? V.getMonogram(title) : (String(title || '').trim().charAt(0).toUpperCase() || '?');
  span.textContent = letter;
  if (size !== 20) {
    span.style.width = size + 'px';
    span.style.height = size + 'px';
    span.style.fontSize = Math.max(10, Math.round(size * 0.45)) + 'px';
  }
  return span;
}

function renderRowIcon(container, it, size = 20) {
  container.textContent = '';
  if (!it || it.category !== 'login') {
    container.innerHTML = categoryIcon((it && it.category) || 'login', size >= 28 ? 44 : 32);
    return;
  }
  const host = (it.favicon_host || (it.url && V && V.hostOfUrl(it.url)) || '').toLowerCase();
  const title = it.title || host || '';
  container.dataset.iconHost = host;

  if (host && S.iconCache && S.iconCache.has(host)) {
    const cached = S.iconCache.get(host);
    if (cached) {
      const img = el('img', 'site-icon-img');
      img.src = cached;
      img.alt = '';
      img.setAttribute('aria-hidden', 'true');
      img.width = size;
      img.height = size;
      img.onerror = () => {
        if (S.iconCache) S.iconCache.set(host, null);
        container.replaceChildren(createMonogramSpan(title, size));
      };
      container.appendChild(img);
      return;
    }
  }

  // Fallback to monogram
  container.appendChild(createMonogramSpan(title, size));

  if (host && S.iconCache && !S.iconCache.has(host) && S.iconPending && !S.iconPending.has(host)) {
    S.iconPending.add(host);
    send({ type: 'popup:site_icon', host })
      .then((res) => {
        if (S.iconPending) S.iconPending.delete(host);
        if (res && res.data_url) {
          if (S.iconCache) S.iconCache.set(host, res.data_url);
          if (container.isConnected && container.dataset.iconHost === host) {
            const img = el('img', 'site-icon-img');
            img.src = res.data_url;
            img.alt = '';
            img.setAttribute('aria-hidden', 'true');
            img.width = size;
            img.height = size;
            img.onerror = () => {
              if (S.iconCache) S.iconCache.set(host, null);
              container.replaceChildren(createMonogramSpan(title, size));
            };
            container.replaceChildren(img);
          }
        } else {
          if (S.iconCache) S.iconCache.set(host, null);
        }
      })
      .catch(() => {
        if (S.iconPending) S.iconPending.delete(host);
        if (S.iconCache) S.iconCache.set(host, null);
      });
  }
}

function applyFilter(selectFirst) {
  const prevId = S.rows[S.sel] && S.rows[S.sel].id;
  S.rows = V.filterItems(S.all, S.suggested, S.filter, S.query, S.sort, { archived: S.archived, deleted: S.deleted });
  let idx = prevId ? S.rows.findIndex((r) => r.id === prevId) : -1;
  if (idx < 0 && (selectFirst || prevId)) idx = S.rows.length ? 0 : -1;
  $('list').scrollTop = 0;
  renderList();
  select(idx);
  fetchFindingsForVisibleRows();
}

let findingsFetchSeq = 0;
const FINDINGS_MAX_ROWS = 60;
async function fetchFindingsForVisibleRows() {
  const seq = ++findingsFetchSeq;
  const need = S.rows.filter((r) => r.category === 'login' && !S.findingsCache[r.id]);
  if (!need.length) return;
  // Bounded fan-out: first lookup warms the host's audit cache, then at most 3 in flight.
  // Only the first FINDINGS_MAX_ROWS rows are fetched; the rest load when the user selects them.
  await V.mapLimit(need.slice(0, FINDINGS_MAX_ROWS), 3, async (r) => {
    if (seq !== findingsFetchSeq) return;
    try {
      const res = await send({ type: 'popup:item_findings', item_id: r.id });
      if (V.cacheableFindings(res)) S.findingsCache[r.id] = res;
    } catch (_) {
      // Leave uncached so dots and the strip appear once the audit is ready.
    }
  });
  if (seq === findingsFetchSeq) {
    renderList();
  }
}

function renderHighlight(text, query) {
  const frag = document.createDocumentFragment();
  const segs = V.highlightSegments(text, query);
  for (const s of segs) {
    if (s.match) {
      const mark = document.createElement('mark');
      mark.className = 'hl';
      mark.textContent = s.text;
      frag.appendChild(mark);
    } else {
      frag.appendChild(document.createTextNode(s.text));
    }
  }
  return frag;
}

function updateSearchHints() {
  const hints = $('searchHints');
  if (!hints) return;
  const show = Boolean(S.rows.length);
  hints.classList.toggle('hidden', !show);
  if (show) {
    hints.textContent = t('searchHints', '↑↓ navigate · ↵ fill · ⌘C copy password · ? all shortcuts · Esc clear');
  }
}

function renderList() {
  const list = $('list'), inner = $('listInner');
  inner.textContent = '';
  // Reset spacer padding first: an empty list must not keep the previous list's padTop/padBottom as blank space.
  inner.style.paddingTop = '0px'; inner.style.paddingBottom = '0px';
  updateSearchHints();
  if (!S.rows.length) { stopListTotp(); renderEmpty(inner); return; }
  const w = V.virtualWindow(list.scrollTop, list.clientHeight || 380, rowHeight(), S.rows.length);
  inner.style.paddingTop = w.padTop + 'px'; inner.style.paddingBottom = w.padBottom + 'px';
  for (let i = w.start; i < w.end; i++) {
    const it = S.rows[i];
    const isSel = i === S.sel;
    const row = el('div', 'row' + (isSel ? ' sel' : ''));
    row.id = 'row-' + i;
    row.setAttribute('role', 'option');
    row.setAttribute('aria-selected', isSel ? 'true' : 'false');
    row.setAttribute('tabindex', '-1');
    row.dataset.idx = i;
    const ic = el('span', 'ic'); renderRowIcon(ic, it, 20);
    const meta = el('div', 'meta');
    const tDiv = el('div', 't');
    if (S.query) tDiv.append(renderHighlight(it.title, S.query));
    else tDiv.textContent = it.title;
    const uDiv = el('div', 'u');
    if (it.deleted_at) {
      const trashSub = V.formatTrashSubtitle(it.deleted_at, Date.now(), t);
      if (trashSub) {
        uDiv.textContent = trashSub.text;
        if (trashSub.danger) uDiv.classList.add('text-danger');
      } else {
        uDiv.textContent = '—';
      }
    } else {
      const uText = V.rowSubtitle(it, S.query);
      if (S.query && uText && uText !== '—') uDiv.append(renderHighlight(uText, S.query));
      else uDiv.textContent = uText || '—';
    }
    meta.append(tDiv, uDiv);
    row.append(ic, meta);
    if (it.favorite) { const f = el('span', 'fav'); f.innerHTML = UI.star; row.append(f); }
    if (it.has_attachments) {
      const att = el('span', 'att');
      att.innerHTML = UI.paperclip;
      att.setAttribute('role', 'img');
      att.setAttribute('aria-label', t('hasAttachments', 'Has attachments'));
      row.append(att);
    }
    if (it.has_totp) row.append(totpChip(it));
    const findings = S.findingsCache[it.id];
    if (it.category === 'login' && V.hasWatchtowerFindings(findings)) {
      const isDanger = findings.kinds.some((k) => V.findingSeverity(k) === 'danger');
      const dot = el('span', 'warn-dot ' + (isDanger ? 'danger' : 'warn'));
      dot.setAttribute('role', 'img');
      dot.setAttribute('aria-label', t('watchtowerWarning', 'Security warning'));
      row.append(dot);
    }
    // Quick actions (issue #255): rendered for every row that has them, but
    // only shown on row hover / selection / focus (see popup.css), so the
    // title never moves and the row height never changes. Only the selected
    // row's buttons are in the Tab order; the rest are mouse-only until
    // selected, which re-renders them tabbable.
    const qa = V.quickActions ? V.quickActions(it) : [];
    if (qa.length) row.append(quickActionsEl(it, i, isSel, qa));
    row.onclick = () => select(i);
    row.ondblclick = () => openAndFill();
    inner.append(row);
  }
  if (inner.querySelector('[data-totp-id]')) startListTotp(); else stopListTotp();
}

/** Empty states: offer the action that fills the gap (create the searched item, save this site's login). */
function renderEmpty(inner) {
  const st = V.emptyState({ query: S.query, filter: S.filter, origin: S.origin, count: S.all.length, excepted: isSiteExcepted() });
  const box = el('div', 'empty');
  const ic = st.kind === 'search' ? UI.search : st.kind === 'site' ? categoryIcon('login', 36) : st.kind === 'nofill' ? UI.external : st.kind === 'vault' ? UI.plus : st.kind === 'archive' ? UI.archive : st.kind === 'deleted' ? UI.trash : null;
  if (ic) { const w = el('span', 'empty-ic'); w.innerHTML = ic; box.append(w); }
  const msg = {
    search: () => t('noResults', 'No items found'),
    site: () => t('noMatches', 'No saved logins for this site'),
    nofill: () => t('notFillable', 'Open an https page to fill'),
    vault: () => t('emptyVaultTitle', 'Your vault is empty'),
    archive: () => t('emptyArchiveTitle', 'Nothing archived'),
    deleted: () => t('emptyTrashTitle', 'Recently Deleted is empty'),
    none: () => t('noItems', 'No items'),
  }[st.kind]();
  box.append(el('p', null, msg));
  if (st.kind === 'site') {
    box.append(el('div', 'empty-host', st.host));
    if (isSiteExcepted()) {
      const note = el('p', 'empty-excepted-note');
      const span = el('span', null, t('savingOff', 'Saving is off for this site'));
      const dot = document.createTextNode(' · ');
      const undo = el('button', 'btn-link', t('undo', 'Undo'));
      undo.type = 'button';
      undo.onclick = async () => {
        await removeNeverSave(GitpassPolicy.registrableDomain(S.origin));
        toast(t('saved', 'Saved'));
        renderFilter();
        applyFilter(false);
      };
      note.append(span, dot, undo);
      box.append(note);
    }
  }
  if (st.kind === 'vault') box.append(el('p', 'empty-sub', t('emptyVaultBody', 'Items you add will appear here.')));
  if (st.kind === 'archive') box.append(el('p', 'empty-sub', t('emptyArchiveBody', 'Items you archive will appear here.')));
  if (st.kind === 'deleted') box.append(el('p', 'empty-sub', t('emptyTrashBody', 'Items you delete stay here for 30 days.')));
  const acts = el('div', 'empty-actions');
  const btn = (cls, label, fn) => { const b = el('button', cls, label); b.type = 'button'; b.onclick = fn; acts.append(b); };
  if (st.kind === 'search') btn('btn btn-primary', t('emptyCreateNamed', 'Create item named “$1”', [st.query]), () => openNewItem('login', { title: st.query }));
  else if (st.kind === 'site') btn('btn btn-primary', t('emptySaveLoginFor', 'Save a login for $1', [st.host]), () => openNewItem('login', { title: st.host, url: S.origin }));
  else if (st.kind === 'vault') btn('btn btn-primary', t('newItem', 'New item'), () => openNewItem());
  if (st.kind === 'site' || st.kind === 'nofill') btn('btn btn-link', t('emptyShowAll', 'Show all items'), () => { S.filter = 'all'; renderFilter(); applyFilter(true); });
  if (acts.children.length) box.append(acts);
  inner.append(box);
}
$('list').addEventListener('scroll', () => renderList(), { passive: true });

let qt = null;
$('q').oninput = () => { clearTimeout(qt); qt = setTimeout(() => { S.query = $('q').value.trim(); applyFilter(true); }, 80); };

$('genBtn').innerHTML = UI.key; $('genBtn').onclick = () => (S.panel === 'gen' ? closePanel() : openGenerator());
  if ($('watchtowerBtn')) { $('watchtowerBtn').innerHTML = UI.shield; $('watchtowerBtn').onclick = () => (S.panel === 'watchtower' ? closePanel() : openWatchtower()); }
  $('siteSettingsBtn').innerHTML = UI.gear; $('siteSettingsBtn').onclick = () => (S.panel === 'site-settings' ? closePanel() : openSiteSettings(S.origin));
$('newBtn').innerHTML = UI.plus; $('newBtn').onclick = () => (S.panel === 'new' ? closePanel() : openNewItem());
$('lock').innerHTML = UI.lock; $('searchIc').innerHTML = UI.search;
if ($('searchHints')) $('searchHints').onclick = () => (S.panel === 'shortcuts' ? closePanel() : openShortcuts());

// ---------- keyboard ----------
document.addEventListener('keydown', (e) => {
  if ($('unlocked').classList.contains('hidden')) { if (e.key === 'Escape') window.close(); return; }
  // Large type is modal: Esc closes, Tab stays inside, and list shortcuts pause while it is open.
  const lt = $('largetype');
  if (lt && !lt.classList.contains('hidden')) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeLargeType(); return; }
    if (e.key === 'Tab') {
      const focusable = Array.from(lt.querySelectorAll('button:not([disabled]), [tabindex]:not([tabindex="-1"])'));
      if (focusable.length) {
        const first = focusable[0], last = focusable[focusable.length - 1];
        if (e.shiftKey) {
          if (document.activeElement === first || !lt.contains(document.activeElement)) { e.preventDefault(); last.focus(); }
        } else if (document.activeElement === last || !lt.contains(document.activeElement)) { e.preventDefault(); first.focus(); }
      }
    }
    return;
  }
  const a = document.activeElement;
  // The detail item menu owns its own keys (arrows, Escape) while it is open; do not also move the list.
  if (a && a.closest && a.closest('.menu') && !a.closest('.menu').classList.contains('hidden')) return;
  // Inline edit owns the keyboard: Esc cancels, ⌘↩ saves, everything else is typing.
  if (S.edit && !S.panel) {
    if (e.key === 'Escape') { e.preventDefault(); cancelEdit(); return; }
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      const f = $('detail') && $('detail').querySelector('form.inline-edit');
      if (f) f.requestSubmit();
      return;
    }
    return;
  }
  // Focus trap for shortcuts modal
  if (e.key === 'Tab' && S.panel === 'shortcuts') {
    const p = $('panel');
    const focusable = Array.from(p.querySelectorAll('button:not([disabled]), [tabindex]:not([tabindex="-1"])'));
    if (focusable.length) {
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first || !p.contains(document.activeElement)) {
          e.preventDefault();
          last.focus();
          return;
        }
      } else {
        if (document.activeElement === last || !p.contains(document.activeElement)) {
          e.preventDefault();
          first.focus();
          return;
        }
      }
    }
  }
  // ⌘⇧G / Ctrl+Shift+G opens the generator from anywhere in the popup.
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'g') {
    e.preventDefault();
    if (S.panel === 'gen') closePanel(); else openGenerator();
    return;
  }
  // Enter on a search with no results: create the item the user was looking for (before the debounce fires).
  if (e.key === 'Enter' && a && a.id === 'q') {
    const q = $('q').value.trim();
    if (q !== S.query) { S.query = q; applyFilter(true); }
    if (S.query && !S.rows.length) { e.preventDefault(); openNewItem('login', { title: S.query }); return; }
  }
  const inForm = !!(a && a.closest && a.closest('#panel, select, .menu, button'));
  const inEditable = !!(a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.isContentEditable));
  // ⌘E edits the selected login, secure note, or API credential inline.
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && (e.key === 'e' || e.key === 'E') && !inForm && S.detail && canEditCategory(S.detail.category) && !S.detail.deleted_at) {
    e.preventDefault(); startEdit(); return;
  }
  // Backspace or Alt+Left navigates back in detail history if present
  if (((e.altKey && e.key === 'ArrowLeft') || e.key === 'Backspace') && !inEditable && !inForm) {
    if (typeof detailNavHistory !== 'undefined' && detailNavHistory.length > 0) {
      e.preventDefault();
      goBackDetail();
      return;
    }
  }
  const sel = window.getSelection && String(window.getSelection());
  const inputSel = a && a.tagName === 'INPUT' && a.selectionStart !== a.selectionEnd;
  const selected = S.rows[S.sel];
  const act = V.keyAction(e, { panel: S.panel, inForm, inEditable, hasSelection: !!sel || inputSel, hasTotp: !!(selected && selected.has_totp), query: S.query });
  if (!act) return;
  e.preventDefault();
  if (act === 'close') window.close();
  else if (act === 'closePanel') {
    if (S.panel === 'watchtower' && typeof wtDrillKind !== 'undefined' && wtDrillKind && typeof wtData !== 'undefined' && wtData) {
      renderWatchtowerOverview($('panel'), wtData);
    } else {
      closePanel();
    }
  }
  else if (act === 'toggleShortcuts' || act === 'shortcuts') {
    if (S.panel === 'shortcuts') closePanel(); else openShortcuts();
  }
  else if (act === 'clearSearch') {
    S.query = '';
    const qIn = $('q');
    if (qIn) qIn.value = '';
    applyFilter(true);
  }
  else if (act === 'down' || act === 'up') select(V.moveSel(S.sel, act === 'down' ? 1 : -1, S.rows.length));
  else if (act === 'fill') openAndFill();
  else if (act === 'copyTotp') { if (selected) copyTotp(selected.id).catch(showErr); }
  else if (act === 'copyPassword') copyPrimary().catch(showErr);
  else if (act === 'copyUsername') copyUsername().catch(showErr);
  else if (act === 'largeType') openLargeTypeForSelected();
  else if (act === 'openWebsite') openWebsite();
  else if (act === 'restore') {
    if (selected) {
      if (selected.deleted_at) restoreItem(selected).catch(showErr);
      else if (selected.archived) unarchiveItem(selected).catch(showErr);
    }
  }
  else if (act === 'generator') { if (S.panel === 'gen') closePanel(); else openGenerator(); }
});

// ---------- live sync ----------
// The app pushes `changed` when anyone writes the vault (GUI, this extension, CLI, git sync). Reload the
// lists in place, keeping the selection and scroll; never clobber an open New-item form (reload after it closes).
let liveT = null, livePending = false;
function liveReload() {
  if (S.view !== 'unlocked-loaded') return;
  if (S.panel === 'new' || S.edit) { livePending = true; return; }
  clearTimeout(liveT);
  liveT = setTimeout(async () => {
    S.findingsCache = {};
    const [all, match] = await Promise.all([
      listAllItems().catch(() => null),
      send({ type: 'popup:match_active' }).catch(() => null),
    ]);
    if (!all) return;
    S.all = all.items || []; if (match) { S.suggested = match.items || []; S.origin = match.origin || null; }
    const top = $('list').scrollTop;
    applyFilter(false);
    $('list').scrollTop = top; renderList();
    refreshWatchtowerBadge();
  }, 120);
}

// ---------- live lock/unlock events ----------
if (globalThis.chrome && chrome.runtime && chrome.runtime.onMessage) {
  chrome.runtime.onMessage.addListener((msg) => {
    if (!msg || msg.type !== 'popup:event') return;
    if (msg.event === 'unlock') refresh();
    else if (msg.event === 'changed') liveReload();
    else if (msg.event === 'lock') {
      if (S.iconCache) S.iconCache.clear();
      if (S.iconPending) S.iconPending.clear();
      closeLargeType(false);
      clearClipboardIfOurs();
      popupSettings().then((st) => { if (st.autolockFollowsApp !== false) refresh(); });
    }
  });
}

if (typeof document !== 'undefined') {
  document.addEventListener('pagehide', () => { stopListTotp(); stopTotp(); closeLargeType(false); });
  GitpassI18n.apply(document);
  refresh();
}

if (typeof window !== 'undefined' && window.__gitpassMock) {
  window.__gitpassPopup = {
  restoreItem,
  archiveItem,
  unarchiveItem,
  trashItem,
  renderFilter,
  renderEmpty,
  copy,
  clearClipboardIfOurs,
  renderListSkeletons,
  renderDetailSkeleton,
  startListLoading,
  stopListLoading,
  renderListError,
  renderDetailError,
  send,
  loadData,
  select,
  S,
  };
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    restoreItem,
    archiveItem,
    unarchiveItem,
    trashItem,
    renderFilter,
    renderEmpty,
    copy,
    clearClipboardIfOurs,
    renderListSkeletons,
    renderDetailSkeleton,
    startListLoading,
    stopListLoading,
    renderListError,
    renderDetailError,
    send,
    loadData,
    select,
    S,
  };
}
