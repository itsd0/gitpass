/* global chrome, GitpassI18n, GitpassPopupIcons, GitpassView */
'use strict';
// gitpass popup v0.6: 1Password-style 2-pane UI. All host calls go through background.js as `popup:*` messages.
const $ = (id) => document.getElementById(id);
const t = (k, f) => GitpassI18n.t(k, f);
const V = GitpassView;
const { UI, categoryIcon } = GitpassPopupIcons;
const ROW_H = 52;

function send(msg) {
  if (window.__gitpassMock) return window.__gitpassMock(msg);
  return new Promise((res, rej) => chrome.runtime.sendMessage(msg, (r) => {
    if (chrome.runtime.lastError) return rej(new Error(chrome.runtime.lastError.message));
    if (r && r.ok) return res(r.result);
    const e = new Error((r && r.error) || 'error'); e.code = r && r.code; rej(e);
  }));
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
  };
  return (c && M[c]) || String((e && e.message) || e);
}
function showErr(e) { if (e && e.code === 'locked') { refresh(); return; } toast(errText(e)); }
async function copy(text, label) {
  if (window.__gitpassMock) { toast(label || t('copied', 'Copied')); return; }
  await navigator.clipboard.writeText(text); toast(label || t('copied', 'Copied'));
}
function show(view) { for (const v of ['nohost', 'locked', 'unlocked']) { const e = $(v); if (e) e.classList.toggle('hidden', v !== view); } }

// ---------- state ----------
const S = { all: [], suggested: [], origin: null, vaults: [], filter: 'suggested', query: '', rows: [], sel: -1, detail: null, revealed: {}, totpTimer: null, panel: null, view: null };

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
  $('q').focus();
}

function showNoHost(e) {
  $('hostErr').textContent = String((e && e.message) || '');
  $('hostCmd').textContent = 'cd ~/gitpass && cargo build --release -p gitpass-host\n"$HOME/Library/Application Support/gitpass/bin/gitpass-host" install --browser chrome --extension-id ' + ((globalThis.chrome && chrome.runtime && chrome.runtime.id) || '<id>');
  show('nohost');
}

function showLocked(view, st) {
  stopTotp();
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
$('lock').onclick = async () => { await send({ type: 'popup:lock' }).catch(showErr); refresh(); };

// ---------- data + list ----------
async function loadData() {
  const [vaults, all, match] = await Promise.all([
    send({ type: 'popup:list_vaults' }).catch(() => ({ vaults: [] })),
    send({ type: 'popup:list_items' }).catch((e) => { showErr(e); return { items: [] }; }),
    send({ type: 'popup:match_active' }).catch(() => ({ items: [], origin: null })),
  ]);
  S.vaults = vaults.vaults || []; S.all = all.items || []; S.suggested = match.items || []; S.origin = match.origin || null;
  const want = window.__gitpassMockParams && window.__gitpassMockParams.get('filter');
  if (want) S.filter = want; else if (S.view !== 'unlocked-loaded' && !S.suggested.length && S.filter === 'suggested') S.filter = 'all';
  S.view = 'unlocked-loaded';
  renderFilter(); applyFilter(true);
  const pre = window.__gitpassMockParams && window.__gitpassMockParams.get('item');
  if (pre) { const i = S.rows.findIndex((r) => r.id === pre); if (i >= 0) select(i); }
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
}
$('filter').onchange = () => { S.filter = $('filter').value; applyFilter(true); };

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
  if (!S.rows.length) {
    const msg = S.query ? t('noResults', 'No items found') : S.filter === 'suggested' ? (S.origin ? t('noMatches', 'No saved logins for this site') : t('notFillable', 'Open an https page to fill')) : t('noItems', 'No items');
    inner.append(el('div', 'empty', msg)); return;
  }
  const w = V.virtualWindow(list.scrollTop, list.clientHeight || 380, ROW_H, S.rows.length);
  inner.style.paddingTop = w.padTop + 'px'; inner.style.paddingBottom = w.padBottom + 'px';
  for (let i = w.start; i < w.end; i++) {
    const it = S.rows[i];
    const row = el('div', 'row' + (i === S.sel ? ' sel' : '')); row.setAttribute('role', 'option'); row.dataset.idx = i;
    const ic = el('span', 'ic'); ic.innerHTML = categoryIcon(it.category, 32);
    const meta = el('div', 'meta'); meta.append(el('div', 't', it.title), el('div', 'u', it.subtitle || '—'));
    row.append(ic, meta);
    if (it.favorite) { const f = el('span', 'fav'); f.innerHTML = UI.star; row.append(f); }
    row.onclick = () => select(i);
    row.ondblclick = () => openAndFill();
    inner.append(row);
  }
}
$('list').addEventListener('scroll', () => renderList(), { passive: true });

let qt = null;
$('q').oninput = () => { clearTimeout(qt); qt = setTimeout(() => { S.query = $('q').value.trim(); applyFilter(true); }, 80); };

// ---------- detail ----------
let detailSeq = 0;
async function select(idx) {
  S.sel = idx; S.revealed = {}; stopTotp();
  const list = $('list');
  if (idx >= 0) list.scrollTop = V.scrollToRow(idx, list.scrollTop, list.clientHeight || 380, ROW_H);
  renderList();
  const pane = $('detail'); pane.textContent = '';
  const it = S.rows[idx];
  if (!it) { pane.append(el('div', 'empty', t('selectItem', 'Select an item'))); S.detail = null; return; }
  const seq = ++detailSeq;
  let d;
  try { d = (await send({ type: 'popup:get_item', item_id: it.id })).item; } catch (e) { d = { ...it, fields: [], tags: [] }; showErr(e); }
  if (seq !== detailSeq) return;
  S.detail = d; renderDetail(d);
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
    const b = el('button', 'btn btn-primary', d.category === 'login' && d.url ? t('openFill', 'Open & Fill') : t('fill', 'Fill')); b.onclick = () => openAndFill();
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
      if (S.revealed[f.name] != null) { delete S.revealed[f.name]; val.textContent = V.mask(f.kind, f.masked); b.innerHTML = UI.eye; return; }
      const v = await secretValue(d, f); S.revealed[f.name] = v; val.textContent = V.formatValue(f.kind, v); b.innerHTML = UI.eyeOff;
    }));
  } else if (f.kind === 'url' && f.value) {
    const a = el('a', null, f.value); a.href = f.value; a.target = '_blank'; a.rel = 'noopener'; val.append(a); val.classList.add('plain');
  } else { val.textContent = V.formatValue(f.kind, f.value); if (!/password|card_number/.test(f.kind)) val.classList.add('plain'); }
  acts.append(iconBtn(UI.copy, t('copy', 'Copy'), async () => copy(await secretValue(d, f), t('copied', 'Copied'))));
  return row;
}

// TOTP ring countdown. Fix for v0.5.1 popup.js:45 null classList: the old handler kept a setInterval that
// touched an element after the list re-rendered. Now there is one timer, guarded by isConnected and cleared on every re-render.
const RING_C = 2 * Math.PI * 7;
function renderTotp(d, val) {
  stopTotp();
  const wrap = el('div', 'totp'); const code = el('span', null, '••• •••');
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
      catch (e) { code.textContent = '—'; return stopTotp(); } finally { fetching = false; }
      if (!wrap.isConnected) return stopTotp();
      code.textContent = V.formatValue('totp', st.code);
    } else st.remaining -= 1;
    fg.setAttribute('stroke-dashoffset', String(V.ringOffset(st.remaining, st.period, RING_C)));
    svg.classList.toggle('low', st.remaining <= 5);
  };
  tick(); S.totpTimer = setInterval(tick, 1000);
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
function closePanel() { const p = $('panel'); p.classList.add('hidden'); p.textContent = ''; S.panel = null; $('q').focus(); }
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
  const cp = el('button', 'btn btn-primary', t('copy', 'Copy')); cp.onclick = () => copy(code.textContent, t('passwordCopied', 'Password copied')).catch(showErr); p.append(cp);
  async function run() { lv.textContent = len.value; code.textContent = await generate(Number(len.value), dg.checked, sy.checked); }
  for (const c of [len, dg, sy]) c.addEventListener('input', run);
  run();
}

function openNewLogin() {
  const p = $('panel'); p.textContent = ''; p.classList.remove('hidden'); S.panel = 'new';
  p.append(panelHead(t('newLogin', 'New Login')));
  const f = el('form', 'stack'); f.style.maxWidth = 'none'; f.autocomplete = 'off';
  const inp = (key, fb, type, val) => { f.append(el('label', null, t(key, fb))); const i = Object.assign(el('input'), { type, value: val || '' }); i.spellcheck = false; f.append(i); return i; };
  const title = inp('title', 'Title', 'text', S.origin ? V.hostOfUrl(S.origin) : '');
  const url = inp('website', 'Website', 'url', S.origin || '');
  const user = inp('username', 'Username', 'text');
  f.append(el('label', null, t('password', 'Password')));
  const pwWrap = el('div', 'field'); const pw = Object.assign(el('input'), { type: 'text' }); pw.spellcheck = false;
  const g = el('button', 'eye'); g.type = 'button'; g.innerHTML = UI.key; g.title = t('generate', 'Generate'); g.onclick = async () => { pw.value = await generate(20, true, true); };
  pwWrap.append(pw, g); f.append(pwWrap);
  const vs = el('select'); for (const v of S.vaults) { const o = el('option', null, v.name); o.value = v.id; vs.append(o); }
  if (S.vaults.length) { f.append(el('label', null, t('vault', 'Vault'))); f.append(vs); }
  const errEl = el('p', 'err'); f.append(errEl);
  const save = el('button', 'btn btn-primary', t('save', 'Save')); save.type = 'submit'; f.append(save);
  f.onsubmit = async (e) => {
    e.preventDefault(); errEl.textContent = '';
    if (!url.value || !pw.value) { errEl.textContent = t('needUrlPw', 'Website and password are required'); return; }
    save.disabled = true;
    try {
      await send({ type: 'popup:save_login', origin: url.value, title: title.value || undefined, username: user.value, password: pw.value, vault_id: vs.value || undefined });
      toast(t('saved', 'Saved')); closePanel(); await loadData();
    } catch (err) { errEl.textContent = String(err.message || err); } finally { save.disabled = false; }
  };
  p.append(f); user.focus();
  pw.value = ''; generate(20, true, true).then((v) => { if (!pw.value) pw.value = v; });
}

$('genBtn').innerHTML = UI.key; $('genBtn').onclick = () => (S.panel === 'gen' ? closePanel() : openGenerator());
$('newBtn').innerHTML = UI.plus; $('newBtn').onclick = () => (S.panel === 'new' ? closePanel() : openNewLogin());
$('lock').innerHTML = UI.lock; $('searchIc').innerHTML = UI.search;

// ---------- keyboard ----------
document.addEventListener('keydown', (e) => {
  if ($('unlocked').classList.contains('hidden')) { if (e.key === 'Escape') window.close(); return; }
  const a = document.activeElement;
  const inForm = !!(a && a.closest && a.closest('#panel, select, .menu'));
  const sel = window.getSelection && String(window.getSelection());
  const inputSel = a && a.tagName === 'INPUT' && a.selectionStart !== a.selectionEnd;
  const act = V.keyAction(e, { panel: S.panel, inForm, hasSelection: !!sel || inputSel });
  if (!act) return;
  e.preventDefault();
  if (act === 'close') window.close();
  else if (act === 'closePanel') closePanel();
  else if (act === 'down' || act === 'up') select(V.moveSel(S.sel, act === 'down' ? 1 : -1, S.rows.length));
  else if (act === 'fill') openAndFill();
  else if (act === 'copyPassword') copyPrimary().catch(showErr);
});

// ---------- user settings (options page) ----------
function popupSettings() {
  return new Promise((res) => {
    try { chrome.storage.local.get('settings', (r) => res(Object.assign({ autofill: true, savePrompts: true, autolockFollowsApp: true }, (r && r.settings) || {}))); }
    catch (_) { res({ autofill: true, savePrompts: true, autolockFollowsApp: true }); }
  });
}

// ---------- live lock/unlock events ----------
if (globalThis.chrome && chrome.runtime && chrome.runtime.onMessage) {
  chrome.runtime.onMessage.addListener((msg) => {
    if (!msg || msg.type !== 'popup:event') return;
    if (msg.event === 'unlock') refresh();
    else if (msg.event === 'lock') popupSettings().then((st) => { if (st.autolockFollowsApp !== false) refresh(); });
  });
}

GitpassI18n.apply(document);
refresh();
