// ---------- save / update banner ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

let banner = null;
let bannerTimer = null;
function clearBannerTimer() { if (bannerTimer) { clearTimeout(bannerTimer); bannerTimer = null; } }
function closeBanner() {
  clearBannerTimer();
  if (banner) {
    if (banner.docKey) document.removeEventListener('keydown', banner.docKey, true);
    banner.host.remove();
    banner = null;
  }
}
function siteName() { return location.hostname.replace(/^www\./, ''); }
function showBanner(p) {
  if (!p || !p.prompt || !SETTINGS.savePrompts) return;
  closeBanner();
  banner = shadowHost(true);
  const me = banner;
  banner.host.style.zIndex = '2147483647'; banner.host.style.top = '0'; banner.host.style.right = '0';
  const b = el('div', 'banner'); b.setAttribute('role', 'dialog'); b.tabIndex = -1;
  const isUpdate = p.prompt === 'update';
  const isCard = p.prompt === 'save_card';
  const headTitle = isUpdate ? t('csUpdateQ', 'Update password in gitpass?')
    : isCard ? t('csSaveCardQ', 'Save card to gitpass?') : t('csSaveQ', 'Save login to gitpass?');
  b.setAttribute('aria-label', headTitle);
  const row = el('div', 'row'); const lg = el('img'); lg.src = LOGO; lg.alt = ''; lg.style.cssText = 'width:24px;height:24px;border-radius:6px';
  const m = el('div', 'm');
  m.append(el('div', 'ti', headTitle));
  // Card banner shows the brand + masked number only — never the full number.
  m.append(el('div', 'u', isCard ? ((p.brand ? p.brand + ' · ' : '') + (p.masked || ''))
    : ((p.title ? p.title + ' · ' : siteName() + ' · ') + (p.username || t('csNoUser', '(no username)')))));
  row.append(lg, m);

  const title = el('input', 'inp'); title.type = 'text'; title.value = isCard && p.title ? p.title : siteName(); title.maxLength = GitpassBanner.MAX_TITLE;
  title.autocomplete = 'off'; title.spellcheck = false;
  const titleFld = el('label', 'fld'); titleFld.append(el('span', null, t('title', 'Title')), title);
  const fields = el('div', 'fields'); fields.append(titleFld);

  const vaults = GitpassBanner.vaultList(p.vaults);
  let vault = null;
  if (vaults.length) {
    vault = el('select', 'inp');
    for (const v of vaults) { const o = el('option', null, v.name); o.value = v.id; vault.append(o); }
    vault.value = GitpassBanner.selectVault(vaults, vault.value);
    const vf = el('label', 'fld'); vf.append(el('span', null, t('vault', 'Vault')), vault);
    fields.append(vf);
  }

  const errLine = el('div', 'err'); errLine.setAttribute('role', 'alert'); errLine.hidden = true;

  const acts = el('div', 'acts');
  const no = el('button', 'btn', t('csNotNow', 'Not now')); no.type = 'button';
  const never = el('button', 'btn ter', t('csNever', 'Never for this site')); never.type = 'button';
  const saveNew = isUpdate ? el('button', 'btn', t('csSaveAsNew', 'Save as new')) : null;
  if (saveNew) saveNew.type = 'button';
  const primary = el('button', 'btn pri', isUpdate ? t('csUpdate', 'Update') : t('csSave', 'Save')); primary.type = 'button';
  const primaryLabel = primary.textContent;
  acts.append(never, el('span', 'grow'), no);
  if (saveNew) acts.append(saveNew);
  acts.append(primary);

  const controls = () => [title, vault, never, no, saveNew, primary].filter(Boolean);
  function setBusy(label) {
    me.busy = true;
    for (const c of controls()) c.disabled = true;
    b.setAttribute('aria-busy', 'true');
    primary.textContent = '';
    const sp = el('span', 'spin'); sp.setAttribute('aria-hidden', 'true');
    primary.append(sp, document.createTextNode(label));
  }
  function setIdle() {
    me.busy = false;
    for (const c of controls()) c.disabled = false;
    b.removeAttribute('aria-busy');
    primary.textContent = primaryLabel;
  }
  function showError(msg, focusEl) { if (banner !== me) return; errLine.textContent = msg; errLine.hidden = false; setIdle(); (focusEl || primary).focus(); }
  function dismiss() {
    if (banner && banner.done) { closeBanner(); return; }
    send('page:save_decision', { action: 'dismiss' }).catch(() => {});
    closeBanner();
  }
  function armClose(ms) {
    clearBannerTimer();
    bannerTimer = setTimeout(() => {
      bannerTimer = null;
      if (banner !== me || !me.done) return;
      // Programmatic focus on Open in gitpass is not interaction: only hover or focus after a real
      // click/key press keeps the banner up.
      if (me.host.matches(':hover') || (me.userActed && me.root.activeElement)) { armClose(2000); return; }
      closeBanner();
    }, ms);
  }
  function showDone(action) {
    if (banner !== me) return; // Esc closed the banner while the request was in flight
    me.busy = false;
    me.done = true;
    me.userActed = false; // the click/key that saved is over; only new interaction keeps it open
    b.removeAttribute('aria-busy');
    fields.remove(); acts.remove(); errLine.remove();
    m.firstChild.textContent = action === 'update' ? t('csUpdated', 'Password updated')
      : action === 'save_new' ? t('csSavedNew', 'Saved as a new login')
      : isCard ? t('csSavedCard', 'Card saved') : t('csSaved', 'Saved to gitpass');
    m.lastChild.textContent = siteName() + ' · ' + (isCard ? (p.masked || '') : (p.username || t('csNoUser', '(no username)')));
    const done = el('div', 'acts');
    const mark = el('span', 'ok', '✓'); mark.setAttribute('aria-hidden', 'true');
    const open = el('button', 'btn pri', t('openInApp', 'Open in gitpass')); open.type = 'button';
    open.addEventListener('click', (e) => {
      if (!e.isTrusted) return;
      send('page:open_app', { last_save: true }).catch(() => {});
      closeBanner();
    });
    done.append(mark, el('span', 'grow'), open);
    b.append(done);
    open.focus();
    armClose(4000);
  }
  async function decide(action) {
    if (action !== 'update') {
      const problem = GitpassBanner.titleError(title.value);
      if (problem) {
        showError(problem === 'too_long' ? t('csTitleTooLong', 'Title is too long (max 200 characters)') : t('csTitleRequired', 'Title is required'), title);
        return;
      }
    }
    errLine.hidden = true;
    setBusy(action === 'update' ? t('csUpdating', 'Updating…') : t('csSaving', 'Saving…'));
    try {
      const msg = { action, title: GitpassBanner.titleForSave(title.value, siteName()), user_click: true };
      const vaultId = vault ? GitpassBanner.selectVault(vaults, vault.value) : '';
      if (vaultId) msg.vault_id = vaultId;
      await send('page:save_decision', msg);
      showDone(action);
    } catch (err) {
      const code = err.code || codeOf(err.message);
      showError(code === 'locked' ? t('csLockedShort', 'Unlock gitpass first, then try again') : 'gitpass: ' + err.message);
      if (code === 'locked') send('page:request_unlock').catch(() => {});
    }
  }

  no.addEventListener('click', (e) => { if (!e.isTrusted) return; dismiss(); });
  never.addEventListener('click', (e) => { if (!e.isTrusted) return; send('page:save_decision', { action: 'never', user_click: true }).catch(() => {}); closeBanner(); });
  primary.addEventListener('click', (e) => { if (!e.isTrusted) return; decide(isUpdate ? 'update' : 'save'); });
  if (saveNew) saveNew.addEventListener('click', (e) => { if (!e.isTrusted) return; decide('save_new'); });

  // Esc = Not now, even when focus left the banner; Tab stays inside it while open (including while
  // Saving…, when every control is disabled and focus has fallen back to the page).
  const onDocKey = (e) => {
    if (!e.isTrusted) return;
    if (GitpassBanner.isEscape(e.key)) {
      if (me.busy) return;
      e.preventDefault(); e.stopPropagation(); dismiss(); return;
    }
    if (e.key !== 'Tab') return;
    const list = Array.from(banner.root.querySelectorAll('input,select,button')).filter((x) => !x.disabled);
    if (!list.length) { e.preventDefault(); e.stopPropagation(); b.focus(); return; }
    if (!banner.root.activeElement) return; // focus is on the page, not in the dialog
    const at = list.indexOf(banner.root.activeElement);
    const next = GitpassBanner.trapTab(e.key, e.shiftKey, at, list.length);
    if (next != null) { e.preventDefault(); e.stopPropagation(); list[next].focus(); }
  };
  document.addEventListener('keydown', onDocKey, true);
  banner.docKey = onDocKey;
  banner.host.addEventListener('mouseenter', clearBannerTimer);
  banner.host.addEventListener('focusin', clearBannerTimer);
  banner.host.addEventListener('mouseleave', () => { if (banner && banner.done) armClose(4000); });
  banner.host.addEventListener('focusout', () => { if (banner && banner.done) armClose(4000); });
  banner.host.addEventListener('pointerdown', () => { me.userActed = true; }, true);
  banner.host.addEventListener('keydown', (e) => { if (e.isTrusted) me.userActed = true; }, true);

  b.append(row, fields, errLine, acts); banner.root.append(b);
  primary.focus();
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
    if (c.kind !== 'card' && !c.username) {
      const fallback = getStoredUsername();
      if (fallback) c.username = fallback;
    }
    send('page:capture', c).then(showBanner).catch(() => {});
    c = null; // the password now lives only in the background's memory
    return;
  }
}
// Document-level capture listeners: only when the guards in content.js passed.
if (window.__gitpassInpage) {
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
}
