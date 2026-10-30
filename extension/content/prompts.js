// ---------- popup "Open & Fill" and in-page prompts (frame 0 only) ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

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
  meta.append(siteRow);

  const matching = Array.isArray(msg.matching) ? msg.matching : [];
  const isMultiChoice = msg.kind === 'get' && matching.length >= 2;

  let selectedCredId = matching[0] ? matching[0].credential_id : null;
  let selectedIndex = 0;
  let listHeader = null;
  let listEl = null;
  let rowEls = [];

  if (!isMultiChoice) {
    const userRow = el('div', 'pk-field');
    userRow.append(el('span', 'pk-label', t('pkAccount', 'Account:')), el('span', 'pk-val', msg.userName || '—'));
    meta.append(userRow);
  } else {
    const vaultNames = new Set(matching.map((m) => m.vault_name).filter(Boolean));
    const showVaultName = vaultNames.size > 1;

    listHeader = el('div', 'pk-list-header');
    const labelSpan = el('span', 'pk-list-label', t('pkChooseAccount', 'Choose an account'));
    const countText = t('pkPasskeysCount', '{n} passkeys for this site').replace('{n}', matching.length);
    const countSpan = el('span', 'pk-list-count', countText);
    listHeader.append(labelSpan, countSpan);

    listEl = el('div', 'pk-list');
    listEl.setAttribute('role', 'listbox');
    listEl.setAttribute('aria-label', t('pkChooseAccount', 'Choose an account'));

    function selectAccount(idx, shouldFocus = true) {
      if (idx < 0 || idx >= matching.length) return;
      selectedIndex = idx;
      selectedCredId = matching[idx].credential_id;
      rowEls.forEach((r, idx2) => {
        const isSel = idx2 === selectedIndex;
        r.classList.toggle('selected', isSel);
        r.setAttribute('aria-selected', isSel ? 'true' : 'false');
        r.setAttribute('tabindex', isSel ? '0' : '-1');
      });
      listEl.setAttribute('aria-activedescendant', `pk-opt-${selectedIndex}`);
      const activeRow = rowEls[selectedIndex];
      if (activeRow) {
        activeRow.scrollIntoView({ block: 'nearest' });
        if (shouldFocus) activeRow.focus();
      }
    }

    rowEls = matching.map((m, idx) => {
      const itemEl = el('div', 'pk-item' + (idx === 0 ? ' selected' : ''));
      itemEl.setAttribute('role', 'option');
      itemEl.setAttribute('id', `pk-opt-${idx}`);
      itemEl.setAttribute('aria-selected', idx === 0 ? 'true' : 'false');
      itemEl.setAttribute('tabindex', idx === 0 ? '0' : '-1');

      const titleText = m.user_name || '—';
      itemEl.title = m.user_name || '';

      const iconObj = itemIcon({ category: 'login', favicon_host: msg.rpId, url: 'https://' + msg.rpId, title: m.user_name || msg.rpId });
      const iconWrap = el('div', 'ic');
      if (iconObj && iconObj.dom) iconWrap.appendChild(iconObj.dom);

      const infoEl = el('div', 'pk-item-info');
      const userSpan = el('span', 'pk-item-user', titleText);
      userSpan.title = m.user_name || '';
      infoEl.appendChild(userSpan);

      if (showVaultName && m.vault_name) {
        const vaultSpan = el('span', 'pk-item-vault', m.vault_name);
        infoEl.appendChild(vaultSpan);
      }

      itemEl.append(iconWrap, infoEl);

      itemEl.addEventListener('click', (e) => {
        if (!e.isTrusted) return;
        selectAccount(idx, true);
      });
      itemEl.addEventListener('dblclick', (e) => {
        if (!e.isTrusted || !armed()) return;
        selectAccount(idx, true);
        finish(true);
      });

      return itemEl;
    });

    rowEls.forEach((r) => listEl.appendChild(r));

    listEl.addEventListener('keydown', (e) => {
      if (!e.isTrusted) return;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        selectAccount(Math.min(matching.length - 1, selectedIndex + 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        selectAccount(Math.max(0, selectedIndex - 1));
      } else if (e.key === 'Home') {
        e.preventDefault();
        selectAccount(0);
      } else if (e.key === 'End') {
        e.preventDefault();
        selectAccount(matching.length - 1);
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (!armed()) return;
        finish(true);
      } else if (e.key === 'Escape') {
        e.preventDefault();
        finish(false);
      }
    });
  }

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
    try { sendResponse({ approved, credential_id: selectedCredId, credentialId: selectedCredId }); } catch (_) {}
  };

  cancelBtn.addEventListener('click', (e) => { if (!e.isTrusted) return; finish(false); });
  okBtn.addEventListener('click', (e) => { if (!e.isTrusted || !armed()) return; finish(true); });
  overlay.addEventListener('keydown', (e) => { if (!e.isTrusted || e.key !== 'Escape') return; if (pkPrompt && pkPrompt.onEscape) pkPrompt.onEscape(e); else finish(false); });

  acts.append(cancelBtn, okBtn);
  if (isMultiChoice) {
    card.append(row, meta, listHeader, listEl, status, acts);
  } else {
    card.append(row, meta, status, acts);
  }
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
  if (isMultiChoice) {
    setTimeout(() => {
      if (rowEls[0]) rowEls[0].focus();
      else okBtn.focus();
    }, 50);
  } else {
    setTimeout(() => okBtn.focus(), 50);
  }
}

// Right-click target for the context menu's "Suggest strong password" (#213):
// the background only knows the clicked frame, not the field, so the last
// contextmenu target is remembered here when it is a password field. Stale
// targets (detached, disabled, read-only) are ignored by the handler below.
let lastContextMenuField = null;
// Page-message and context-menu listeners: only when the guards in content.js passed.
if (window.__gitpassInpage) {
  document.addEventListener('contextmenu', (e) => {
    const el = e.target;
    if (el && el.tagName === 'INPUT' && (el.getAttribute('type') || 'text').toLowerCase() === 'password') lastContextMenuField = el;
  }, true);

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
    if (msg.event === 'lock') {
      siteIconCache.clear();
      siteIconPending.clear();
    }
    if (msg.origin !== location.origin) return; // page navigated meanwhile: drop
    if (msg.type === 'gitpass-can-fill-generated') { sendResponse({ canFill: !!focusedPasswordField() }); return; }
    if (msg.type === 'gitpass-fill-generated') {
      const field = focusedPasswordField();
      if (!field) { sendResponse({ filled: false }); return; }
      GitpassForms.setValue(field, String(msg.password || ''));
      sendResponse({ filled: true });
      return;
    }
    if (msg.type === 'gitpass-command-menu') { openCommandMenu(); return; }
    if (msg.type === 'gitpass-command-notice') { showNotice(msg.code); return; }
    if (msg.type === 'gitpass-suggest-password') {
      // Context menu (#213): same generate path as the in-page suggest row
      // (field passwordrules -> page:generate -> fillNewPassword). The filled
      // password is then captured by the usual save banner on submit; it is
      // never kept here beyond this call. Async: keep the channel open.
      (async () => {
        let field = null;
        if (lastContextMenuField && document.contains(lastContextMenuField) && !lastContextMenuField.disabled && !lastContextMenuField.readOnly) field = lastContextMenuField;
        if (!field) field = focusedPasswordField();
        if (!field) { sendResponse({ filled: false }); return; }
        const PR = globalThis.GitpassPasswordRules;
        const rules = PR ? PR.fromField(field) : null;
        const fit = rules ? PR.sanitize(rules) : null;
        if (fit && !fit.ok) { sendResponse({ filled: false }); return; } // site rules too short: refuse, don't weaken
        const r = await send('page:generate', rules ? { rules } : {});
        if (!originOk(r)) { sendResponse({ filled: false }); return; }
        const forms = GitpassForms.detectForms(document);
        const form = forms.find((f) => f && f.fields && Object.values(f.fields).includes(field));
        if (form && form.fields.newPassword) GitpassForms.fillNewPassword(form, r.password);
        else GitpassForms.setValue(field, r.password);
        sendResponse({ filled: true });
      })().catch(() => { try { sendResponse({ filled: false }); } catch (_) {} });
      return true;
    }
    const list = GitpassForms.detectForms(document);
    if (msg.type === 'gitpass-fill') {
      const f = list.find((x) => ['login', 'login_username', 'change_password'].includes(x.kind));
      if (f) { GitpassForms.fillLogin(f, msg); if (f.kind !== 'change_password') maybeAutoSubmit(f, msg.origin); }
      else if (msg.via === 'command') showNotice('no_form');
    } else if (msg.type === 'gitpass-fill-fields' && (msg.kind === 'card' || msg.kind === 'identity')) {
      // fields = host fill_card/fill_identity map (gitpass-core field id -> value); lib/forms.js maps ids to form roles.
      const f = list.find((x) => x.kind === msg.kind);
      if (f) (msg.kind === 'card' ? GitpassForms.fillCard : GitpassForms.fillIdentity)(f, msg.fields || {});
    }
  });
}
