// ---------- ⌘⇧L / Ctrl+Shift+L "fill-login" command (#124) ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

// The background decides (one match: fills via `gitpass-fill`; several: `gitpass-command-menu`;
// none / locked / app not running: `gitpass-command-notice`). Picking from the menu still needs a
// trusted click or Enter, like every other in-page fill.
const COMMAND_FORM_KINDS = new Set(['login', 'login_username']);
function commandAnchor() {
  forms = GitpassForms.detectForms(document);
  const a = document.activeElement;
  const here = a && a.tagName === 'INPUT' ? GitpassForms.locate(forms, a) : null;
  if (here && COMMAND_FORM_KINDS.has(here.form.kind) && (here.role === 'username' || here.role === 'password')) return a;
  for (const f of forms) {
    if (!COMMAND_FORM_KINDS.has(f.kind)) continue;
    for (const { el: field } of GitpassForms.anchorFields(f)) if (field && field.isConnected) return field;
  }
  return null;
}
function openCommandMenu() {
  const field = commandAnchor();
  if (!field) { showNotice('no_form'); return; }
  if (!bound.has(field)) { bound.add(field); bindField(field); } // also when in-page suggestions are off
  if (dd && dd.field !== field) closeDropdown();
  field.focus();
  openDropdown(field);
}

let notice = null;
function closeNotice() { if (notice) { clearTimeout(notice.timer); notice.host.remove(); notice = null; } }
function noticeText(code) {
  switch (code) {
    case 'locked': return t('csLockedShort', 'Unlock gitpass first, then try again');
    case 'app_not_running': return t('csCmdAppNotRunning', 'Open the gitpass app, then try again');
    case 'no_match': return t('csNoLogins', 'No gitpass logins for this site');
    case 'no_form': return t('csCmdNoForm', 'No login form found on this page');
    default: return t('csCmdFailed', 'gitpass could not fill this page');
  }
}
function showNotice(code) {
  closeNotice();
  const n = shadowHost(true);
  n.host.style.zIndex = '2147483647'; n.host.style.top = '0'; n.host.style.right = '0';
  const b = el('div', 'banner'); b.setAttribute('role', 'status'); b.setAttribute('aria-live', 'polite');
  const row = el('div', 'row'); const lg = el('img'); lg.src = LOGO; lg.alt = ''; lg.style.cssText = 'width:24px;height:24px;border-radius:6px';
  const m = el('div', 'm'); const txt = el('div', 'u', noticeText(code)); txt.style.whiteSpace = 'normal';
  m.append(el('div', 'ti', 'gitpass'), txt);
  row.append(lg, m); b.append(row); n.root.append(b);
  notice = { host: n.host, timer: setTimeout(closeNotice, 4000) };
}
