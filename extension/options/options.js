/* global chrome, GitpassOnboard, GitpassI18n */
'use strict';
const $ = (id) => document.getElementById(id);

async function renderNeverSave() {
  const listEl = $('never-save-list');
  const emptyEl = $('never-save-empty');
  if (!listEl || !emptyEl) return;
  const domains = await GitpassOnboard.getNeverSave();
  listEl.textContent = '';
  if (!domains.length) {
    emptyEl.style.display = 'block';
    return;
  }
  emptyEl.style.display = 'none';
  for (const domain of domains) {
    const row = document.createElement('div');
    row.className = 't';
    row.style.cssText = 'display:flex;align-items:center;justify-content:space-between;gap:16px;padding:10px 0;border-bottom:1px solid var(--line)';
    const span = document.createElement('span');
    const b = document.createElement('b');
    b.textContent = domain;
    span.append(b);
    const btn = document.createElement('button');
    btn.className = 'sec';
    btn.type = 'button';
    btn.setAttribute('data-domain', domain);
    btn.textContent = globalThis.GitpassI18n ? GitpassI18n.t('remove', 'Remove') : 'Remove';
    btn.onclick = async () => {
      await GitpassOnboard.removeNeverSave(domain);
      const statusEl = $('saved');
      if (statusEl) {
        statusEl.textContent = globalThis.GitpassI18n ? GitpassI18n.t('saved', 'Saved') : 'Saved';
        setTimeout(() => { if (statusEl.textContent === (globalThis.GitpassI18n ? GitpassI18n.t('saved', 'Saved') : 'Saved')) statusEl.textContent = ''; }, 1200);
      }
      await renderNeverSave();
    };
    row.append(span, btn);
    listEl.append(row);
  }
}

(async () => {
  if (globalThis.GitpassI18n) GitpassI18n.apply(document);
  const s = await GitpassOnboard.getSettings();
  for (const k of Object.keys(GitpassOnboard.DEFAULTS)) {
    const el = $(k);
    if (!el) continue;
    el.checked = !!s[k];
    el.onchange = async () => { await GitpassOnboard.setSettings({ [k]: el.checked }); $('saved').textContent = 'Saved'; setTimeout(() => { $('saved').textContent = ''; }, 1200); };
  }
  try { const r = await GitpassOnboard.send({ type: 'onboard-shortcut' }); $('shortcut').textContent = r.shortcut || 'not set'; } catch (_) { $('shortcut').textContent = 'Cmd+Shift+X'; }
  const scBtn = $('change-shortcut');
  if (scBtn) {
    scBtn.onclick = () => {
      const isFirefox = typeof browser !== 'undefined' || navigator.userAgent.includes('Firefox');
      if (isFirefox) $('saved').textContent = 'Firefox: about:addons → ⚙ → Manage Extension Shortcuts';
      else chrome.tabs.create({ url: 'chrome://extensions/shortcuts' });
    };
  }
  await renderNeverSave();
})();
