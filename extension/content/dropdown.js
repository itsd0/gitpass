// ---------- dropdown ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

let dd = null; // { field, host, root, box, hd, rows:[{btn, run}], active, hovered, seq, mode, searchInput, allLogins, confirmRun, cancelRun }
let ddSeq = 0;
function closeDropdown() {
  if (dd) {
    if (dd.mode === 'search') send('page:search_disable').catch(() => {});
    dd.host.remove();
    dd = null;
    if (totpChip) placeTotpChip();
  }
}
function placeDropdown() {
  const r = dd.field.getBoundingClientRect();
  dd.host.style.left = Math.max(4, Math.min(r.left + scrollX, scrollX + innerWidth - 328)) + 'px';
  dd.host.style.top = (r.bottom + scrollY + 4) + 'px';
}
function setActive(i) {
  if (!dd || !dd.rows.length) return;
  dd.active = (i + dd.rows.length) % dd.rows.length;
  dd.rows.forEach((r, j) => r.btn.classList.toggle('active', j === dd.active));
  if (dd.rows[dd.active].btn.scrollIntoView) dd.rows[dd.active].btn.scrollIntoView({ block: 'nearest' });
}
function onFieldKey(e) {
  if (!dd || dd.field !== e.target) {
    if (e.key === 'ArrowDown' && e.isTrusted && (e.altKey || !e.target.value)) { e.preventDefault(); openDropdown(e.target); }
    return;
  }
  if (dd.mode === 'confirm') {
    if (e.key === 'Escape') { e.preventDefault(); if (dd.cancelRun) dd.cancelRun(); }
    else if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); if (dd.confirmRun) dd.confirmRun(); }
    return;
  }
  if (dd.mode === 'postfill') {
    if (e.key === 'Escape') { e.preventDefault(); closeDropdown(); }
    return;
  }
  if (dd.mode === 'search') {
    if (e.key === 'Escape') { e.preventDefault(); exitSearchMode(ctxFor(dd.field)); return; }
  }
  if (e.key === 'ArrowDown') { e.preventDefault(); setActive(dd.active + 1); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); setActive(dd.active - 1); }
  else if (e.key === 'Enter' && dd.active >= 0) { e.preventDefault(); e.stopPropagation(); if (e.isTrusted) dd.rows[dd.active].run(); }
  else if (e.key === 'Escape') { e.preventDefault(); closeDropdown(); }
  else if (e.key === 'Tab') closeDropdown();
}
const siteIconCache = new Map();
const siteIconPending = new Set();

function createMonogramEl(title) {
  const span = el('span', 'monogram');
  span.setAttribute('aria-hidden', 'true');
  const mono = (typeof GitpassIcons !== 'undefined' && GitpassIcons.getMonogram)
    ? GitpassIcons.getMonogram(title)
    : (String(title || '').trim().charAt(0).toUpperCase() || '?');
  span.textContent = mono;
  return span;
}

function itemIcon(it) {
  if (!it || (it.category && it.category !== 'login')) {
    return catIcon(it && it.category);
  }
  const host = (it.favicon_host || (it.url && GitpassPolicy.originOf(it.url) && new URL(it.url).hostname) || '').toLowerCase();
  const title = it.title || host || '';

  if (host && siteIconCache.has(host)) {
    const cached = siteIconCache.get(host);
    if (cached) {
      const img = el('img', 'site-icon-img');
      img.src = cached;
      img.alt = '';
      img.setAttribute('aria-hidden', 'true');
      img.width = 20;
      img.height = 20;
      img.onerror = () => {
        siteIconCache.set(host, null);
        img.replaceWith(createMonogramEl(title));
      };
      return { dom: img };
    }
  }

  const span = createMonogramEl(title);
  if (host && !siteIconCache.has(host) && !siteIconPending.has(host)) {
    siteIconPending.add(host);
    send('page:site_icon', { host })
      .then((res) => {
        siteIconPending.delete(host);
        if (res && res.data_url) {
          siteIconCache.set(host, res.data_url);
          if (span.isConnected) {
            const img = el('img', 'site-icon-img');
            img.src = res.data_url;
            img.alt = '';
            img.setAttribute('aria-hidden', 'true');
            img.width = 20;
            img.height = 20;
            img.onerror = () => {
              siteIconCache.set(host, null);
              img.replaceWith(createMonogramEl(title));
            };
            span.replaceWith(img);
          }
        } else {
          siteIconCache.set(host, null);
        }
      })
      .catch(() => {
        siteIconPending.delete(host);
        siteIconCache.set(host, null);
      });
  }
  return { dom: span };
}

function addRow(icon, title, sub, run, opts) {
  const b = el('button', 'it' + (opts && opts.primary ? ' primary' : '')); b.type = 'button'; b.tabIndex = -1;
  const ic = el('span', 'ic');
  if (icon && icon.dom) ic.appendChild(icon.dom);
  else if (icon && icon.svg) ic.innerHTML = icon.svg;
  else if (icon && icon.text) ic.textContent = icon.text;
  const m = el('span', 'm'); m.append(el('div', 'ti', title)); if (sub) m.append(el('div', 'u' + (opts && opts.mono ? ' mono' : ''), sub));
  b.append(ic, m);
  b.addEventListener('mousedown', (e) => e.preventDefault());
  const row = { btn: b, run: () => Promise.resolve().then(run).catch(showErr) };
  b.addEventListener('click', (e) => { if (!e.isTrusted) return; e.preventDefault(); e.stopPropagation(); row.run(); });
  b.addEventListener('mouseenter', () => setActive(dd.rows.indexOf(row)));
  dd.box.append(b); dd.rows.push(row);
  return row;
}
function showErr(e) { if (!dd) return; const d = el('div', 'msg err', 'gitpass: ' + (e && e.message || e)); dd.box.append(d); }
function catIcon(cat) { try { return { svg: GitpassIcons.categoryIcon(cat || 'login', 28) }; } catch (_) { return { text: '🔑' }; } }
function clearRows() { if (!dd) return; dd.rows = []; dd.active = -1; while (dd.box.childNodes.length > 1) dd.box.lastChild.remove(); }

function isCrossSite(it) {
  if (dd && dd.matchedIds && dd.matchedIds.has(it && it.id)) return false;
  if (!it || !it.url) return true;
  return GitpassPolicy.isCrossSite(it.url, location.origin);
}

async function openDropdown(field) {
  const ctx = ctxFor(field);
  if (!ctx) return;
  if (dd && dd.field === field) return;
  closeDropdown();
  const { host, root } = shadowHost();
  host.style.zIndex = '2147483647';
  const box = el('div', 'menu'); box.setAttribute('role', 'listbox');
  const hd = el('div', 'hd'); const lg = el('img'); lg.src = LOGO; lg.alt = ''; hd.append(lg, document.createTextNode('gitpass'));
  box.append(hd); root.append(box);
  host.addEventListener('mouseenter', () => { if (dd) dd.hovered = true; });
  host.addEventListener('mouseleave', () => { if (dd) dd.hovered = false; });
  dd = { field, host, root, box, hd, rows: [], active: -1, hovered: false, seq: ++ddSeq, mode: 'suggested', searchInput: null, allLogins: null, matchedIds: new Set() };
  placeDropdown();
  await populate(ctx);
}

/** "16 characters, no symbols" for the suggest row when the field states password rules. */
function rulesSubtitle(PR, fit) {
  const s = PR && fit ? PR.summary(fit) : null;
  if (!s) return '';
  const parts = [t('csRulesLen', '$1 characters', [String(s.len)])];
  if (s.noSymbols) parts.push(t('csRulesNoSymbols', 'no symbols'));
  else if (s.limitedSymbols) parts.push(t('csRulesFewSymbols', 'limited symbols'));
  return parts.join(', ');
}

async function populate(ctx) {
  const mine = dd && dd.seq;
  const live = () => dd && dd.seq === mine;
  clearRows();
  const loading = el('div', 'msg', '…'); dd.box.append(loading);
  const { form, role } = ctx;
  const isNew = NEWPW_ROLES.has(role) || (role === 'username' && (form.kind === 'signup'));
  const isLogin = !GitpassForms.CARD_ROLES.has(role) && form.kind !== 'identity';
  try {
    let items = [];
    if (GitpassForms.CARD_ROLES.has(role)) items = (await send('page:list', { category: 'credit_card' })).items;
    else if (form.kind === 'identity') items = (await send('page:list', { category: 'identity' })).items;
    else items = (await send('page:match')).items.filter((i) => !i.category || i.category === 'login');
    if (!live()) return;
    loading.remove();
    dd.matchedIds = new Set((items || []).map((i) => i.id));
    if (form.fields.newPassword && (isNew || form.kind === 'change_password')) {
      // Site rules (#188): maxlength / minlength / passwordrules on the field, sent as parsed constraints only.
      const PR = globalThis.GitpassPasswordRules;
      const rules = PR ? PR.fromField(form.fields.newPassword) : null;
      const fit = rules ? PR.sanitize(rules) : null;
      if (fit && !fit.ok) {
        dd.box.append(el('div', 'msg', t('csRulesTooShort', "This site's password rules are too short for a strong password")));
      } else {
        addRow({ text: '✨' }, t('csSuggest', 'Suggest strong password'), rulesSubtitle(PR, fit) || t('csSuggestSub', 'gitpass will save it when you submit'), async () => {
          const r = await send('page:generate', rules ? { rules } : {});
          if (!originOk(r)) return;
          GitpassForms.fillNewPassword(form, r.password); // generated value is not kept after this call
          closeDropdown();
        }, { primary: true });
      }
    }
    if (role === 'totp') items = items.filter((i) => i.has_totp);
    for (const it of items) addRow(itemIcon(it), it.title || '(untitled)', it.subtitle || '', () => selectItem(ctx, it));
    if (!items.length) {
      dd.box.append(el('div', 'msg', GitpassForms.CARD_ROLES.has(role) ? t('csNoCards', 'No cards in gitpass') : form.kind === 'identity' ? t('csNoIdentities', 'No identities in gitpass') : t('csNoLogins', 'No gitpass logins for this site')));
    }
    if (isLogin) {
      addRow({ text: '🔍' }, t('csSearchGitpass', 'Search gitpass…'), '', (e) => {
        if (e && !e.isTrusted) return;
        openSearchMode(ctx);
      });
    }
    setActive(0);
  } catch (e) {
    if (!live()) return;
    loading.remove();
    const code = e.code || codeOf(e.message);
    if (code === 'locked') {
      addRow({ text: '🔒' }, t('csUnlock', 'Unlock gitpass'), t('csUnlockSub', 'Use Touch ID or your password in the gitpass app'), async () => {
        const r = await send('page:request_unlock');
        if (r && r.unlocked && live()) await populate(ctx);
      }, { primary: true });
      setActive(0);
    } else if (code === 'app_not_running') {
      addRow({ text: '↗' }, t('csOpenApp', 'Open gitpass app to unlock'), '', async () => { await send('page:open_app'); closeDropdown(); }, { primary: true });
      setActive(0);
    } else dd.box.append(el('div', 'msg err', 'gitpass: ' + e.message));
  }
}

async function openSearchMode(ctx) {
  if (!dd) return;
  try {
    await send('page:search_enable', { user_click: true });
  } catch (_) {}
  dd.mode = 'search';
  clearRows();
  dd.hd.textContent = '';
  const searchWrap = el('div', 'search-wrap');
  const lg = el('img'); lg.src = LOGO; lg.alt = '';
  const input = el('input', 'search-input');
  input.type = 'text';
  input.placeholder = t('csSearchGitpass', 'Search gitpass…');
  input.autocomplete = 'off';
  input.spellcheck = false;
  searchWrap.append(lg, input);
  dd.hd.append(searchWrap);
  dd.searchInput = input;
  input.focus();

  const loading = el('div', 'msg', '…');
  dd.box.append(loading);

  let searchSeq = 0;
  let debounceTimer = null;
  let lastItems = [];

  async function doSearch(query) {
    const seq = ++searchSeq;
    try {
      const res = await send('page:search', { query: (query || '').trim() });
      if (seq !== searchSeq || !dd || dd.mode !== 'search') return;
      loading.remove();
      lastItems = res.items || [];
      renderSearchResults(ctx, lastItems, query);
    } catch (e) {
      if (seq !== searchSeq || !dd || dd.mode !== 'search') return;
      loading.remove();
      const code = e.code || codeOf(e.message);
      clearRows();
      if (code === 'locked') {
        addRow({ text: '🔒' }, t('csUnlock', 'Unlock gitpass'), t('csUnlockSub', 'Use Touch ID or your password in the gitpass app'), async () => {
          const r = await send('page:request_unlock');
          if (r && r.unlocked && dd && dd.mode === 'search') {
            openSearchMode(ctx);
          }
        }, { primary: true });
        setActive(0);
      } else if (code === 'app_not_running') {
        addRow({ text: '↗' }, t('csOpenApp', 'Open gitpass app to unlock'), '', async () => { await send('page:open_app'); closeDropdown(); }, { primary: true });
        setActive(0);
      } else {
        dd.box.append(el('div', 'msg err', 'gitpass: ' + e.message));
      }
    }
  }

  input.addEventListener('input', () => {
    const q = input.value;
    renderSearchResults(ctx, lastItems, q);
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      doSearch(q);
    }, 50);
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setActive(dd.active + 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActive(dd.active - 1); }
    else if (e.key === 'Enter') {
      e.preventDefault();
      if (dd.active >= 0 && dd.rows[dd.active]) dd.rows[dd.active].run();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      exitSearchMode(ctx);
    }
  });

  doSearch('');
  setTimeout(() => input.focus(), 10);
}

function exitSearchMode(ctx) {
  if (!dd) return;
  send('page:search_disable').catch(() => {});
  dd.mode = 'suggested';
  dd.searchInput = null;
  dd.hd.textContent = '';
  const lg = el('img'); lg.src = LOGO; lg.alt = '';
  dd.hd.append(lg, document.createTextNode('gitpass'));
  populate(ctx);
  if (dd.field) dd.field.focus();
}

function renderSearchResults(ctx, items, query) {
  if (!dd || dd.mode !== 'search') return;
  clearRows();
  const q = (query !== undefined ? query : (dd.searchInput ? dd.searchInput.value : '')).trim().toLowerCase();
  const isTotp = ctx && ctx.role === 'totp';
  const list = (items || []).filter((i) => {
    if (isTotp && !i.has_totp) return false;
    if (!q) return true;
    return [i.title, i.subtitle, i.url].some((x) => x && String(x).toLowerCase().includes(q));
  });
  list.sort((a, b) => (Number(b.favorite || 0) - Number(a.favorite || 0)) || String(a.title || '').localeCompare(String(b.title || '')));
  for (const it of list) {
    addRow(itemIcon(it), it.title || '(untitled)', it.subtitle || it.url || '', () => selectItem(ctx, it));
  }
  if (!list.length) {
    dd.box.append(el('div', 'msg', t('csNoMatchingLogins', 'No matching logins')));
  }
  setActive(0);
}

async function selectItem(ctx, it) {
  if (ctx && ctx.role === 'totp' && !it.has_totp) return;
  if (isCrossSite(it)) {
    try {
      const res = await send('page:request_cross_site_confirm', { item_id: it.id });
      if (res && res.approved) {
        await fillItem(ctx, it, { keepOpenForAddSite: true });
        showAddSitePrompt(ctx, it);
      }
    } catch (e) {
      if (e && e.code !== 'cancelled' && !/cancelled/i.test(e.message)) {
        showErr(e);
      }
    }
  } else {
    fillItem(ctx, it);
  }
}

function showAddSitePrompt(ctx, it) {
  if (!dd) return;
  dd.mode = 'postfill';
  clearRows();
  const box = el('div', 'confirm-box');
  const addBtn = el('button', 'btn pri', t('csAlsoAddSite', 'Also add this site to the item'));
  addBtn.type = 'button';
  addBtn.style.width = '100%';
  addBtn.addEventListener('click', async (e) => {
    if (!e.isTrusted) return;
    addBtn.disabled = true;
    try {
      await send('page:add_url', { item_id: it.id, user_click: true });
      addBtn.textContent = t('csSiteAdded', 'Site added');
      setTimeout(closeDropdown, 1200);
    } catch (err) {
      addBtn.textContent = 'gitpass: ' + (err && err.message || err);
      setTimeout(closeDropdown, 2000);
    }
  });
  box.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeDropdown(); }
  });
  box.append(addBtn);
  dd.box.append(box);
  setTimeout(() => addBtn.focus(), 10);
}

async function fillItem(ctx, it, opts = {}) {
  const { form, role } = ctx;
  if (it.category === 'credit_card') {
    const r = await send('page:fill_card', { item_id: it.id, user_click: true });
    if (originOk(r)) GitpassForms.fillCard(freshForm(form), r.fields);
  } else if (it.category === 'identity') {
    const r = await send('page:fill_identity', { item_id: it.id, user_click: true });
    if (originOk(r)) GitpassForms.fillIdentity(freshForm(form), r.fields);
  } else if (role === 'totp' || form.kind === 'totp') {
    const r = await send('page:totp', { item_id: it.id });
    if (originOk(r)) GitpassForms.fillTotp(freshForm(form), r.code);
  } else {
    let r = await send('page:fill', { item_id: it.id });
    if (originOk(r)) {
      const f = freshForm(form);
      if (f.kind === 'change_password' && !f.fields.password) GitpassForms.fillLogin({ fields: { ...f.fields, password: f.fields.current, totp: null } }, { username: r.username, password: r.password });
      else { GitpassForms.fillLogin(f, r); maybeAutoSubmit(f, r.origin); }
    }
    r = null; // drop secrets
  }
  if (!opts.keepOpenForAddSite) {
    closeDropdown();
  }
}
// Re-detect right before writing, in case the page swapped fields since the dropdown opened.
function freshForm(form) {
  forms = GitpassForms.detectForms(document);
  return forms.find((f) => f.scope === form.scope && f.kind === form.kind) || form;
}
