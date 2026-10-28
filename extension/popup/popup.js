/* global chrome, GitpassI18n, GitpassPopupIcons, GitpassView, GitpassNewItem */
'use strict';
// gitpass popup v0.6: 1Password-style 2-pane UI. All host calls go through background.js as `popup:*` messages.
const $ = (id) => document.getElementById(id);
const t = (k, f, subs) => GitpassI18n.t(k, f, subs);
const V = typeof GitpassView !== 'undefined' ? GitpassView : (typeof require !== 'undefined' ? require('./view.js') : null);
const { UI, categoryIcon } = typeof GitpassPopupIcons !== 'undefined' ? GitpassPopupIcons : (typeof require !== 'undefined' ? require('./icons.js') : { UI: {} });
const ROW_H = 52;

function send(msg, timeoutMs) {
  const ms = typeof timeoutMs === 'number'
    ? timeoutMs
    : (msg && typeof msg.__timeoutMs === 'number' ? msg.__timeoutMs : (V ? V.REQUEST_TIMEOUT_MS : 8000));
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
const S = { all: [], suggested: [], origin: null, vaults: [], filter: 'suggested', query: '', rows: [], sel: -1, detail: null, revealed: {}, totpCache: {}, totpFetching: {}, totpGen: 0, totpTimer: null, listTotpTimer: null, panel: null, view: null, neverSave: [] };

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
  S.revealed = {}; $('detail').textContent = '';
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
async function loadData() {
  startListLoading();
  let listError = null;
  let vaults = { vaults: [] };
  let all = null;
  let match = { items: [], origin: null };

  try {
    const results = await Promise.all([
      send({ type: 'popup:list_vaults' }).catch(() => ({ vaults: [] })),
      send({ type: 'popup:list_items' }),
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

function renderFilter() {
  const s = $('filter'); s.textContent = '';
  let group = null, parent = s;
  for (const f of V.buildFilters(S.vaults)) {
    if (f.group !== group) { group = f.group; parent = group ? Object.assign(el('optgroup'), { label: group === 'category' ? t('categories', 'Categories') : t('vaults', 'Vaults') }) : s; if (group) s.append(parent); }
    const o = el('option', null, f.key ? t(f.key, f.label) : f.label); o.value = f.id;
    if (f.id === 'suggested' && S.origin) o.textContent += ' · ' + V.hostOfUrl(S.origin);
    parent.append(o);
  }
  s.value = S.filter;
  const noteEl = $('siteExceptedNote');
  if (noteEl) {
    const showNote = S.filter === 'suggested' && isSiteExcepted();
    noteEl.classList.toggle('hidden', !showNote);
  }
}
$('filter').onchange = () => { S.filter = $('filter').value; applyFilter(true); };
const undoExceptedBtn = $('undoExcepted');
if (undoExceptedBtn) {
  undoExceptedBtn.onclick = async () => {
    await removeNeverSave(GitpassPolicy.registrableDomain(S.origin));
    toast(t('saved', 'Saved'));
    renderFilter();
    applyFilter(false);
  };
}

function applyFilter(selectFirst) {
  const prevId = S.rows[S.sel] && S.rows[S.sel].id;
  S.rows = V.filterItems(S.all, S.suggested, S.filter, S.query);
  let idx = prevId ? S.rows.findIndex((r) => r.id === prevId) : -1;
  if (idx < 0 && (selectFirst || prevId)) idx = S.rows.length ? 0 : -1;
  $('list').scrollTop = 0;
  renderList();
  select(idx);
}

function renderList() {
  const list = $('list'), inner = $('listInner');
  inner.textContent = '';
  if (!S.rows.length) { stopListTotp(); renderEmpty(inner); return; }
  const w = V.virtualWindow(list.scrollTop, list.clientHeight || 380, ROW_H, S.rows.length);
  inner.style.paddingTop = w.padTop + 'px'; inner.style.paddingBottom = w.padBottom + 'px';
  for (let i = w.start; i < w.end; i++) {
    const it = S.rows[i];
    const row = el('div', 'row' + (i === S.sel ? ' sel' : '')); row.setAttribute('role', 'option'); row.dataset.idx = i;
    const ic = el('span', 'ic'); ic.innerHTML = categoryIcon(it.category, 32);
    const meta = el('div', 'meta'); meta.append(el('div', 't', it.title), el('div', 'u', it.subtitle || '—'));
    row.append(ic, meta);
    if (it.favorite) { const f = el('span', 'fav'); f.innerHTML = UI.star; row.append(f); }
    if (it.has_totp) row.append(totpChip(it));
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
  S.sel = idx; S.revealed = {}; stopTotp();
  const list = $('list');
  if (idx >= 0) list.scrollTop = V.scrollToRow(idx, list.scrollTop, list.clientHeight || 380, ROW_H);
  renderList();
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

  let d;
  try {
    const res = await send({ type: 'popup:get_item', item_id: it.id });
    d = res.item;
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
  S.detail = d;
  renderDetail(d);
}

function renderDetail(d) {
  const pane = $('detail'); pane.textContent = '';
  const head = el('div', 'd-head');
  const ic = el('span', 'ic'); ic.innerHTML = categoryIcon(d.category, 44);
  const g = el('div', 'grow'); g.append(el('h2', null, d.title), el('div', 'sub', d.vault_name + ' · ' + t(V.CAT_KEY[d.category], V.CAT_LABEL[d.category])));
  head.append(ic, g, moreMenu(d));
  pane.append(head);
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
  if (acts.children.length) pane.append(acts);
  const box = el('div', 'fields');
  for (const f of d.fields || []) box.append(fieldRow(d, f));
  if (box.children.length) pane.append(box);
  if (d.notes) pane.append(el('div', 'notes', d.notes));
  if (d.tags && d.tags.length) { const tg = el('div', 'tags'); for (const x of d.tags) tg.append(el('span', 'tag', x)); pane.append(tg); }
  if (d.updated_at) pane.append(el('div', 'meta-line', t('lastEdited', 'Last edited') + ' ' + new Date(d.updated_at).toLocaleString()));
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

// ---------- panels: generator / new login ----------
function closePanel() { const p = $('panel'); p.classList.add('hidden'); p.textContent = ''; S.panel = null; $('q').focus(); if (livePending) { livePending = false; liveReload(); } }
function panelHead(title) { const h = el('h3', null, title); h.append(iconBtn(UI.close, t('close', 'Close'), closePanel)); return h; }

function localGenerate(len, digits, symbols) {
  let set = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ';
  if (digits) set += '23456789'; if (symbols) set += '!@#$%^&*-_=+?';
  const out = [], buf = new Uint8Array(1), lim = 256 - (256 % set.length);
  while (out.length < len) { crypto.getRandomValues(buf); if (buf[0] < lim) out.push(set[buf[0] % set.length]); }
  return out.join('');
}
async function generate(len, digits, symbols) {
  try { return (await send({ type: 'popup:generate', len, digits, symbols })).password; } catch (_) { return localGenerate(len, digits, symbols); }
}

function openGenerator() {
  const p = $('panel'); p.textContent = ''; p.classList.remove('hidden'); S.panel = 'gen';
  p.append(panelHead(t('generator', 'Password generator')));
  const out = el('div', 'gen-out'); const code = el('code'); out.append(code);
  const regen = iconBtn(UI.regen, t('regenerate', 'Regenerate'), () => run()); out.append(regen); p.append(out);
  const r1 = el('div', 'gen-row'); const lab = el('label', null, t('length', 'Length')); const len = Object.assign(el('input'), { type: 'range', min: 8, max: 64, value: 20 }); const lv = el('span', null, '20'); r1.append(lab, len, lv); p.append(r1);
  const r2 = el('div', 'gen-row');
  const tog = (key, fb) => { const l = el('label', 'tog'); const c = Object.assign(el('input'), { type: 'checkbox', checked: true }); l.append(c, el('span', null, t(key, fb))); r2.append(l); return c; };
  const dg = tog('digits', 'Digits'), sy = tog('symbols', 'Symbols'); p.append(r2);
  const cp = el('button', 'btn btn-primary', t('copy', 'Copy'));
  cp.onclick = async () => {
    if (cp.disabled) return;
    cp.disabled = true;
    const prevHtml = cp.innerHTML;
    cp.innerHTML = '<span class="spinner"></span> <span>' + t('copy', 'Copy') + '</span>';
    try {
      await copy(code.textContent, t('passwordCopied', 'Password copied'));
    } catch (err) {
      toast(errText(err));
    } finally {
      cp.innerHTML = prevHtml;
      cp.disabled = false;
    }
  };
  p.append(cp);
  async function run() { lv.textContent = len.value; code.textContent = await generate(Number(len.value), dg.checked, sy.checked); }
  for (const c of [len, dg, sy]) c.addEventListener('input', run);
  run();
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
      if (ui === 'password') { i.type = 'text'; const g = el('button', 'eye gen'); g.type = 'button'; g.innerHTML = UI.key; g.title = t('generate', 'Generate'); g.onclick = async () => { i.value = await generate(20, true, true); }; w.replaceChildren(i, g); }
      f.append(w);
    } else f.append(i);
    inputs[id] = i; return i;
  };
  const cap = (x) => x.charAt(0).toUpperCase() + x.slice(1);
  add('title', type.titleLabel || 'Title', 'text', pre.title != null ? String(pre.title) : (type.id === 'login' && S.origin ? V.hostOfUrl(S.origin) : ''), { required: true });
  if (type.url) add('url', t('website', 'Website'), 'url', pre.url != null ? String(pre.url) : (S.origin || ''));
  for (const sec of N.sectionsOf(type)) {
    if (sec.title) f.append(el('div', 'sec-title', sec.title));
    for (const fl of sec.fields) add(fl.id, fl.hintLabel || cap(fl.label), fl.ui, '', fl);
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
  if (type.id === 'login' && inputs.password) generate(20, true, true).then((v) => { if (!inputs.password.value) inputs.password.value = v; });
}

$('genBtn').innerHTML = UI.key; $('genBtn').onclick = () => (S.panel === 'gen' ? closePanel() : openGenerator());
$('newBtn').innerHTML = UI.plus; $('newBtn').onclick = () => (S.panel === 'new' ? closePanel() : openNewItem());
$('lock').innerHTML = UI.lock; $('searchIc').innerHTML = UI.search;

// ---------- keyboard ----------
document.addEventListener('keydown', (e) => {
  if ($('unlocked').classList.contains('hidden')) { if (e.key === 'Escape') window.close(); return; }
  const a = document.activeElement;
  // Enter on a search with no results: create the item the user was looking for (before the debounce fires).
  if (e.key === 'Enter' && a && a.id === 'q') {
    const q = $('q').value.trim();
    if (q !== S.query) { S.query = q; applyFilter(true); }
    if (S.query && !S.rows.length) { e.preventDefault(); openNewItem('login', { title: S.query }); return; }
  }
  const inForm = !!(a && a.closest && a.closest('#panel, select, .menu, button'));
  const sel = window.getSelection && String(window.getSelection());
  const inputSel = a && a.tagName === 'INPUT' && a.selectionStart !== a.selectionEnd;
  const selected = S.rows[S.sel];
  const act = V.keyAction(e, { panel: S.panel, inForm, hasSelection: !!sel || inputSel, hasTotp: !!(selected && selected.has_totp) });
  if (!act) return;
  e.preventDefault();
  if (act === 'close') window.close();
  else if (act === 'closePanel') closePanel();
  else if (act === 'down' || act === 'up') select(V.moveSel(S.sel, act === 'down' ? 1 : -1, S.rows.length));
  else if (act === 'fill') openAndFill();
  else if (act === 'copyTotp') { if (selected) copyTotp(selected.id).catch(showErr); }
  else if (act === 'copyPassword') copyPrimary().catch(showErr);
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
  if (S.panel === 'new') { livePending = true; return; }
  clearTimeout(liveT);
  liveT = setTimeout(async () => {
    const [all, match] = await Promise.all([
      send({ type: 'popup:list_items' }).catch(() => null),
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

globalThis.__gitpassPopup = {
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
    S,
  };
}
