// gitpass content script (v0.6, workstream 4): field badges, inline dropdown, keyboard nav, fill,
// password suggestions, TOTP, card/identity fill, and the save/update banner. All UI in closed shadow DOM.
// Security: top frame only; https/loopback only; every fill needs a trusted (isTrusted) click or key press;
// the background derives the origin from the sender and we re-check it against location.origin before
// writing; secret values are only held in local variables for the duration of the fill.
/* global chrome, GitpassPolicy, GitpassForms, GitpassI18n, GitpassIcons */
(function () {
  'use strict';
  if (window.top !== window) return;                         // top frame only (no iframes, even same-origin)
  if (!GitpassPolicy.isFillableOrigin(location.origin)) return; // https or http://localhost only
  if (window.__gitpassInpage) return; window.__gitpassInpage = true;
  const LOGO = 'data:image/png;base64,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';
  const t = (k, f) => GitpassI18n.t(k, f);
  const LOGIN_ROLES = new Set(['username', 'password', 'current_password', 'totp']);
  const NEWPW_ROLES = new Set(['new_password', 'confirm_password']);

  // Dark is the fallback palette; prefers-color-scheme:light swaps the tokens. The orange accent
  // (#F28C28) stays constant for the primary button, active row indicator and badge ring.
  const CSS = `:host{all:initial;position:absolute;z-index:2147483646;color-scheme:light dark;
  --gp-accent:#F28C28;--gp-accent-fg:#1C1C1E;--gp-accent-text:#F28C28;--gp-bg:#1C1C1E;--gp-fg:#ECEDF0;--gp-muted:#969AA4;--gp-border:#3c3e44;--gp-hover:#34363c;--gp-active:#45321f;--gp-field:#25262a;--gp-btn-bg:#2a2b30;--gp-overlay:rgba(0,0,0,.6);--gp-danger:#ff8a80;--gp-ok:#7ee2a8;--gp-shadow:0 12px 32px rgba(0,0,0,.45);--gp-shadow-lg:0 16px 48px rgba(0,0,0,.6);--gp-badge-shadow:0 1px 3px rgba(0,0,0,.35),0 0 0 1px rgba(242,140,40,.45);--gp-badge-hover-shadow:0 2px 6px rgba(0,0,0,.4),0 0 0 2px var(--gp-accent)}
  @media (prefers-color-scheme:light){:host{--gp-accent-text:#B25B00;--gp-bg:#FFFFFF;--gp-fg:#1C1C1E;--gp-muted:#5A5F67;--gp-border:#D5D8DE;--gp-hover:#F1F2F4;--gp-active:#FDEBD9;--gp-field:#F4F5F7;--gp-btn-bg:#EDEFF2;--gp-overlay:rgba(15,18,24,.35);--gp-danger:#B3261E;--gp-ok:#1B7F4B;--gp-shadow:0 12px 32px rgba(15,18,24,.18);--gp-shadow-lg:0 16px 48px rgba(15,18,24,.24);--gp-badge-shadow:0 1px 3px rgba(15,18,24,.22),0 0 0 1px rgba(242,140,40,.55);--gp-badge-hover-shadow:0 2px 6px rgba(15,18,24,.28),0 0 0 2px var(--gp-accent)}}
  *{box-sizing:border-box}
  .badge{all:unset;box-sizing:border-box;width:22px;height:22px;border-radius:6px;cursor:pointer;display:flex;align-items:center;justify-content:center;background:var(--gp-bg);box-shadow:var(--gp-badge-shadow);transition:transform .12s,box-shadow .12s}
  .badge:hover{transform:scale(1.08);box-shadow:var(--gp-badge-hover-shadow)}
  .badge img{width:16px;height:16px;border-radius:4px;display:block}
  .menu{width:320px;max-height:340px;overflow:auto;background:var(--gp-bg);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:10px;box-shadow:var(--gp-shadow);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI","Noto Sans Myanmar","Noto Sans Thai",sans-serif}
  .hd{display:flex;align-items:center;gap:8px;padding:8px 10px;border-bottom:1px solid var(--gp-border);font-size:11px;font-weight:600;letter-spacing:.5px;text-transform:uppercase;color:var(--gp-muted)}
  .hd img{width:16px;height:16px;border-radius:4px}
  .it{display:flex;align-items:center;gap:10px;width:100%;padding:8px 10px;cursor:pointer;border:0;background:none;color:inherit;font:inherit;text-align:left}
  .it:hover{background:var(--gp-hover)}
  .it.active,.it.active:hover{background:var(--gp-active);box-shadow:inset 3px 0 0 var(--gp-accent)}
  .it.primary .ti{color:var(--gp-accent-text)}
  .ic{width:28px;height:28px;flex-shrink:0;display:flex;align-items:center;justify-content:center;font-size:16px}
  .m{min-width:0;flex:1}.ti{font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.u{font-size:12px;color:var(--gp-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .mono{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
  .msg{padding:12px 10px;color:var(--gp-muted)}.err{color:var(--gp-danger)}
  .banner{position:fixed;top:16px;right:16px;width:340px;background:var(--gp-bg);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:12px;box-shadow:var(--gp-shadow-lg);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;padding:14px}
  .banner .row{display:flex;align-items:center;gap:10px}
  .banner .acts{display:flex;justify-content:flex-end;gap:8px;margin-top:12px}
  .btn{border:1px solid var(--gp-border);background:var(--gp-btn-bg);color:var(--gp-fg);border-radius:8px;padding:6px 12px;font:inherit;cursor:pointer}
  .btn.pri{background:var(--gp-accent);border-color:var(--gp-accent);color:var(--gp-accent-fg);font-weight:600}
  .btn.ter{border:0;background:transparent;color:var(--gp-muted);font-size:12px;padding:6px 8px;cursor:pointer}
  .btn.ter:hover{color:var(--gp-fg);text-decoration:underline}
  .pk-overlay{position:fixed;inset:0;background:var(--gp-overlay);display:flex;align-items:center;justify-content:center;z-index:2147483647;backdrop-filter:blur(2px)}
  .pk-card{width:360px;background:var(--gp-bg);color:var(--gp-fg);border:1px solid var(--gp-border);border-radius:12px;box-shadow:var(--gp-shadow-lg);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;padding:20px}
  .pk-row{display:flex;align-items:center;gap:12px;margin-bottom:14px}
  .pk-icon{width:32px;height:32px;border-radius:8px;display:flex;align-items:center;justify-content:center;background:var(--gp-btn-bg);flex-shrink:0}
  .pk-title{font-size:15px;font-weight:600;color:var(--gp-fg)}
  .pk-meta{background:var(--gp-field);border-radius:8px;padding:12px;margin:12px 0;display:flex;flex-direction:column;gap:8px}
  .pk-field{display:flex;justify-content:space-between;align-items:center;font-size:12px}
  .pk-label{color:var(--gp-muted)}
  .pk-val{font-weight:600;color:var(--gp-fg);max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .pk-acts{display:flex;justify-content:flex-end;gap:10px;margin-top:16px;flex-wrap:wrap}
  .pk-status{font-size:12px;margin-top:4px;color:var(--gp-muted);white-space:pre-wrap;word-break:break-word}
  .pk-status.err{color:var(--gp-danger)}
  .pk-status.ok{color:var(--gp-ok)}`;

  // ---------- messaging ----------
  async function send(type, args) {
    const r = await chrome.runtime.sendMessage({ type, ...(args || {}) });
    if (!r || !r.ok) { const e = new Error((r && r.error) || 'no response'); e.code = (r && r.code) || codeOf(e.message); throw e; }
    return r.result;
  }
  function codeOf(m) { const x = /\b(locked|denied|not_found|origin_mismatch|bad_request|app_not_running|internal)\b/.exec(m || ''); return x ? x[1] : (/disconnect|native host/i.test(m) ? 'app_not_running' : null); }
  function originOk(res) { return res && res.origin === location.origin; } // page navigated meanwhile -> drop

  function shadowHost(fixed) {
    const host = document.createElement('gitpass-ui');
    const root = host.attachShadow({ mode: 'closed' });
    const st = document.createElement('style'); st.textContent = CSS; root.append(st);
    if (fixed) host.style.position = 'fixed';
    document.documentElement.appendChild(host);
    return { host, root };
  }
  const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };

  // ---------- user settings (options page: chrome.storage.local.settings) ----------
  const SETTINGS = { autofill: true, savePrompts: true, autolockFollowsApp: true };
  try {
    chrome.storage.local.get('settings', (r) => { Object.assign(SETTINGS, (r && r.settings) || {}); rescan(); });
    chrome.storage.onChanged.addListener((ch, area) => { if (area === 'local' && ch.settings) { Object.assign(SETTINGS, ch.settings.newValue || {}); if (!SETTINGS.savePrompts) closeBanner(); rescan(); } });
  } catch (_) { /* storage unavailable: keep defaults */ }

  // ---------- detection state ----------
  let forms = [];
  const badges = new Map(); // field -> { host, btn, role }
  const bound = new WeakSet();
  const decoratedInputs = new WeakSet();

  function pruneDetachedBadges() {
    for (const [field, b] of badges) {
      if (!field.isConnected) {
        if (b.host) b.host.remove();
        badges.delete(field);
        try { delete field.dataset.gitpassDecorated; } catch (_) {}
      }
    }
  }

  function removeBadge(field) {
    const b = badges.get(field);
    if (b) {
      if (b.host) b.host.remove();
      badges.delete(field);
    }
    try { delete field.dataset.gitpassDecorated; } catch (_) {}
  }

  function hasOverlappingBadge(field, role) {
    const r1 = field.getBoundingClientRect ? field.getBoundingClientRect() : { left: 0, top: 0, width: 0, height: 0 };
    if (!r1.width || !r1.height) return false;
    for (const [existingField, b] of badges) {
      if (existingField === field || !existingField.isConnected) continue;
      const sameContainer = (field.form && existingField.form && field.form === existingField.form) ||
                            (field.parentElement && existingField.parentElement && field.parentElement === existingField.parentElement);
      if (!sameContainer) continue;
      if (b.role !== role) continue;
      const r2 = existingField.getBoundingClientRect ? existingField.getBoundingClientRect() : { left: 0, top: 0, width: 0, height: 0 };
      if (!r2.width || !r2.height) continue;
      const dx = Math.abs(r1.left - r2.left);
      const dy = Math.abs(r1.top - r2.top);
      if (dx < 4 && dy < 4) return true;
    }
    return false;
  }

  function rescan() {
    pruneDetachedBadges();
    if (!SETTINGS.autofill) { // in-page suggestions turned off: drop badges/dropdown, keep capture (governed by savePrompts)
      closeDropdown();
      for (const field of Array.from(badges.keys())) removeBadge(field);
      return;
    }
    forms = GitpassForms.detectForms(document);
    const live = new Set();
    for (const f of forms) {
      for (const { el: field, role } of GitpassForms.anchorFields(f)) {
        if (!field || !field.isConnected) continue;
        if (hasOverlappingBadge(field, role) && !badges.has(field)) continue;
        live.add(field);
        if (!badges.has(field)) addBadge(field, role);
        if (!bound.has(field)) { bound.add(field); bindField(field); }
      }
    }
    for (const [field] of badges) {
      if (!live.has(field) || !field.isConnected) removeBadge(field);
    }
    placeAll();
  }
  function ctxFor(field) { return GitpassForms.locate(forms, field); }

  function addBadge(field, role) {
    if (badges.has(field)) return;
    decoratedInputs.add(field);
    try { field.dataset.gitpassDecorated = 'true'; } catch (_) {}
    const { host, root } = shadowHost();
    const btn = el('button', 'badge'); btn.type = 'button'; btn.tabIndex = -1;
    btn.title = t('csFillWith', 'Fill with gitpass');
    const img = el('img'); img.src = LOGO; img.alt = 'gitpass'; btn.append(img); root.append(btn);
    btn.addEventListener('mousedown', (e) => e.preventDefault()); // keep focus in the field
    btn.addEventListener('click', (e) => { if (!e.isTrusted) return; e.preventDefault(); e.stopPropagation(); field.focus(); openDropdown(field); });
    badges.set(field, { host, btn, role });
  }
  function placeAll() {
    pruneDetachedBadges();
    for (const [field, b] of badges) {
      const r = field.getBoundingClientRect();
      if (!r.width || !r.height) { b.host.style.display = 'none'; continue; }
      b.host.style.display = '';
      b.host.style.left = (r.right + window.scrollX - 28) + 'px';
      b.host.style.top = (r.top + window.scrollY + (r.height - 22) / 2) + 'px';
    }
    if (dd) placeDropdown();
  }

  function bindField(field) {
    field.addEventListener('focus', (e) => { if (e.isTrusted) openDropdown(field); });
    field.addEventListener('blur', () => setTimeout(() => { if (dd && dd.field === field && !dd.hovered) closeDropdown(); }, 150));
    field.addEventListener('keydown', onFieldKey, true);
  }

  // ---------- dropdown ----------
  let dd = null; // { field, host, root, box, rows:[{btn, run}], active, hovered, seq }
  let ddSeq = 0;
  function closeDropdown() { if (dd) { dd.host.remove(); dd = null; } }
  function placeDropdown() {
    const r = dd.field.getBoundingClientRect();
    dd.host.style.left = Math.max(4, Math.min(r.left + scrollX, scrollX + innerWidth - 328)) + 'px';
    dd.host.style.top = (r.bottom + scrollY + 4) + 'px';
  }
  function setActive(i) {
    if (!dd || !dd.rows.length) return;
    dd.active = (i + dd.rows.length) % dd.rows.length;
    dd.rows.forEach((r, j) => r.btn.classList.toggle('active', j === dd.active));
    dd.rows[dd.active].btn.scrollIntoView({ block: 'nearest' });
  }
  function onFieldKey(e) {
    if (!dd || dd.field !== e.target) {
      if (e.key === 'ArrowDown' && e.isTrusted && (e.altKey || !e.target.value)) { e.preventDefault(); openDropdown(e.target); }
      return;
    }
    if (e.key === 'ArrowDown') { e.preventDefault(); setActive(dd.active + 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActive(dd.active - 1); }
    else if (e.key === 'Enter' && dd.active >= 0) { e.preventDefault(); e.stopPropagation(); if (e.isTrusted) dd.rows[dd.active].run(); }
    else if (e.key === 'Escape') { e.preventDefault(); closeDropdown(); }
    else if (e.key === 'Tab') closeDropdown();
  }
  function addRow(icon, title, sub, run, opts) {
    const b = el('button', 'it' + (opts && opts.primary ? ' primary' : '')); b.type = 'button'; b.tabIndex = -1;
    const ic = el('span', 'ic');
    if (icon.svg) ic.innerHTML = icon.svg; else ic.textContent = icon.text; // static SVG only, never page data
    const m = el('span', 'm'); m.append(el('div', 'ti', title)); if (sub) m.append(el('div', 'u' + (opts && opts.mono ? ' mono' : ''), sub));
    b.append(ic, m);
    b.addEventListener('mousedown', (e) => e.preventDefault());
    const row = { btn: b, run: () => run().catch(showErr) };
    b.addEventListener('click', (e) => { if (!e.isTrusted) return; e.preventDefault(); e.stopPropagation(); row.run(); });
    b.addEventListener('mouseenter', () => setActive(dd.rows.indexOf(row)));
    dd.box.append(b); dd.rows.push(row);
    return row;
  }
  function showErr(e) { if (!dd) return; const d = el('div', 'msg err', 'gitpass: ' + (e && e.message || e)); dd.box.append(d); }
  function catIcon(cat) { try { return { svg: GitpassIcons.categoryIcon(cat || 'login', 28) }; } catch (_) { return { text: '🔑' }; } }
  function clearRows() { if (!dd) return; dd.rows = []; dd.active = -1; while (dd.box.childNodes.length > 1) dd.box.lastChild.remove(); }

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
    dd = { field, host, root, box, rows: [], active: -1, hovered: false, seq: ++ddSeq };
    placeDropdown();
    await populate(ctx);
  }

  async function populate(ctx) {
    const mine = dd && dd.seq;
    const live = () => dd && dd.seq === mine;
    clearRows();
    const loading = el('div', 'msg', '…'); dd.box.append(loading);
    const { form, role } = ctx;
    const isNew = NEWPW_ROLES.has(role) || (role === 'username' && (form.kind === 'signup'));
    try {
      let items = [];
      if (GitpassForms.CARD_ROLES.has(role)) items = (await send('page:list', { category: 'credit_card' })).items;
      else if (form.kind === 'identity') items = (await send('page:list', { category: 'identity' })).items;
      else items = (await send('page:match')).items.filter((i) => !i.category || i.category === 'login');
      if (!live()) return;
      loading.remove();
      if (form.fields.newPassword && (isNew || form.kind === 'change_password')) {
        addRow({ text: '✨' }, t('csSuggest', 'Suggest strong password'), t('csSuggestSub', 'gitpass will save it when you submit'), async () => {
          const r = await send('page:generate');
          if (!originOk(r)) return;
          GitpassForms.fillNewPassword(form, r.password); // generated value is not kept after this call
          closeDropdown();
        }, { primary: true });
      }
      if (role === 'totp') items = items.filter((i) => i.has_totp);
      for (const it of items) addRow(catIcon(it.category), it.title || '(untitled)', it.subtitle || '', () => fillItem(ctx, it));
      if (!dd.rows.length) dd.box.append(el('div', 'msg', GitpassForms.CARD_ROLES.has(role) ? t('csNoCards', 'No cards in gitpass') : form.kind === 'identity' ? t('csNoIdentities', 'No identities in gitpass') : t('csNoLogins', 'No gitpass logins for this site')));
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

  async function fillItem(ctx, it) {
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
        else GitpassForms.fillLogin(f, r);
      }
      r = null; // drop secrets
    }
    closeDropdown();
  }
  // Re-detect right before writing, in case the page swapped fields since the dropdown opened.
  function freshForm(form) {
    forms = GitpassForms.detectForms(document);
    return forms.find((f) => f.scope === form.scope && f.kind === form.kind) || form;
  }

  // ---------- username tracking across multi-step flows ----------
  const USERNAME_TTL_MS = 5 * 60 * 1000;
  let inMemoryUsername = null;
  let inMemoryUsernameTs = 0;

  // Extension memory only. Never the page's sessionStorage/localStorage: those are readable and
  // writable by the page itself. Across page loads the background keeps the value per tab+origin
  // (bg-page.js `lastUsername`, 5 min TTL), and page:capture falls back to it.
  function getStoredUsername() {
    if (inMemoryUsername && (Date.now() - inMemoryUsernameTs < USERNAME_TTL_MS)) return inMemoryUsername;
    return null;
  }

  function storeUsername(user) {
    if (!user || typeof user !== 'string') return;
    const trimmed = user.trim().slice(0, 512);
    if (!trimmed || trimmed === inMemoryUsername) { inMemoryUsernameTs = Date.now(); return; }
    inMemoryUsername = trimmed;
    inMemoryUsernameTs = Date.now();
    send('page:capture', { kind: 'username', username: trimmed }).catch(() => {});
  }

  function handlePotentialUsername(el) {
    if (!el || el.tagName !== 'INPUT') return;
    const type = (el.getAttribute('type') || 'text').toLowerCase();
    if (type === 'password' || type === 'hidden') return;
    const toks = (el.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/);
    const h = GitpassForms.hints ? GitpassForms.hints(el) : ((el.name || '') + ' ' + (el.id || '') + ' ' + (el.placeholder || ''));
    if (toks.includes('username') || toks.includes('email') || type === 'email' || GitpassForms.RX.username.test(h) || GitpassForms.RX.email.test(h)) {
      if (typeof el.value === 'string' && el.value.trim().length >= 1) {
        storeUsername(el.value);
      }
    }
  }

  // ---------- save / update banner ----------
  let banner = null;
  function closeBanner() { if (banner) { banner.host.remove(); banner = null; } }
  function showBanner(p) {
    if (!p || !p.prompt || !SETTINGS.savePrompts) return;
    closeBanner();
    banner = shadowHost(true);
    banner.host.style.zIndex = '2147483647'; banner.host.style.top = '0'; banner.host.style.right = '0';
    const b = el('div', 'banner'); b.setAttribute('role', 'dialog');
    const row = el('div', 'row'); const lg = el('img'); lg.src = LOGO; lg.alt = ''; lg.style.cssText = 'width:24px;height:24px;border-radius:6px';
    const m = el('div', 'm');
    m.append(el('div', 'ti', p.prompt === 'update' ? t('csUpdateQ', 'Update password in gitpass?') : t('csSaveQ', 'Save login to gitpass?')));
    m.append(el('div', 'u', (p.title ? p.title + ' · ' : location.hostname + ' · ') + (p.username || t('csNoUser', '(no username)'))));
    row.append(lg, m);
    const acts = el('div', 'acts');
    const no = el('button', 'btn', t('csNotNow', 'Not now')); no.type = 'button';
    const never = el('button', 'btn ter', t('csNever', 'Never for this site')); never.type = 'button';
    const yes = el('button', 'btn pri', p.prompt === 'update' ? t('csUpdate', 'Update') : t('csSave', 'Save')); yes.type = 'button';
    no.addEventListener('click', (e) => { if (!e.isTrusted) return; send('page:save_decision', { action: 'dismiss' }).catch(() => {}); closeBanner(); });
    never.addEventListener('click', (e) => { if (!e.isTrusted) return; send('page:save_decision', { action: 'never', user_click: true }).catch(() => {}); closeBanner(); });
    yes.addEventListener('click', async (e) => {
      if (!e.isTrusted) return;
      yes.disabled = true;
      try {
        await send('page:save_decision', { action: p.prompt === 'update' ? 'update' : 'save', username: p.username, item_id: p.item_id, user_click: true });
        m.firstChild.textContent = p.prompt === 'update' ? t('csUpdated', 'Password updated') : t('csSaved', 'Saved to gitpass');
        acts.remove();
        setTimeout(closeBanner, 1800);
      }
      catch (err) {
        yes.disabled = false;
        const code = err.code || codeOf(err.message);
        m.lastChild.textContent = code === 'locked' ? t('csLockedShort', 'Unlock gitpass first, then try again') : 'gitpass: ' + err.message;
        if (code === 'locked') send('page:request_unlock').catch(() => {});
      }
    });
    acts.append(no, never, yes); b.append(row, acts); banner.root.append(b);
  }

  let lastCapture = 0;
  function capture(scopeHint) {
    if (!SETTINGS.savePrompts) return;
    const now = Date.now(); if (now - lastCapture < 400) return; lastCapture = now;
    const list = GitpassForms.detectForms(document);
    const cand = scopeHint ? list.filter((f) => f.scope === scopeHint || (f.scope === document && scopeHint === document)) : list;
    for (const f of (cand.length ? cand : list)) {
      let c = GitpassForms.captureCredentials(f);
      if (!c) continue;
      if (!c.username) {
        const fallback = getStoredUsername();
        if (fallback) c.username = fallback;
      }
      send('page:capture', c).then(showBanner).catch(() => {});
      c = null; // the password now lives only in the background's memory
      return;
    }
  }
  document.addEventListener('input', (e) => { if (e.isTrusted) handlePotentialUsername(e.target); }, true);
  document.addEventListener('change', (e) => { if (e.isTrusted) handlePotentialUsername(e.target); }, true);
  document.addEventListener('submit', (e) => capture(e.target), true);
  document.addEventListener('click', (e) => {
    if (!e.isTrusted) return;
    const b = e.target && e.target.closest && e.target.closest('button, input[type=submit], [role=button]');
    if (b && !b.closest('gitpass-ui')) { const f = b.form || (b.closest && b.closest('form')) || document; capture(f); }
  }, true);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.isTrusted && e.target && e.target.tagName === 'INPUT' && !dd) capture(e.target.form || document);
  }, true);

  // ---------- popup "Open & Fill" and in-page prompts (frame 0 only) ----------
  let pkPrompt = null;
  function closePkPrompt() { if (pkPrompt) { pkPrompt.host.remove(); pkPrompt = null; } }
  function showPkPrompt(msg, sendResponse) {
    closePkPrompt();
    const { host, root } = shadowHost(true);
    host.style.zIndex = '2147483647'; host.style.inset = '0'; host.style.position = 'fixed';
    const overlay = el('div', 'pk-overlay');
    const card = el('div', 'pk-card');
    card.setAttribute('role', 'dialog');
    card.setAttribute('aria-modal', 'true');

    const row = el('div', 'pk-row');
    const ic = el('div', 'pk-icon');
    const img = el('img'); img.src = LOGO; img.alt = 'gitpass'; img.style.cssText = 'width:20px;height:20px;border-radius:4px';
    ic.append(img);
    const ti = el('div', 'pk-title', msg.kind === 'create' ? t('pkCreateTitle', 'Save a passkey?') : t('pkGetTitle', 'Sign in with a passkey?'));
    row.append(ic, ti);

    const meta = el('div', 'pk-meta');
    const siteRow = el('div', 'pk-field');
    siteRow.append(el('span', 'pk-label', t('pkSite', 'Site:')), el('span', 'pk-val', msg.rpId));
    const userRow = el('div', 'pk-field');
    userRow.append(el('span', 'pk-label', t('pkAccount', 'Account:')), el('span', 'pk-val', msg.userName || '—'));
    meta.append(siteRow, userRow);

    const acts = el('div', 'pk-acts');
    const cancelBtn = el('button', 'btn', t('pkCancel', 'Cancel')); cancelBtn.type = 'button';
    const okBtn = el('button', 'btn pri', msg.kind === 'create' ? t('pkSave', 'Save passkey') : t('pkSignIn', 'Sign in')); okBtn.type = 'button';

    let answered = false;
    // Anti-clickjacking: ignore clicks that land in the first 600 ms (a page can trigger the prompt
    // right under a click it solicited).
    const shownAt = performance.now();
    const armed = () => performance.now() - shownAt > 600;
    const status = el('div', 'pk-status');
    const finish = (approved) => {
      if (answered) return;
      answered = true;
      // Approved: keep the dialog open and show progress; the background reports the outcome with
      // `gitpass-passkey-result` (success, or the error plus retry / native fallback). Never silent.
      if (approved) {
        cancelBtn.disabled = true; okBtn.disabled = true;
        status.className = 'pk-status';
        status.textContent = msg.kind === 'create' ? t('pkSaving', 'Saving to gitpass…') : t('pkSigning', 'Signing in with gitpass…');
        const mine = pkPrompt;
        setTimeout(() => {
          if (pkPrompt !== mine || mine.resulted) return;
          status.className = 'pk-status err';
          status.textContent = t('pkNoAnswer', 'No answer from gitpass (the extension may have restarted). Close this and try again.');
          const c = el('button', 'btn', t('pkClose', 'Close')); c.type = 'button';
          c.addEventListener('click', (e) => { if (e.isTrusted) closePkPrompt(); });
          acts.textContent = ''; acts.append(c);
        }, 45000);
      } else closePkPrompt();
      try { sendResponse({ approved }); } catch (_) {}
    };

    cancelBtn.addEventListener('click', (e) => { if (!e.isTrusted) return; finish(false); });
    okBtn.addEventListener('click', (e) => { if (!e.isTrusted || !armed()) return; finish(true); });
    overlay.addEventListener('keydown', (e) => { if (!e.isTrusted || e.key !== 'Escape') return; if (pkPrompt && pkPrompt.onEscape) pkPrompt.onEscape(e); else finish(false); });

    acts.append(cancelBtn, okBtn);
    card.append(row, meta, status, acts);
    overlay.append(card);
    root.append(overlay);
    // Outcome from the background after the host call.
    const showResult = (res, respond) => {
      if (pkPrompt) pkPrompt.resulted = true;
      acts.textContent = '';
      if (res.ok) {
        status.className = 'pk-status ok';
        status.textContent = msg.kind === 'create'
          ? t('pkSaved', 'Saved to gitpass') + (res.title ? ` — “${res.title}” (${t('catPasskey', 'Passkeys')})` : '')
          : t('pkSignedIn', 'Signed in with gitpass');
        setTimeout(closePkPrompt, 1800);
        respond({ action: 'done' });
        return;
      }
      status.className = 'pk-status err';
      status.textContent = (msg.kind === 'create' ? t('pkSaveFailed', 'gitpass could not save this passkey:') : t('pkGetFailed', 'gitpass could not sign in:')) + '\n' + (res.error || res.code || 'unknown error');
      let done = false;
      const pick = (action) => (e) => {
        if (!e.isTrusted || done) return;
        done = true;
        if (action === 'retry') { status.className = 'pk-status'; status.textContent = t('pkRetrying', 'Retrying…'); acts.textContent = ''; }
        else closePkPrompt();
        respond({ action });
      };
      const closeB = el('button', 'btn', t('pkClose', 'Close')); closeB.type = 'button'; closeB.addEventListener('click', pick('close'));
      const retryB = el('button', 'btn', res.code === 'locked' ? t('pkUnlockRetry', 'Unlock & retry') : t('pkRetry', 'Retry')); retryB.type = 'button'; retryB.addEventListener('click', pick('retry'));
      const nativeB = el('button', 'btn pri', t('pkUseDevice', 'Use device passkey')); nativeB.type = 'button'; nativeB.addEventListener('click', pick('native'));
      if (res.code === 'exists') acts.append(closeB, nativeB); else acts.append(closeB, retryB, nativeB);
      pkPrompt.onEscape = pick('close');
    };
    pkPrompt = { host, finish, reqId: msg.reqId, showResult };
    setTimeout(() => okBtn.focus(), 50);
  }

  chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
    if (!msg || (sender && sender.id && sender.id !== chrome.runtime.id)) return;
    if (msg.type === 'gitpass-passkey-prompt') {
      showPkPrompt(msg, sendResponse);
      return true; // async sendResponse
    }
    if (msg.type === 'gitpass-passkey-result') {
      if (!pkPrompt || pkPrompt.reqId !== msg.reqId) { sendResponse({ action: 'close' }); return; }
      pkPrompt.showResult(msg, sendResponse);
      return true;
    }
    if (msg.origin !== location.origin) return; // page navigated meanwhile: drop
    const list = GitpassForms.detectForms(document);
    if (msg.type === 'gitpass-fill') {
      const f = list.find((x) => ['login', 'login_username', 'change_password'].includes(x.kind));
      if (f) GitpassForms.fillLogin(f, msg);
    } else if (msg.type === 'gitpass-fill-fields' && (msg.kind === 'card' || msg.kind === 'identity')) {
      // fields = host fill_card/fill_identity map (gitpass-core field id -> value); lib/forms.js maps ids to form roles.
      const f = list.find((x) => x.kind === msg.kind);
      if (f) (msg.kind === 'card' ? GitpassForms.fillCard : GitpassForms.fillIdentity)(f, msg.fields || {});
    }
  });

  // ---------- wiring ----------
  document.addEventListener('mousedown', (e) => {
    const p = e.composedPath();
    if (dd && !p.includes(dd.host) && p[0] !== dd.field) closeDropdown();
  }, true);
  addEventListener('resize', placeAll); addEventListener('scroll', placeAll, true);
  let scheduled = false;
  new MutationObserver((muts) => {
    const ours = (n) => n && n.nodeName === 'GITPASS-UI';
    if (muts.every((m) => ours(m.target) || (m.type === 'childList' && [...m.addedNodes, ...m.removedNodes].every(ours)))) return; // only our own UI changed
    pruneDetachedBadges();
    if (scheduled) return; scheduled = true; setTimeout(() => { scheduled = false; rescan(); }, 250); })
    .observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['style', 'class', 'hidden', 'type'] });
  rescan();
  send('page:pending').then(showBanner).catch(() => {});
})();
