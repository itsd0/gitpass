/* global chrome, GitpassI18n, GitpassPopupIcons, GitpassView, GitpassNewItem */
'use strict';
// gitpass popup v0.6: 2-pane UI. All host calls go through background.js as `popup:*` messages.
const $ = (id) => document.getElementById(id);
const t = (k, f, subs) => GitpassI18n.t(k, f, subs);
const V = typeof GitpassView !== 'undefined' ? GitpassView : (typeof require !== 'undefined' ? require('./view.js') : null);
const { UI, categoryIcon } = typeof GitpassPopupIcons !== 'undefined' ? GitpassPopupIcons : (typeof require !== 'undefined' ? require('./icons.js') : { UI: {} });
const ROW_H = 52;

// ---------- password generator (issue #68) ----------
const G = typeof GitpassGenerator !== 'undefined' ? GitpassGenerator : (typeof require !== 'undefined' ? require('./generator.js') : null);
const GEN_MODES = G.MODES;
const GEN_DEFAULTS = G.DEFAULTS;
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
function toast(msg) { const e = $('toast'); if (!e) return; e.textContent = msg; e.classList.remove('hidden'); clearTimeout(toastT); toastT = setTimeout(() => e.classList.add('hidden'), 1600); }
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
async function refresh() {
  let st = null, err = null;
  try { st = await send({ type: 'popup:status' }); } catch (e) { err = e; }
  const view = V.viewFor(st, err);
  S.view = view;
  if (view === 'nohost') return showNoHost(err);
  if (view === 'locked' || view === 'appdown') return showLocked(view, st);
  show('unlocked');
  if (st && st.account) {
    const nm = String(st.account.name || ''); // never show a raw account UUID
    $('acctName').textContent = (!nm || /^[0-9a-f]{8}-[0-9a-f]{4}-/i.test(nm)) ? 'gitpass' : nm;
    $('acct').title = st.account.email || '';
  }
  await loadData();
  if (!document.querySelector('.inline-err')) $('q').focus();
}

function showNoHost(e) {
  stopListTotp(); clearTotpCache();
  $('hostErr').textContent = String((e && e.message) || '');
  $('hostCmd').textContent = 'cd ~/gitpass && cargo build --release -p gitpass-host\n"$HOME/Library/Application Support/gitpass/bin/gitpass-host" install --browser chrome --extension-id ' + ((globalThis.chrome && chrome.runtime && chrome.runtime.id) || '<id>');
  show('nohost');
}

function showLocked(view, st) {
  stopTotp(); stopListTotp(); clearTotpCache();
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
$('unlockForm').onsubmit = async (e) => {
  e.preventDefault();
  const btn = $('unlockBtn'); if (btn.disabled) return;
  const msg = { type: 'popup:unlock', password: $('pw').value };
  const sk = $('sk').value.trim(); if (sk) msg.secret_key = sk;
  if (!msg.password) { $('unlockErr').textContent = t('pwRequired', 'Enter your master password'); return; }
  const sp = btn.querySelector('.spinner');
  btn.disabled = true; if (sp) sp.classList.remove('hidden');
  try { await send(msg); $('pw').value = ''; $('sk').value = ''; await refresh(); }
  catch (err) { $('unlockErr').textContent = (err.code === 'denied' || /unlock failed|denied/i.test(err.message)) ? t('wrongCreds', 'Wrong master password or Secret Key') : String(err.message); $('pw').select(); }
  finally { btn.disabled = false; if (sp) sp.classList.add('hidden'); }
};
document.querySelectorAll('.eye').forEach((b) => {
  b.innerHTML = UI.eye;
  b.addEventListener('click', () => { const i = $(b.dataset.for); const on = i.type === 'password'; i.type = on ? 'text' : 'password'; b.innerHTML = on ? UI.eyeOff : UI.eye; });
});
$('retry').onclick = refresh;
$('lock').onclick = async () => { clearClipboardIfOurs(); await send({ type: 'popup:lock' }).catch(showErr); refresh(); };

// ---------- Skeletons & error cards ----------
let listSkeletonTimer = null;
let listLoading = false;

function renderListSkeletons() {
  stopListTotp();
  const list = $('list'), inner = $('listInner');
  if (!list || !inner) return;
  list.setAttribute('aria-busy', 'true');
  list.setAttribute('aria-label', t('loadingItems', 'Loading items…'));
  inner.textContent = '';
  inner.style.paddingTop = '0px';
  inner.style.paddingBottom = '0px';
  for (let i = 0; i < 6; i++) {
    const row = el('div', 'row skeleton-row skeleton-shimmer');
    row.setAttribute('aria-hidden', 'true');
    const ic = el('span', 'skeleton-icon');
    const meta = el('div', 'skeleton-meta');
    meta.append(el('span', 'skeleton-bar t'), el('span', 'skeleton-bar u'));
    row.append(ic, meta);
    inner.append(row);
  }
}

function startListLoading(delayMs = (V ? V.SKELETON_DELAY_MS : 150)) {
  listLoading = true;
  const list = $('list');
  if (list) list.setAttribute('aria-busy', 'true');
  clearTimeout(listSkeletonTimer);
  listSkeletonTimer = setTimeout(() => {
    if (listLoading) renderListSkeletons();
  }, delayMs);
}

function stopListLoading() {
  listLoading = false;
  clearTimeout(listSkeletonTimer);
  listSkeletonTimer = null;
  const list = $('list');
  if (list) {
    list.removeAttribute('aria-busy');
    list.removeAttribute('aria-label');
  }
}

function renderListError(err, onRetry) {
  stopListTotp();
  const list = $('list'), inner = $('listInner');
  if (!list || !inner) return;
  list.removeAttribute('aria-busy');
  list.removeAttribute('aria-label');
  inner.textContent = '';
  inner.style.paddingTop = '0px';
  inner.style.paddingBottom = '0px';
  const card = el('div', 'inline-err');
  card.setAttribute('role', 'alert');
  const ic = el('div', 'inline-err-ic');
  ic.innerHTML = UI.alert || UI.warn;
  const msg = el('h3', 'inline-err-msg', t('errListLoad', "Couldn't load items"));
  const sub = el('p', 'inline-err-sub muted', errText(err));
  const btn = el('button', 'btn btn-primary btn-retry', t('retry', 'Retry'));
  btn.type = 'button';
  btn.onclick = () => { onRetry(); };
  card.append(ic, msg, sub, btn);
  inner.append(card);
  btn.focus();
}

function renderDetailSkeleton(it) {
  const pane = $('detail');
  if (!pane) return;
  pane.textContent = '';
  pane.setAttribute('aria-busy', 'true');
  pane.setAttribute('aria-label', t('loadingDetail', 'Loading item details…'));

  const head = el('div', 'd-head skeleton-head skeleton-shimmer');
  head.setAttribute('aria-hidden', 'true');
  const ic = el('span', 'skeleton-icon lg');
  const g = el('div', 'skeleton-meta grow');
  g.append(el('span', 'skeleton-bar h'), el('span', 'skeleton-bar sub'));
  head.append(ic, g);
  pane.append(head);

  const box = el('div', 'fields skeleton-fields');
  box.setAttribute('aria-hidden', 'true');
  for (let i = 0; i < 3; i++) {
    const f = el('div', 'f skeleton-field skeleton-shimmer');
    const fg = el('div', 'grow');
    fg.append(el('span', 'skeleton-bar lbl'), el('span', 'skeleton-bar val'));
    f.append(fg);
    box.append(f);
  }
  pane.append(box);
}

function renderDetailError(err, onRetry) {
  const pane = $('detail');
  if (!pane) return;
  pane.removeAttribute('aria-busy');
  pane.removeAttribute('aria-label');
  pane.textContent = '';
  const card = el('div', 'inline-err');
  card.setAttribute('role', 'alert');
  const ic = el('div', 'inline-err-ic');
  ic.innerHTML = UI.alert || UI.warn;
  const msg = el('h3', 'inline-err-msg', t('errDetailLoad', "Couldn't load item details"));
  const sub = el('p', 'inline-err-sub muted', errText(err));
  const btn = el('button', 'btn btn-primary btn-retry', t('retry', 'Retry'));
  btn.type = 'button';
  btn.onclick = () => { onRetry(); };
  card.append(ic, msg, sub, btn);
  pane.append(card);
  btn.focus();
}

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

  try {
    const results = await Promise.all([
      send({ type: 'popup:list_vaults' }).catch(() => ({ vaults: [] })),
      listAllItems(),
      send({ type: 'popup:match_active' }).catch(() => ({ items: [], origin: null })),
    ]);
    vaults = results[0];
    all = results[1];
    match = results[2];
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
document.addEventListener('click', (e) => {
  if (!vaultMenuOpen) return;
  if (e.target && e.target.closest && e.target.closest('.filter-chips-row')) return;
  closeVaultMenu();
});
function renderFilter() {
  const wrap = $('filterChips'); if (!wrap) return;
  const prev = document.activeElement;
  const hadChipFocus = !!(prev && prev.classList && prev.classList.contains('filter-chip'));
  const focusId = hadChipFocus ? prev.dataset.filterId : null;
  wrap.textContent = '';
  closeVaultMenu();
  const chips = V.buildFilterChips(S.all, S.vaults, { origin: S.origin, currentFilter: S.filter });
  const setRoving = (focused) => {
    for (const c of wrap.querySelectorAll('.filter-chip')) c.tabIndex = c === focused ? 0 : -1;
  };
  for (const f of chips) {
    const active = f.kind === 'vaults' ? S.filter.startsWith('vault:') : S.filter === f.id;
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
    } else {
      b.textContent = f.key ? t(f.key, f.label) : f.label;
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
      S.filter = f.id; renderFilter(); applyFilter(true);
    };
    b.onkeydown = (e) => {
      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft' || e.key === 'Home' || e.key === 'End') {
        e.preventDefault();
        const all = [...wrap.querySelectorAll('.filter-chip')];
        const next = all[V.chipNavIndex(all.indexOf(b), e.key, all.length)];
        if (next) { setRoving(next); next.focus(); }
      } else if (e.key === 'Escape' && vaultMenuOpen) {
        e.stopPropagation(); closeVaultMenu(); b.focus();
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

function renderRecent() {
  const sec = $('recentSection');
  const rList = $('recentList');
  if (!sec || !rList) return;
  const shouldShow = !S.query && (!S.suggested || !S.suggested.length) && (S.filter === 'all' || S.filter === 'suggested');
  if (!shouldShow) {
    sec.classList.add('hidden');
    rList.textContent = '';
    return;
  }
  const recent = V && V.getRecentlyUsed ? V.getRecentlyUsed(S.all, 5) : [];
  if (!recent.length) {
    sec.classList.add('hidden');
    rList.textContent = '';
    return;
  }
  sec.classList.remove('hidden');
  rList.textContent = '';
  const currentSelectedId = S.rows[S.sel] && S.rows[S.sel].id;
  for (let i = 0; i < recent.length; i++) {
    const it = recent[i];
    const isSel = it.id === currentSelectedId;
    const row = el('div', 'row' + (isSel ? ' sel' : ''));
    row.id = 'recent-row-' + i;
    row.setAttribute('role', 'option');
    row.setAttribute('aria-selected', isSel ? 'true' : 'false');
    row.setAttribute('tabindex', '-1');
    row.dataset.itemId = it.id;
    const ic = el('span', 'ic');
    renderRowIcon(ic, it, 20);
    const meta = el('div', 'meta');
    const tDiv = el('div', 't');
    tDiv.textContent = it.title;
    const uDiv = el('div', 'u');
    uDiv.textContent = V ? V.rowSubtitle(it, '') : (it.subtitle || '—');
    meta.append(tDiv, uDiv);
    row.append(ic, meta);
    if (it.favorite) {
      const f = el('span', 'fav');
      f.innerHTML = UI.star;
      row.append(f);
    }
    if (it.has_totp) row.append(totpChip(it));
    row.onclick = () => {
      const idx = S.rows.findIndex((r) => r.id === it.id);
      if (idx >= 0) select(idx);
    };
    row.ondblclick = () => openAndFill();
    rList.append(row);
  }
}

function applyFilter(selectFirst) {
  const prevId = S.rows[S.sel] && S.rows[S.sel].id;
  S.rows = V.filterItems(S.all, S.suggested, S.filter, S.query, S.sort);
  let idx = prevId ? S.rows.findIndex((r) => r.id === prevId) : -1;
  if (idx < 0 && (selectFirst || prevId)) idx = S.rows.length ? 0 : -1;
  $('list').scrollTop = 0;
  renderRecent();
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
  const w = V.virtualWindow(list.scrollTop, list.clientHeight || 380, ROW_H, S.rows.length);
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
    const uText = V.rowSubtitle(it, S.query);
    if (S.query && uText && uText !== '—') uDiv.append(renderHighlight(uText, S.query));
    else uDiv.textContent = uText || '—';
    meta.append(tDiv, uDiv);
    row.append(ic, meta);
    if (it.favorite) { const f = el('span', 'fav'); f.innerHTML = UI.star; row.append(f); }
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
  const ic = st.kind === 'search' ? UI.search : st.kind === 'site' ? categoryIcon('login', 36) : st.kind === 'nofill' ? UI.external : st.kind === 'vault' ? UI.plus : null;
  if (ic) { const w = el('span', 'empty-ic'); w.innerHTML = ic; box.append(w); }
  const msg = {
    search: () => t('noResults', 'No items found'),
    site: () => t('noMatches', 'No saved logins for this site'),
    nofill: () => t('notFillable', 'Open an https page to fill'),
    vault: () => t('emptyVaultTitle', 'Your vault is empty'),
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

// ---------- TOTP codes in the list ----------
// One shared timer for every visible row. Codes are fetched from the host per item (`popup:totp` returns the
// current code + seconds remaining only, never the secret) and cached until the period rolls over.
const MINI_RING_C = 2 * Math.PI * 5;

function totpRemaining(c) { return Math.max(0, Math.ceil((c.expiresAt - Date.now()) / 1000)); }
function chipsFor(id) { return [...document.querySelectorAll('#listInner [data-totp-id]')].filter((x) => x.dataset.totpId === id); }
function clearTotpCache() { S.totpCache = {}; S.totpFetching = {}; S.totpGen += 1; }

function totpRing() {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg'); svg.setAttribute('viewBox', '0 0 12 12'); svg.setAttribute('class', 'ring');
  const bg = document.createElementNS(ns, 'circle'), fg = document.createElementNS(ns, 'circle');
  for (const c of [bg, fg]) { c.setAttribute('cx', '6'); c.setAttribute('cy', '6'); c.setAttribute('r', '5'); }
  bg.setAttribute('class', 'bg'); fg.setAttribute('class', 'fg'); fg.setAttribute('stroke-dasharray', MINI_RING_C.toFixed(3));
  svg.append(bg, fg);
  return svg;
}

function paintTotpChip(chip, c) {
  const code = chip.querySelector('.totp-code'), svg = chip.querySelector('.ring');
  if (!code || !svg) return;
  const fg = svg.querySelector('.fg');
  chip.title = t('copyTotp', 'Copy TOTP');
  svg.classList.remove('hidden');
  paintRing(svg, fg, c ? totpRemaining(c) : 0, c && c.period);
  if (c && c.error) {
    code.textContent = '—'; code.classList.add('bad');
    chip.title = t('totpUnavailable', 'Code unavailable');
    svg.classList.add('hidden');
    return;
  }
  code.classList.remove('bad');
  code.textContent = c && c.code ? V.formatValue('totp', c.code) : '••• •••';
}

function paintRing(svg, fg, remaining, period) {
  fg.setAttribute('stroke-dashoffset', String(V.ringOffset(remaining, period, MINI_RING_C)));
  svg.classList.toggle('low', !!period && remaining <= 5);
}

function totpChip(it) {
  const b = el('button', 'totp-mini'); b.type = 'button'; b.dataset.totpId = it.id;
  b.setAttribute('aria-label', t('copyTotpCode', 'Copy one-time code'));
  b.append(el('span', 'totp-code'), totpRing());
  b.onclick = async (e) => {
    e.stopPropagation();
    if (b.disabled) return;
    b.disabled = true;
    try {
      await copyTotp(it.id);
    } catch (err) {
      toast(errText(err));
    } finally {
      b.disabled = false;
    }
  };
  paintTotpChip(b, S.totpCache[it.id]);
  return b;
}

async function fetchTotp(id) {
  const gen = S.totpGen;
  const r = await send({ type: 'popup:totp', item_id: id });
  const c = { code: r.code, period: r.period || 30, expiresAt: Date.now() + (Number(r.remaining) || 0) * 1000 };
  if (gen === S.totpGen) S.totpCache[id] = c; // a lock while in flight must not repopulate the cache
  return c;
}

async function copyTotp(id) {
  let c = S.totpCache[id];
  if (!c || c.error || totpRemaining(c) <= 0) c = await fetchTotp(id);
  for (const chip of chipsFor(id)) paintTotpChip(chip, c);
  await copy(c.code, t('copied', 'Copied'));
}

function startListTotp() { if (!S.listTotpTimer) { S.listTotpTimer = setInterval(tickListTotp, 1000); tickListTotp(); } }
function stopListTotp() { if (S.listTotpTimer) { clearInterval(S.listTotpTimer); S.listTotpTimer = null; } }

async function tickListTotp() {
  const gen = S.totpGen;
  const chips = [...document.querySelectorAll('#listInner [data-totp-id]')];
  if (!chips.length) return stopListTotp();
  for (const id of new Set(chips.map((x) => x.dataset.totpId))) {
    const c = S.totpCache[id];
    if (c && c.error) continue;
    if (c && totpRemaining(c) > 0) { for (const chip of chipsFor(id)) paintTotpChip(chip, c); continue; }
    if (S.totpFetching[id]) continue;
    S.totpFetching[id] = true;
    try {
      const fresh = await fetchTotp(id);
      if (gen !== S.totpGen) return stopListTotp();
      for (const chip of chipsFor(id)) paintTotpChip(chip, fresh);
    } catch (_) {
      if (gen !== S.totpGen) return stopListTotp();
      S.totpCache[id] = { error: true };
      for (const chip of chipsFor(id)) paintTotpChip(chip, S.totpCache[id]);
    } finally { delete S.totpFetching[id]; }
  }
}

let qt = null;
$('q').oninput = () => { clearTimeout(qt); qt = setTimeout(() => { S.query = $('q').value.trim(); applyFilter(true); }, 80); };

// ---------- detail ----------
let detailSeq = 0;
async function select(idx) {
  const target = S.rows[idx];
  if (S.edit) {
    if (target && target.id === S.edit.id) return;
    if (!confirmDiscard()) {
      const keep = S.rows.findIndex((r) => r.id === S.edit.id);
      if (keep >= 0) S.sel = keep;
      renderList();
      return;
    }
    S.edit = null;
  }
  S.sel = idx; S.revealed = {}; stopTotp();
  const list = $('list'), qInput = $('q');
  if (idx >= 0) {
    list.scrollTop = V.scrollToRow(idx, list.scrollTop, list.clientHeight || 380, ROW_H);
    qInput && qInput.setAttribute('aria-activedescendant', 'row-' + idx);
    list.setAttribute('aria-activedescendant', 'row-' + idx);
  } else {
    qInput && qInput.removeAttribute('aria-activedescendant');
    list.removeAttribute('aria-activedescendant');
  }
  renderList();
  renderRecent();
  const pane = $('detail');
  const it = S.rows[idx];
  if (!it) {
    pane.textContent = '';
    pane.removeAttribute('aria-busy');
    pane.removeAttribute('aria-label');
    pane.append(el('div', 'empty', t('selectItem', 'Select an item')));
    S.detail = null;
    return;
  }
  const seq = ++detailSeq;
  renderDetailSkeleton(it);

  let d, findings;
  try {
    const findingsPromise = it.category === 'login'
      ? (S.findingsCache[it.id]
          ? Promise.resolve(S.findingsCache[it.id])
          : send({ type: 'popup:item_findings', item_id: it.id }).catch(() => null))
      : Promise.resolve({ kinds: [] });

    const [res, findingsRes] = await Promise.all([
      send({ type: 'popup:get_item', item_id: it.id }),
      findingsPromise,
    ]);
    d = res.item;
    findings = findingsRes || { kinds: [] };
    if (it.category === 'login' && V.cacheableFindings(findingsRes)) {
      S.findingsCache[it.id] = findingsRes;
    }
  } catch (e) {
    if (seq !== detailSeq) return;
    pane.removeAttribute('aria-busy');
    pane.removeAttribute('aria-label');
    if (e.code === 'nohost' || /disconnected|cannot connect/i.test(e.message)) {
      return showNoHost(e);
    }
    renderDetailError(e, () => select(idx));
    return;
  }
  if (seq !== detailSeq) return;
  pane.removeAttribute('aria-busy');
  pane.removeAttribute('aria-label');
  d.findings = findings;
  S.detail = d;
  renderDetail(d);
}

function renderDetail(d) {
  const pane = $('detail'); pane.textContent = '';
  const head = el('div', 'd-head');
  const ic = el('span', 'ic'); renderRowIcon(ic, d, 28);
  const g = el('div', 'grow'); g.append(el('h2', null, d.title), el('div', 'sub', d.vault_name + ' · ' + t(V.CAT_KEY[d.category], V.CAT_LABEL[d.category])));
  head.append(ic, g, moreMenu(d));
  pane.append(head);

  if (d.category === 'login' && V.hasWatchtowerFindings(d.findings)) {
    const strip = el('div', 'warning-strip');
    strip.setAttribute('role', 'status');
    strip.setAttribute('aria-label', t('watchtowerWarnings', 'Watchtower warnings'));

    const chips = el('div', 'warning-chips');
    chips.setAttribute('role', 'list');
    for (const k of d.findings.kinds) {
      const sev = V.findingSeverity(k);
      const chip = el('span', 'warning-chip ' + sev, V.findingLabel(k, d.findings, t));
      chip.setAttribute('role', 'listitem');
      chip.setAttribute('tabindex', '0');
      chips.append(chip);
    }
    strip.append(chips);

    const warnActs = el('div', 'warning-actions');
    const genBtn = el('button', 'btn btn-primary warn-action', t('generateAndUpdate', 'Generate & update'));
    genBtn.type = 'button';
    genBtn.onclick = () => openGenerator({ updateItem: d });
    warnActs.append(genBtn);

    if (d.findings.change_password_url) {
      const siteBtn = el('button', 'btn warn-action', t('changeOnSite', 'Change on site'));
      siteBtn.type = 'button';
      siteBtn.onclick = () => {
        if (globalThis.chrome && chrome.tabs && chrome.tabs.create) {
          chrome.tabs.create({ url: d.findings.change_password_url, active: true });
        } else {
          window.open(d.findings.change_password_url, '_blank', 'noopener');
        }
      };
      warnActs.append(siteBtn);
    }

    const wtBtn = el('button', 'btn btn-link warn-action', t('openWatchtower', 'Open Watchtower'));
    wtBtn.type = 'button';
    wtBtn.onclick = () => {
      send({ type: 'popup:open_in_app', view: 'watchtower' }).catch(showErr);
    };
    warnActs.append(wtBtn);

    const appBtn = el('button', 'btn btn-link warn-action', t('openInGitpass', 'Open in gitpass'));
    appBtn.type = 'button';
    appBtn.onclick = () => {
      send({ type: 'popup:open_in_app', item_id: d.id }).catch(showErr);
    };
    warnActs.append(appBtn);

    strip.append(warnActs);
    pane.append(strip);
  }
  const acts = el('div', 'd-actions');
  if (V.fillKind(d.category)) {
    const fillLabel = d.category === 'login' && d.url ? t('openFill', 'Open & Fill') : t('fill', 'Fill');
    const b = el('button', 'btn btn-primary', fillLabel);
    b.onclick = async () => {
      if (b.disabled) return;
      b.disabled = true;
      const prevHtml = b.innerHTML;
      b.innerHTML = '<span class="spinner"></span> <span>' + fillLabel + '</span>';
      try {
        await openAndFill();
      } catch (err) {
        toast(errText(err));
      } finally {
        b.innerHTML = prevHtml;
        b.disabled = false;
      }
    };
    acts.append(b);
  }
  if (d.category === 'login') {
    const edit = el('button', 'btn', t('edit', 'Edit'));
    edit.type = 'button';
    edit.dataset.action = 'edit';
    edit.title = t('shortcutEdit', 'Edit login') + ' (⌘E)';
    edit.onclick = () => startEdit();
    acts.append(edit);
  }
  if (acts.children.length) pane.append(acts);
  const box = el('div', 'fields');
  for (const f of d.fields || []) box.append(fieldRow(d, f));
  if (box.children.length) pane.append(box);
  if (d.notes) pane.append(el('div', 'notes', d.notes));
  if (d.tags && d.tags.length) { const tg = el('div', 'tags'); for (const x of d.tags) tg.append(el('span', 'tag', x)); pane.append(tg); }
  if (d.updated_at) pane.append(el('div', 'meta-line', t('lastEdited', 'Last edited') + ' ' + new Date(d.updated_at).toLocaleString()));
}

// ---------- inline edit (logins) ----------
function editDirty() { return !!(S.edit && S.edit.dirty); }
function confirmDiscard() { return !editDirty() || window.confirm(t('discardChanges', 'Discard unsaved changes?')); }
function isHttpUrl(v) { try { const u = new URL(v); return (u.protocol === 'http:' || u.protocol === 'https:') && !!u.hostname; } catch (_) { return false; } }
function withScheme(v) { const s = String(v || '').trim(); return s && !/^[a-z][a-z0-9+.-]*:/i.test(s) ? 'https://' + s : s; }
function focusEditButton() { const b = $('detail') && $('detail').querySelector('[data-action="edit"]'); if (b) b.focus(); }

function startEdit() {
  if (!S.detail || S.detail.category !== 'login' || S.edit) return;
  S.edit = { id: S.detail.id, dirty: false, passwordLoaded: false, passwordTouched: false, saving: false };
  renderEditForm(S.detail);
}

function renderEditForm(d) {
  const pane = $('detail'); pane.textContent = '';
  const head = el('div', 'd-head');
  const ic = el('span', 'ic'); renderRowIcon(ic, d, 28);
  const g = el('div', 'grow');
  g.append(el('h2', null, t('editLogin', 'Edit login')), el('div', 'sub', d.vault_name + ' · ' + t(V.CAT_KEY[d.category], V.CAT_LABEL[d.category])));
  head.append(ic, g);
  pane.append(head);

  const form = el('form', 'stack inline-edit');
  form.autocomplete = 'off'; form.noValidate = true;
  form.style.maxWidth = 'none';
  form.setAttribute('aria-label', t('editLogin', 'Edit login'));
  const label = (text, forId, required) => {
    const l = el('label', null, text + (required ? ' *' : ''));
    l.htmlFor = forId; form.append(l); return l;
  };
  const input = (id, type, value) => {
    const i = Object.assign(el('input'), { type });
    i.id = id; i.value = value || ''; i.spellcheck = false; form.append(i); return i;
  };

  label(t('title', 'Title'), 'edit-title', true);
  const titleInput = input('edit-title', 'text', d.title);
  titleInput.setAttribute('aria-required', 'true');

  label(t('username', 'Username'), 'edit-username');
  const userField = (d.fields || []).find((f) => f.name === 'username');
  const userInput = input('edit-username', 'text', userField ? userField.value : '');

  label(t('password', 'Password'), 'edit-password');
  const pwRow = el('div', 'edit-pw');
  const pwInput = Object.assign(el('input'), { type: 'password' });
  pwInput.id = 'edit-password'; pwInput.autocomplete = 'new-password'; pwInput.spellcheck = false;
  const eye = el('button', 'icon-btn'); eye.type = 'button'; eye.innerHTML = UI.eye; eye.title = t('reveal', 'Reveal');
  eye.onclick = () => { const on = pwInput.type === 'password'; pwInput.type = on ? 'text' : 'password'; eye.innerHTML = on ? UI.eyeOff : UI.eye; };
  const gen = el('button', 'icon-btn'); gen.type = 'button'; gen.innerHTML = UI.key; gen.title = t('generate', 'Generate');
  gen.onclick = () => openGenerator((pw) => {
    pwInput.value = pw;
    pwInput.dispatchEvent(new Event('input', { bubbles: true }));
    pwInput.focus();
  });
  pwRow.append(pwInput, eye, gen);
  form.append(pwRow);
  pwInput.addEventListener('input', () => { if (S.edit) S.edit.passwordTouched = true; });

  label(t('website', 'Website'), 'edit-url-0');
  const urlsWrap = el('div', 'edit-urls');
  form.append(urlsWrap);
  const urlInputs = [];
  let urlSeq = 0;
  const addUrlRow = (value) => {
    const row = el('div', 'edit-url');
    const i = Object.assign(el('input'), { type: 'url' });
    i.id = 'edit-url-' + (urlSeq++);
    i.value = value || ''; i.spellcheck = false;
    i.setAttribute('aria-label', t('website', 'Website'));
    const rm = el('button', 'icon-btn'); rm.type = 'button'; rm.innerHTML = UI.close; rm.title = t('removeWebsite', 'Remove website');
    rm.onclick = () => {
      if (urlInputs.length === 1) { i.value = ''; i.focus(); }
      else { urlInputs.splice(urlInputs.indexOf(i), 1); row.remove(); }
      if (S.edit) S.edit.dirty = true;
    };
    urlInputs.push(i);
    row.append(i, rm);
    urlsWrap.append(row);
    return i;
  };
  const initialUrls = [];
  if (d.url) initialUrls.push(d.url);
  for (const f of d.fields || []) if (f.kind === 'url' && f.name !== 'url' && f.value && !initialUrls.includes(f.value)) initialUrls.push(f.value);
  if (!initialUrls.length) addUrlRow('');
  for (const u of initialUrls) addUrlRow(u);
  const addUrl = el('button', 'btn btn-link add-url', t('addWebsite', 'Add website'));
  addUrl.type = 'button';
  addUrl.onclick = () => { if (S.edit) S.edit.dirty = true; addUrlRow('').focus(); };
  form.append(addUrl);

  label(t('notes', 'Notes'), 'edit-notes');
  const notesInput = el('textarea');
  notesInput.id = 'edit-notes'; notesInput.rows = 3; notesInput.value = d.notes || ''; notesInput.spellcheck = false;
  form.append(notesInput);

  const errEl = el('p', 'err'); errEl.setAttribute('role', 'alert'); form.append(errEl);

  const acts = el('div', 'edit-acts');
  const save = el('button', 'btn btn-primary', t('save', 'Save')); save.type = 'submit';
  const cancel = el('button', 'btn btn-link', t('cancel', 'Cancel')); cancel.type = 'button'; cancel.onclick = () => cancelEdit();
  acts.append(save, cancel); form.append(acts);

  form.addEventListener('input', (e) => {
    if (S.edit) S.edit.dirty = true;
    if (e.target && e.target.classList) e.target.classList.remove('bad');
  });
  form.onsubmit = (e) => { e.preventDefault(); void saveEdit(d, { titleInput, userInput, pwInput, urlInputs, notesInput, errEl, save }); };
  pane.append(form);
  titleInput.focus();

  // The host never returns secrets in item details; fetch the current password for the field.
  const edit = S.edit;
  send({ type: 'popup:reveal', item_id: d.id, field: 'password' })
    .then((r) => {
      if (S.edit !== edit || edit.passwordTouched || pwInput.value) return;
      pwInput.value = r.value || '';
      edit.passwordLoaded = true;
    })
    .catch(() => {});
}

async function saveEdit(d, ui) {
  if (!S.edit || S.edit.saving) return;
  const { titleInput, userInput, pwInput, urlInputs, notesInput, errEl, save } = ui;
  errEl.textContent = '';
  titleInput.classList.remove('bad');
  for (const i of urlInputs) i.classList.remove('bad');
  const title = titleInput.value.trim();
  if (!title) { errEl.textContent = t('titleRequired', 'Title is required'); titleInput.classList.add('bad'); titleInput.focus(); return; }
  const urls = [];
  for (const i of urlInputs) {
    const raw = i.value.trim();
    if (!raw) continue;
    const u = withScheme(raw);
    if (!isHttpUrl(u)) { errEl.textContent = t('websiteInvalid', 'Website must be an http(s) URL'); i.classList.add('bad'); i.focus(); return; }
    if (!urls.includes(u)) urls.push(u);
  }
  const msg = { type: 'popup:update_login', item_id: d.id, title, username: userInput.value.trim(), urls, notes: notesInput.value };
  if (S.edit.passwordLoaded || S.edit.passwordTouched) msg.password = pwInput.value;

  S.edit.saving = true;
  save.disabled = true;
  const prevHtml = save.innerHTML;
  save.innerHTML = '<span class="spinner"></span> <span>' + t('save', 'Save') + '</span>';
  try {
    await send(msg);
    S.edit = null; livePending = false;
    toast(t('saved', 'Saved'));
    await loadData();
    if (S.sel >= 0) await select(S.sel); // loadData's selection is async; wait for the detail to render
    focusEditButton();
  } catch (e) {
    if (S.edit) S.edit.saving = false;
    save.disabled = false;
    save.innerHTML = prevHtml;
    if (e.code === 'locked') { S.edit = null; refresh(); return; }
    errEl.textContent = errText(e);
  }
}

function cancelEdit() {
  if (!S.edit) return;
  if (!confirmDiscard()) return;
  S.edit = null;
  if (S.detail) renderDetail(S.detail); else select(S.sel);
  focusEditButton();
  if (livePending) { livePending = false; liveReload(); }
}

async function secretValue(d, f) {
  if (S.revealed[f.name] != null) return S.revealed[f.name];
  if (!f.concealed) return f.value || '';
  if (f.kind === 'totp') return (await send({ type: 'popup:totp', item_id: d.id })).code;
  return (await send({ type: 'popup:reveal', item_id: d.id, field: f.name })).value;
}

function fieldRow(d, f) {
  const row = el('div', 'f'); const g = el('div', 'grow');
  g.append(el('div', 'lbl', f.label || f.name));
  const val = el('div', 'val'); g.append(val); row.append(g);
  const acts = el('div', 'acts'); row.append(acts);
  if (f.kind === 'totp') { renderTotp(d, val); }
  else if (f.concealed) {
    val.textContent = V.mask(f.kind, f.masked);
    acts.append(iconBtn(UI.eye, t('reveal', 'Reveal'), async (b) => {
      if (b.disabled) return;
      if (S.revealed[f.name] != null) { delete S.revealed[f.name]; val.textContent = V.mask(f.kind, f.masked); b.innerHTML = UI.eye; return; }
      b.disabled = true;
      const prevHtml = b.innerHTML;
      b.innerHTML = '<span class="spinner"></span>';
      try {
        const v = await secretValue(d, f);
        S.revealed[f.name] = v;
        val.textContent = V.formatValue(f.kind, v);
        b.innerHTML = UI.eyeOff;
      } catch (err) {
        b.innerHTML = prevHtml;
        toast(errText(err));
      } finally {
        b.disabled = false;
      }
    }));
  } else if (f.kind === 'url' && f.value) {
    const a = el('a', null, f.value); a.href = f.value; a.target = '_blank'; a.rel = 'noopener'; val.append(a); val.classList.add('plain');
  } else { val.textContent = V.formatValue(f.kind, f.value); if (!/password|card_number/.test(f.kind)) val.classList.add('plain'); }
  acts.append(iconBtn(UI.copy, t('copy', 'Copy'), async (b) => {
    if (b.disabled) return;
    b.disabled = true;
    const prevHtml = b.innerHTML;
    b.innerHTML = '<span class="spinner"></span>';
    try {
      const v = await secretValue(d, f);
      await copy(v, t('copied', 'Copied'));
    } catch (err) {
      toast(errText(err));
    } finally {
      b.innerHTML = prevHtml;
      b.disabled = false;
    }
  }));
  return row;
}

// TOTP ring countdown. Fix for v0.5.1 popup.js:45 null classList: the old handler kept a setInterval that
// touched an element after the list re-rendered. Now there is one timer, guarded by isConnected and cleared on every re-render.
const RING_C = 2 * Math.PI * 7;
function renderTotp(d, val) {
  stopTotp();
  const wrap = el('div', 'totp'); const code = el('span', 'totp-code', '••• •••');
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg'); svg.setAttribute('viewBox', '0 0 18 18'); svg.setAttribute('class', 'ring');
  const bg = document.createElementNS(ns, 'circle'), fg = document.createElementNS(ns, 'circle');
  for (const c of [bg, fg]) { c.setAttribute('cx', '9'); c.setAttribute('cy', '9'); c.setAttribute('r', '7'); }
  bg.setAttribute('class', 'bg'); fg.setAttribute('class', 'fg'); fg.setAttribute('stroke-dasharray', RING_C.toFixed(3));
  svg.append(bg, fg); wrap.append(code, svg); val.append(wrap);
  let st = null, fetching = false;
  const tick = async () => {
    if (!wrap.isConnected) return stopTotp();
    if (!st || st.remaining <= 0) {
      if (fetching) return; fetching = true;
      try { const r = await send({ type: 'popup:totp', item_id: d.id }); st = { code: r.code, remaining: r.remaining, period: r.period || 30 }; S.revealed.totp = r.code; }
      catch (e) { code.textContent = '—'; code.classList.add('bad'); wrap.title = t('totpUnavailable', 'Code unavailable'); svg.classList.add('hidden'); return stopTotp(); } finally { fetching = false; }
      if (!wrap.isConnected) return stopTotp();
      code.textContent = V.formatValue('totp', st.code);
    } else st.remaining -= 1;
    fg.setAttribute('stroke-dashoffset', String(V.ringOffset(st.remaining, st.period, RING_C)));
    svg.classList.toggle('low', st.remaining <= 5);
  };
  S.totpTimer = setInterval(tick, 1000);
  setTimeout(tick, 0); // the row is still detached while fieldRow builds it; the isConnected guard needs it mounted
}
function stopTotp() { if (S.totpTimer) { clearInterval(S.totpTimer); S.totpTimer = null; } }

function moreMenu(d) {
  const w = el('div', 'menu-wrap');
  const m = el('div', 'menu hidden');
  const item = (svg, label, fn) => { const b = el('button'); b.type = 'button'; b.innerHTML = svg; b.append(document.createTextNode(label)); b.onclick = (e) => { e.stopPropagation(); m.classList.add('hidden'); Promise.resolve(fn()).catch(showErr); }; m.append(b); };
  item(UI.external, t('openInApp', 'Open in gitpass'), () => send({ type: 'popup:open_in_app', item_id: d.id }));
  item(UI.link, t('copyLink', 'Copy link'), () => copy('gitpass://item/' + encodeURIComponent(d.id), t('linkCopied', 'Link copied')));
  item(UI.star, d.favorite ? t('unfavorite', 'Remove from Favorites') : t('favorite', 'Add to Favorites'), async () => {
    await send({ type: 'popup:set_favorite', item_id: d.id, favorite: !d.favorite });
    d.favorite = !d.favorite; const s = S.all.find((x) => x.id === d.id); if (s) s.favorite = d.favorite; applyFilter(false);
  });
  w.append(iconBtn(UI.more, t('more', 'More'), () => m.classList.toggle('hidden')), m);
  document.addEventListener('click', () => m.classList.add('hidden'));
  return w;
}

async function openAndFill() {
  const d = S.detail; if (!d) return;
  const kind = V.fillKind(d.category); if (!kind) return;
  await send({ type: 'popup:open_fill', item_id: d.id, kind, url: d.url || null }).then(() => window.close(), showErr);
}

async function copyPrimary() {
  const d = S.detail; const f = V.primarySecretField(d); if (!f) return;
  await copy(await secretValue(d, f), f.kind === 'password' ? t('passwordCopied', 'Password copied') : t('copied', 'Copied'));
}

async function copyUsername() {
  const selected = S.rows[S.sel];
  if (!selected) return;
  const user = V.itemUsername(S.detail, selected);
  if (user) {
    await copy(user, t('usernameCopied', 'Username copied'));
  } else {
    toast(t('noUsername', 'No username'));
  }
}

function openWebsite() {
  const selected = S.rows[S.sel];
  if (!selected) return;
  const url = (S.detail && S.detail.url) || selected.url || '';
  if (url && V.isAllowedUrl(url)) {
    if (typeof chrome !== 'undefined' && chrome.tabs && typeof chrome.tabs.create === 'function') {
      chrome.tabs.create({ url, active: true });
    } else {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  }
}

// ---------- list row quick actions (issue #255) ----------
// Copy actions reuse copyUsername()/copyPrimary(), so concealed values and
// the 30 s clipboard auto-clear behave exactly as today, and no value is
// fetched until the button is clicked (select() loads the detail, then the
// copy path reveals only that one field). Copy number (cards) and Copy
// credential (API credentials) are both the primary-secret path.
const QUICK_ACTION_META = {
  copyUsername: ['copyUsername', 'Copy username', 'copy'],
  copyPassword: ['copyPassword', 'Copy password', 'copy'],
  copyNumber: ['copyNumber', 'Copy number', 'copy'],
  copyCredential: ['copyCredential', 'Copy credential', 'copy'],
  openWebsite: ['openWebsite', 'Open website', 'external'],
};

async function runQuickAction(action) {
  if (action === 'copyUsername') await copyUsername();
  else if (action === 'openWebsite') openWebsite();
  else await copyPrimary();
}

function quickActionsEl(it, idx, isSel, actions) {
  const wrap = el('span', 'quick-actions');
  for (const action of actions) {
    const [key, fallback, icon] = QUICK_ACTION_META[action] || [action, action, 'copy'];
    const label = t(key, fallback);
    const b = el('button', 'quick-btn');
    b.type = 'button';
    b.dataset.action = action;
    b.setAttribute('aria-label', label);
    b.title = label;
    b.tabIndex = isSel ? 0 : -1;
    b.innerHTML = UI[icon] || UI.copy;
    b.onclick = async (e) => {
      e.stopPropagation();
      try {
        // The copy paths work off the selected row + its detail, so select
        // first when this row is not the selected one, then run the action.
        if (S.sel !== idx) await select(idx);
        await runQuickAction(action);
      } catch (err) { showErr(err); }
      // select() re-rendered the row; put focus back on this button so a
      // keyboard user stays in the quick actions.
      try {
        const row = document.querySelector('#row-' + idx);
        const btn = row && row.querySelector('.quick-btn[data-action="' + action + '"]');
        if (btn) btn.focus();
      } catch (_) {}
    };
    b.ondblclick = (e) => e.stopPropagation();
    wrap.append(b);
  }
  return wrap;
}

// ---------- panels: generator / new login / shortcuts ----------
let prevFocus = null;

function closePanel() {
  const p = $('panel');
  p.classList.add('hidden');
  p.classList.remove('shortcuts-panel');
  p.textContent = '';
  p.removeAttribute('aria-modal');
  p.removeAttribute('aria-label');
  const wasShortcuts = S.panel === 'shortcuts';
  S.panel = null;
  if (wasShortcuts && prevFocus && typeof prevFocus.focus === 'function' && prevFocus.isConnected) {
    prevFocus.focus();
  } else {
    $('q').focus();
  }
  prevFocus = null;
  if (livePending) { livePending = false; liveReload(); }
}

function panelHead(title) { const h = el('h3', null, title); h.append(iconBtn(UI.close, t('close', 'Close'), closePanel)); return h; }

function openShortcuts() {
  prevFocus = document.activeElement;
  const p = $('panel');
  p.textContent = '';
  p.classList.remove('hidden');
  p.classList.add('shortcuts-panel');
  p.setAttribute('role', 'dialog');
  p.setAttribute('aria-modal', 'true');
  p.setAttribute('aria-label', t('keyboardShortcuts', 'Keyboard shortcuts'));
  S.panel = 'shortcuts';

  p.append(panelHead(t('keyboardShortcuts', 'Keyboard shortcuts')));

  const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent);
  const cmd = isMac ? '⌘' : 'Ctrl+';
  const shift = isMac ? '⇧' : 'Shift+';

  const list = el('div', 'shortcuts-list');
  list.setAttribute('role', 'list');

  const shortcuts = [
    { keys: ['↑', '↓'], label: t('shortcutNav', 'Navigate list') },
    { keys: ['←', '→'], label: t('shortcutFilterChips', 'Move between filter chips') },
    { keys: [isMac ? '↵' : 'Enter'], label: t('shortcutFill', 'Fill item') },
    { keys: [`${cmd}C`], label: t('shortcutCopyPw', 'Copy password') },
    { keys: [`${cmd}${shift}C`], label: t('shortcutCopyTotp', 'Copy one-time code (TOTP)') },
    { keys: [`${cmd}${shift}U`], label: t('shortcutCopyUser', 'Copy username') },
    { keys: [`${cmd}O`], label: t('shortcutOpenUrl', 'Open website') },
    { keys: [`${cmd}E`], label: t('shortcutEdit', 'Edit login') },
    { keys: [`${cmd}${shift}G`], label: t('shortcutGen', 'Password generator') },
    { keys: ['Esc'], label: t('shortcutClose', 'Close or clear search') },
  ];

  for (const s of shortcuts) {
    const row = el('div', 'shortcut-row');
    row.setAttribute('role', 'listitem');
    const lbl = el('span', 'shortcut-label', s.label);
    const keysWrap = el('span', 'shortcut-keys');
    for (const k of s.keys) {
      const kbd = document.createElement('kbd');
      kbd.textContent = k;
      keysWrap.append(kbd);
    }
    row.append(lbl, keysWrap);
    list.append(row);
  }

  p.append(list);

  const closeBtn = p.querySelector('button');
  if (closeBtn) closeBtn.focus();
}

async function generate(len, digits, symbols, extra) {
  const msg = { type: 'popup:generate', len, digits, symbols };
  if (extra && extra.words) msg.words = true;
  if (extra && extra.pin) msg.pin = true;
  return (await send(msg)).password;
}

let genSeq = 0;
const GEN_LABELS = { random: ['genModeRandom', 'Random'], memorable: ['genModeMemorable', 'Memorable'], pin: ['genModePin', 'PIN'] };
async function openGenerator(opts) {
  const onUse = typeof opts === 'function' ? opts : (opts && opts.onUse);
  const updateItem = opts && typeof opts === 'object' ? opts.updateItem : null;
  const p = $('panel'); p.textContent = ''; p.classList.remove('hidden'); S.panel = 'gen';
  const saved = await generatorSettings();
  if (S.panel !== 'gen') return; // closed while settings loaded
  const st = { ...saved };
  let pw = '';

  p.append(panelHead(t('generator', 'Password generator')));
  const out = el('div', 'gen-out');
  const code = el('code', 'gen-code');
  out.append(code, iconBtn(UI.regen, t('regenerate', 'Regenerate'), () => run()));
  p.append(out);

  const meter = el('div', 'gen-strength');
  meter.setAttribute('role', 'meter');
  meter.setAttribute('aria-valuemin', '0'); meter.setAttribute('aria-valuemax', '100');
  const meterBar = el('div', 'gen-strength-bar');
  const meterFill = el('div', 'gen-strength-fill');
  const meterLab = el('span', 'gen-strength-lab');
  meterBar.append(meterFill); meter.append(meterBar, meterLab);
  p.append(meter);

  const errEl = el('p', 'err'); errEl.setAttribute('role', 'alert'); p.append(errEl);

  const tabs = el('div', 'gen-modes');
  tabs.setAttribute('role', 'tablist'); tabs.setAttribute('aria-label', t('generator', 'Password generator'));
  const tabBtns = {};
  for (const m of ['random', 'memorable', 'pin']) {
    const [key, fallback] = GEN_LABELS[m];
    const b = el('button', 'gen-mode', t(key, fallback));
    b.type = 'button'; b.setAttribute('role', 'tab');
    b.onclick = () => setMode(m);
    tabs.append(b); tabBtns[m] = b;
  }
  p.append(tabs);

  const rowLen = el('div', 'gen-row');
  const lenLab = el('label');
  const range = Object.assign(el('input'), { type: 'range' });
  const num = Object.assign(el('input', 'gen-num'), { type: 'number' });
  rowLen.append(lenLab, range, num); p.append(rowLen);

  const rowTog = el('div', 'gen-row gen-toggles');
  const mkTog = (labelKey, fallback) => {
    const l = el('label', 'tog');
    const c = Object.assign(el('input'), { type: 'checkbox' });
    l.append(c, el('span', null, t(labelKey, fallback)));
    rowTog.append(l);
    return c;
  };
  const togDigits = mkTog('genIncludeDigits', 'Include digits');
  const togSymbols = mkTog('genIncludeSymbols', 'Include symbols');
  p.append(rowTog);

  const acts = el('div', 'gen-acts');
  const fillBtn = el('button', 'btn hidden', t('fillTitle', 'Fill on this page')); fillBtn.type = 'button';
  const saveBtn = el('button', 'btn btn-link', t('genSaveAsLogin', 'Save as new Login…')); saveBtn.type = 'button';
  let useBtn = null;
  if (onUse) {
    useBtn = el('button', 'btn', t('useGenerated', 'Use password')); useBtn.type = 'button';
    useBtn.onclick = () => { if (!pw) return; const v = pw; closePanel(); onUse(v); };
  }
  let updateBtn = null;
  if (updateItem) {
    updateBtn = el('button', 'btn btn-primary', t('updatePassword', 'Update password'));
    updateBtn.type = 'button';
    updateBtn.onclick = async () => {
      if (!pw || updateBtn.disabled) return;
      updateBtn.disabled = true;
      const prevHtml = updateBtn.innerHTML;
      updateBtn.innerHTML = '<span class="spinner"></span> <span>' + t('saving', 'Saving…') + '</span>';
      try {
        await send({ type: 'popup:update_login', item_id: updateItem.id, password: pw });
        toast(t('passwordUpdated', 'Password updated'));
        closePanel();
        // Reuse findings change for every login that shared the old password.
        S.findingsCache = {};
        await select(S.sel);
        renderList();
      } catch (err) {
        toast(errText(err));
      } finally {
        updateBtn.innerHTML = prevHtml;
        updateBtn.disabled = false;
      }
    };
  }
  const copyBtn = el('button', 'btn ' + (updateItem ? '' : 'btn-primary'), t('copy', 'Copy')); copyBtn.type = 'button';
  acts.append(fillBtn, saveBtn);
  if (useBtn) acts.append(useBtn);
  if (updateBtn) acts.append(updateBtn);
  acts.append(copyBtn); p.append(acts);
  p.append(recentSection());

  const persist = () => saveGeneratorSettings(st).catch(() => {});
  let persistT = null;
  const persistSoon = () => { clearTimeout(persistT); persistT = setTimeout(persist, 250); }; // slider drag: one write
  const limits = () => GEN_MODES[st.mode];
  const label = () => st.mode === 'memorable' ? t('genWords', 'Words') : st.mode === 'pin' ? t('genDigits', 'Digits') : t('length', 'Length');

  function syncControls() {
    for (const [m, b] of Object.entries(tabBtns)) { const on = m === st.mode; b.classList.toggle('sel', on); b.setAttribute('aria-selected', String(on)); }
    const lim = limits();
    for (const i of [range, num]) { i.min = lim.min; i.max = lim.max; i.value = st.length; i.setAttribute('aria-label', label()); }
    lenLab.textContent = label();
    rowTog.classList.toggle('hidden', st.mode === 'pin');
    togDigits.checked = st.digits; togSymbols.checked = st.symbols;
  }
  function setMode(m) {
    st.mode = genMode(m);
    st.length = clampGenLength(st.mode, st.length);
    syncControls(); persist(); void run();
  }
  function setLength(v) { st.length = clampGenLength(st.mode, v); syncControls(); persistSoon(); void run(); }

  function paintCode() {
    code.textContent = '';
    for (const ch of pw) code.append(el('span', /[0-9]/.test(ch) ? 'num' : /[^a-zA-Z0-9]/.test(ch) ? 'sym' : null, ch));
  }
  function paintStrength() {
    const score = strengthScore(pw);
    const key = strengthKey(score);
    meterFill.className = 'gen-strength-fill ' + key;
    meterFill.style.width = score + '%';
    const text = t('genStrength' + key[0].toUpperCase() + key.slice(1), key[0].toUpperCase() + key.slice(1));
    meterLab.textContent = text;
    meter.setAttribute('aria-valuenow', String(score));
    meter.setAttribute('aria-valuetext', text);
    for (const b of [fillBtn, saveBtn, copyBtn, useBtn, updateBtn].filter(Boolean)) b.disabled = !pw;
  }

  async function run() {
    const seq = ++genSeq;
    errEl.textContent = '';
    code.textContent = '…';
    try {
      const r = await send({ type: 'popup:generate', len: st.length, digits: st.mode === 'pin' ? false : st.digits, symbols: st.mode === 'pin' ? false : st.symbols, words: st.mode === 'memorable', pin: st.mode === 'pin' });
      if (seq !== genSeq || S.panel !== 'gen') return;
      pw = String((r && r.password) || '');
      paintCode(); paintStrength();
    } catch (e) {
      if (seq !== genSeq || S.panel !== 'gen') return;
      pw = ''; paintCode();
      errEl.textContent = e && e.code === 'locked' ? t('errLocked', 'gitpass is locked. Unlock it to continue.')
        : e && e.code === 'app_not_running' ? t('errAppDown', 'The gitpass app is not running.') : errText(e);
      paintStrength();
    }
  }

  range.oninput = () => setLength(range.value);
  num.onchange = () => setLength(num.value);
  togDigits.onchange = () => { st.digits = togDigits.checked; persist(); void run(); };
  togSymbols.onchange = () => { st.symbols = togSymbols.checked; persist(); void run(); };
  copyBtn.onclick = async () => {
    if (copyBtn.disabled) return;
    copyBtn.disabled = true;
    const prevHtml = copyBtn.innerHTML;
    copyBtn.innerHTML = '<span class="spinner"></span> <span>' + t('copy', 'Copy') + '</span>';
    try {
      await copy(code.textContent, t('passwordCopied', 'Password copied'));
      void send({ type: 'popup:record_generated', password: code.textContent }).catch(() => {});
    } catch (err) {
      toast(errText(err));
    } finally {
      copyBtn.innerHTML = prevHtml;
      copyBtn.disabled = false;
    }
  };
  fillBtn.onclick = () => send({ type: 'popup:fill_generated', password: pw })
    .then(() => { toast(t('genFilled', 'Filled on this page')); window.close(); })
    .catch(() => toast(t('genFillFailed', 'Could not fill on this page')));
  saveBtn.onclick = () => openNewItem('login', { title: S.origin ? V.hostOfUrl(S.origin) : '', url: S.origin || '', password: pw });

  send({ type: 'popup:can_fill_generated' })
    .then((r) => { if (S.panel === 'gen' && fillBtn.isConnected) fillBtn.classList.toggle('hidden', !(r && r.canFill)); })
    .catch(() => {});
  syncControls();
  paintStrength();
  void run();
}

/** Collapsible "Recent" list of generated passwords (issue #226). Values come from the read-only host command
 *  `generator_history` (popup only), are masked until revealed per row, and are fetched only when expanded. */
function recentSection() {
  const box = el('section', 'gen-recent');
  const head = el('button', 'gen-recent-toggle'); head.type = 'button';
  head.setAttribute('aria-expanded', 'false');
  const headLab = el('span', null, t('genRecent', 'Recent'));
  const chev = el('span', 'gen-recent-chev'); chev.innerHTML = UI.chevron;
  head.append(headLab, chev);
  const body = el('div', 'gen-recent-body hidden');
  body.id = 'genRecentBody'; head.setAttribute('aria-controls', body.id);
  box.append(head, body);
  let loaded = false;
  let canFill = false;

  const note = (text) => { body.textContent = ''; body.append(el('p', 'muted gen-recent-note', text)); };
  const rows = () => [...body.querySelectorAll('.gen-recent-row')];
  function focusRow(i) {
    const rs = rows(); if (!rs.length) return;
    const n = Math.max(0, Math.min(rs.length - 1, i));
    rs.forEach((r, k) => { r.tabIndex = k === n ? 0 : -1; });
    rs[n].focus();
  }
  function paintRow(e, nowSecs) {
    const li = el('li', 'gen-recent-row'); li.tabIndex = -1; li.dataset.id = String(e.id);
    let shown = false;
    const val = el('code', 'gen-recent-val', G.maskValue(e.value));
    const age = G.ageParts(e.created_at, nowSecs);
    const ageText = age.key === 'genAgoNow' ? t('genAgoNow', 'just now')
      : t(age.key, age.key === 'genAgoMin' ? '$1 min ago' : age.key === 'genAgoHour' ? '$1 h ago' : '$1 d ago', [String(age.n)]);
    const meta = el('span', 'gen-recent-meta', (e.host ? e.host + ' · ' : '') + ageText);
    const info = el('div', 'gen-recent-info'); info.append(val, meta);
    const toggle = () => {
      shown = !shown;
      val.textContent = shown ? e.value : G.maskValue(e.value);
      eyeB.innerHTML = shown ? UI.eyeOff : UI.eye;
      eyeB.setAttribute('aria-pressed', String(shown));
      eyeB.title = shown ? t('genRecentHide', 'Hide') : t('reveal', 'Reveal');
    };
    const doCopy = () => copy(e.value, t('passwordCopied', 'Password copied'));
    const copyB = iconBtn(UI.copy, t('copy', 'Copy'), doCopy);
    const eyeB = iconBtn(UI.eye, t('reveal', 'Reveal'), toggle); eyeB.setAttribute('aria-pressed', 'false');
    const acts = el('div', 'gen-recent-acts'); acts.append(copyB, eyeB);
    if (canFill && G.hostMatches(e.host, S.origin)) {
      acts.append(iconBtn(UI.external, t('fillTitle', 'Fill on this page'), () => send({ type: 'popup:fill_generated', password: e.value })
        .then(() => { toast(t('genFilled', 'Filled on this page')); window.close(); })
        .catch(() => toast(t('genFillFailed', 'Could not fill on this page')))));
    }
    li.append(info, acts);
    li.addEventListener('keydown', (ev) => {
      if (ev.target !== li) return;
      if (ev.key === 'ArrowDown') { ev.preventDefault(); focusRow(rows().indexOf(li) + 1); }
      else if (ev.key === 'ArrowUp') { ev.preventDefault(); focusRow(rows().indexOf(li) - 1); }
      else if (ev.key === 'Enter') { ev.preventDefault(); void doCopy(); }
      else if (ev.key === 'r' || ev.key === 'R') { ev.preventDefault(); toggle(); }
    });
    return li;
  }
  async function load() {
    note('…');
    let r;
    try { r = await send({ type: 'popup:generator_history' }); }
    catch (err) {
      if (err && err.code === 'locked') { box.classList.add('hidden'); return; } // locked: section hidden
      note(errText(err)); return;
    }
    if (S.panel !== 'gen' || !box.isConnected) return;
    const h = G.cleanHistory(r, 10);
    if (!h.enabled) return note(t('genRecentOff', 'Generator history is off'));
    if (!h.entries.length) return note(t('genRecentEmpty', 'No generated passwords yet'));
    try { canFill = !!((await send({ type: 'popup:can_fill_generated' })) || {}).canFill; } catch (_) { canFill = false; }
    if (S.panel !== 'gen' || !box.isConnected) return;
    const ul = el('ul', 'gen-recent-list'); ul.setAttribute('role', 'list');
    const now = Math.floor(Date.now() / 1000);
    for (const e of h.entries) ul.append(paintRow(e, now));
    body.textContent = '';
    body.append(ul);
    const all = el('button', 'btn-link gen-recent-all', t('genRecentViewAll', 'View all in gitpass')); all.type = 'button';
    all.onclick = () => send({ type: 'popup:open_in_app' }).catch(showErr);
    body.append(all);
    const first = ul.firstElementChild; if (first) first.tabIndex = 0;
  }
  head.onclick = () => {
    const open = head.getAttribute('aria-expanded') !== 'true';
    head.setAttribute('aria-expanded', String(open));
    body.classList.toggle('hidden', !open);
    chev.classList.toggle('open', open);
    if (open && !loaded) { loaded = true; void load(); }
  };
  return box;
}

/** New item: every type the desktop app supports (except passkeys, which websites create, and documents,
 *  which need a file). Fields/ids come from popup/newitem.js (mirrors the GUI templates); saved via `create_item`. */
function openNewItem(typeId, prefill) {
  const N = GitpassNewItem;
  const pre = prefill || {};
  const p = $('panel'); p.textContent = ''; p.classList.remove('hidden'); S.panel = 'new';
  const type = N.typeById(typeId || S.newType || 'login'); S.newType = type.id;
  p.append(panelHead(t('newItem', 'New item')));
  const f = el('form', 'stack newitem'); f.style.maxWidth = 'none'; f.autocomplete = 'off'; f.noValidate = true;
  const ts = el('select'); ts.setAttribute('aria-label', t('itemType', 'Item type'));
  for (const x of N.TYPES) { const o = el('option', null, x.label); o.value = x.id; if (x.id === type.id) o.selected = true; ts.append(o); }
  ts.onchange = () => openNewItem(ts.value);
  f.append(el('label', null, t('itemType', 'Type')), ts);
  const genLink = el('button', 'btn-link', t('genOpen', 'Generate a password…'));
  genLink.type = 'button'; genLink.onclick = () => openGenerator();
  f.append(genLink);
  const inputs = {};
  const add = (id, label, ui, val, opt) => {
    const o = opt || {};
    const lab = el('label', null, label + (o.required ? ' *' : '')); f.append(lab);
    let i;
    if (ui === 'multiline') { i = el('textarea'); i.rows = id === 'private_key' || id === 'note' ? 5 : 3; if (o.secret) i.classList.add('secret'); }
    else i = Object.assign(el('input'), { type: ui === 'url' ? 'url' : ui === 'email' ? 'email' : ui === 'date' ? 'date' : ui === 'phone' ? 'tel' : 'text' });
    i.value = val || ''; i.spellcheck = false; i.dataset.field = id; i.setAttribute('aria-label', label);
    if (o.inputmode) i.inputMode = o.inputmode;
    if (ui === 'month_year') i.placeholder = 'MM/YY';
    if (o.hint) i.placeholder = o.hint;
    if (ui === 'secret' || ui === 'password') {
      const w = el('div', 'field'); i.type = 'password'; i.autocomplete = 'new-password';
      const eye = el('button', 'eye'); eye.type = 'button'; eye.innerHTML = UI.eye || UI.key; eye.title = t('show', 'Show');
      eye.onclick = () => { i.type = i.type === 'password' ? 'text' : 'password'; };
      w.append(i, eye);
      if (ui === 'password') { i.type = 'text'; const g = el('button', 'eye gen'); g.type = 'button'; g.innerHTML = UI.key; g.title = t('generate', 'Generate'); g.onclick = async () => { try { i.value = await generate(20, true, true); } catch (e) { showErr(e); } }; w.replaceChildren(i, g); }
      f.append(w);
    } else f.append(i);
    inputs[id] = i; return i;
  };
  const cap = (x) => x.charAt(0).toUpperCase() + x.slice(1);
  add('title', type.titleLabel || 'Title', 'text', pre.title != null ? String(pre.title) : (type.id === 'login' && S.origin ? V.hostOfUrl(S.origin) : ''), { required: true });
  if (type.url) add('url', t('website', 'Website'), 'url', pre.url != null ? String(pre.url) : (S.origin || ''));
  for (const sec of N.sectionsOf(type)) {
    if (sec.title) f.append(el('div', 'sec-title', sec.title));
    for (const fl of sec.fields) add(fl.id, fl.hintLabel || cap(fl.label), fl.ui, pre[fl.id] != null ? String(pre[fl.id]) : '', fl);
  }
  if (!type.noNotes) add('notes', t('notes', 'Notes'), 'multiline', '');
  const vs = el('select'); vs.setAttribute('aria-label', t('vault', 'Vault'));
  for (const v of S.vaults) { const o = el('option', null, v.name); o.value = v.id; vs.append(o); }
  if (S.vaults.length) { f.append(el('label', null, t('vault', 'Vault'))); f.append(vs); }
  const errEl = el('p', 'err'); errEl.setAttribute('role', 'alert'); f.append(errEl);
  const save = el('button', 'btn btn-primary', t('save', 'Save')); save.type = 'submit'; f.append(save);
  f.onsubmit = async (e) => {
    e.preventDefault(); errEl.textContent = '';
    for (const i of Object.values(inputs)) i.classList.remove('bad');
    const values = {}; for (const [k, i] of Object.entries(inputs)) values[k] = i.value;
    const bad = N.validate(type.id, values);
    if (bad) { errEl.textContent = bad.error; if (inputs[bad.field]) { inputs[bad.field].classList.add('bad'); inputs[bad.field].focus(); } return; }
    if (save.disabled) return;
    save.disabled = true;
    const prevHtml = save.innerHTML;
    save.innerHTML = '<span class="spinner"></span> <span>' + t('save', 'Save') + '</span>';
    try {
      const r = await send({ type: 'popup:create_item', item: N.buildItem(type.id, values, vs.value || undefined) });
      toast(t('saved', 'Saved') + ' — ' + (r && r.title ? r.title : values.title));
      closePanel();
      await loadData();
    } catch (err) {
      errEl.textContent = errText(err);
      toast(errText(err));
    } finally {
      save.innerHTML = prevHtml;
      save.disabled = false;
    }
  };
  p.append(f);
  const first = type.id === 'login' ? inputs.username : (inputs.title.value ? Object.values(inputs)[1] : inputs.title);
  (first || inputs.title).focus();
  if (type.id === 'login' && inputs.password && !inputs.password.value) {
    generate(20, true, true)
      .then((v) => { if (!inputs.password.value) inputs.password.value = v; })
      .catch((e) => showErr(e));
  }
}

$('genBtn').innerHTML = UI.key; $('genBtn').onclick = () => (S.panel === 'gen' ? closePanel() : openGenerator());
$('newBtn').innerHTML = UI.plus; $('newBtn').onclick = () => (S.panel === 'new' ? closePanel() : openNewItem());
$('lock').innerHTML = UI.lock; $('searchIc').innerHTML = UI.search;
if ($('searchHints')) $('searchHints').onclick = () => (S.panel === 'shortcuts' ? closePanel() : openShortcuts());

// ---------- keyboard ----------
document.addEventListener('keydown', (e) => {
  if ($('unlocked').classList.contains('hidden')) { if (e.key === 'Escape') window.close(); return; }
  const a = document.activeElement;
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
  // ⌘E edits the selected login inline.
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && (e.key === 'e' || e.key === 'E') && !inForm && S.detail && S.detail.category === 'login') {
    e.preventDefault(); startEdit(); return;
  }
  const sel = window.getSelection && String(window.getSelection());
  const inputSel = a && a.tagName === 'INPUT' && a.selectionStart !== a.selectionEnd;
  const selected = S.rows[S.sel];
  const act = V.keyAction(e, { panel: S.panel, inForm, inEditable, hasSelection: !!sel || inputSel, hasTotp: !!(selected && selected.has_totp), query: S.query });
  if (!act) return;
  e.preventDefault();
  if (act === 'close') window.close();
  else if (act === 'closePanel') closePanel();
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
  else if (act === 'openWebsite') openWebsite();
  else if (act === 'generator') { if (S.panel === 'gen') closePanel(); else openGenerator(); }
});

// ---------- user settings (options page) ----------
function popupSettings() {
  return new Promise((res) => {
    try { chrome.storage.local.get('settings', (r) => res(Object.assign({ autofill: true, savePrompts: true, autolockFollowsApp: true }, (r && r.settings) || {}))); }
    catch (_) { res({ autofill: true, savePrompts: true, autolockFollowsApp: true }); }
  });
}

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
      clearClipboardIfOurs();
      popupSettings().then((st) => { if (st.autolockFollowsApp !== false) refresh(); });
    }
  });
}

if (typeof document !== 'undefined') {
  document.addEventListener('pagehide', () => { stopListTotp(); stopTotp(); });
  GitpassI18n.apply(document);
  refresh();
}

if (typeof window !== 'undefined' && window.__gitpassMock) {
  window.__gitpassPopup = {
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
    renderRecent,
    S,
  };
}
