/* global chrome, GitpassOnboard, GitpassI18n */
'use strict';

/**
 * Pure state mapping for connection status.
 * Returns state ('unlocked' | 'locked' | 'not_found'), label key, and whether Retry/Docs should appear.
 */
function connectionState(probeResult, err) {
  if (err || !probeResult) {
    return {
      state: 'not_found',
      labelKey: 'statusAppNotFound',
      defaultLabel: 'Desktop app not found',
      canRetry: true,
    };
  }

  // Handle probe result from onboard-probe ({ host, app, unlocked })
  if ('host' in probeResult || 'app' in probeResult) {
    if (!probeResult.host || probeResult.app === false) {
      return {
        state: 'not_found',
        labelKey: 'statusAppNotFound',
        defaultLabel: 'Desktop app not found',
        canRetry: true,
      };
    }
    if (probeResult.unlocked) {
      return {
        state: 'unlocked',
        labelKey: 'statusConnectedUnlocked',
        defaultLabel: 'Connected to gitpass (unlocked)',
        canRetry: false,
      };
    }
    return {
      state: 'locked',
      labelKey: 'statusAppLocked',
      defaultLabel: 'gitpass is locked',
      canRetry: false,
    };
  }

  // Handle direct status object ({ unlocked, app_running })
  if (probeResult.app_running === false) {
    return {
      state: 'not_found',
      labelKey: 'statusAppNotFound',
      defaultLabel: 'Desktop app not found',
      canRetry: true,
    };
  }
  if (probeResult.unlocked) {
    return {
      state: 'unlocked',
      labelKey: 'statusConnectedUnlocked',
      defaultLabel: 'Connected to gitpass (unlocked)',
      canRetry: false,
    };
  }
  return {
    state: 'locked',
    labelKey: 'statusAppLocked',
    defaultLabel: 'gitpass is locked',
    canRetry: false,
  };
}

/**
 * Pure function to remove a domain from a list (case-insensitive).
 */
function removeDomain(list, domain) {
  const norm = (domain || '').trim().toLowerCase();
  if (!norm) return Array.isArray(list) ? [...list] : [];
  return (list || []).filter((d) => (d || '').trim().toLowerCase() !== norm);
}

/**
 * Pure function to restore a domain at a specific index (or append) without duplicates.
 */
function restoreDomain(list, domain, index) {
  const norm = (domain || '').trim().toLowerCase();
  if (!norm) return Array.isArray(list) ? [...list] : [];
  const current = (list || []).filter((d) => (d || '').trim().toLowerCase() !== norm);
  const targetIndex = (typeof index === 'number' && index >= 0 && index <= current.length)
    ? index
    : current.length;
  current.splice(targetIndex, 0, norm);
  return current;
}

/**
 * Pure function to normalize a domain for the exception lists: trim, lowercase, strip a
 * leading www. (matches the save banner's siteName()).
 */
function normalizeDomain(domain) {
  return (domain || '').trim().toLowerCase().replace(/^www\./, '');
}

// Browser DOM logic
const $ = (id) => (typeof document !== 'undefined' ? document.getElementById(id) : null);
const t = (k, f, subs) => (globalThis.GitpassI18n ? GitpassI18n.t(k, f, subs) : f);

let toastTimer = null;
function showToast(msg) {
  const el = $('saved');
  if (!el) return;
  el.textContent = msg;
  el.classList.remove('hidden');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    if (el.textContent === msg) {
      el.classList.add('hidden');
      el.textContent = '';
    }
  }, 1400);
}

let lastRemoved = null;

function hideUndoBanner() {
  const banner = $('never-save-undo-banner');
  if (banner) banner.classList.add('hidden');
  if (lastRemoved && lastRemoved.timer) {
    clearTimeout(lastRemoved.timer);
    lastRemoved.timer = null;
  }
}

function showUndoBanner(domain) {
  const banner = $('never-save-undo-banner');
  const textEl = $('never-save-undo-text');
  const undoBtn = $('never-save-undo-btn');
  if (!banner || !textEl) return;

  textEl.textContent = t('siteRemoved', `Removed ${domain}`, [domain]);
  banner.classList.remove('hidden');

  if (undoBtn) {
    undoBtn.onclick = async () => {
      await onUndoRemove();
    };
  }
}

async function onRemoveDomain(domain) {
  const current = await GitpassOnboard.getNeverSave();
  const idx = current.findIndex((d) => (d || '').toLowerCase() === domain.toLowerCase());
  const next = removeDomain(current, domain);
  await GitpassOnboard.setNeverSave(next);

  if (lastRemoved && lastRemoved.timer) clearTimeout(lastRemoved.timer);
  lastRemoved = {
    domain,
    index: idx >= 0 ? idx : next.length,
    timer: setTimeout(() => {
      hideUndoBanner();
      lastRemoved = null;
    }, 8000),
  };

  showUndoBanner(domain);
  showToast(t('saved', 'Saved'));
  await renderNeverSave();
}

async function onUndoRemove() {
  if (!lastRemoved) return;
  const { domain, index, timer } = lastRemoved;
  if (timer) clearTimeout(timer);
  lastRemoved = null;

  const current = await GitpassOnboard.getNeverSave();
  const next = restoreDomain(current, domain, index);
  await GitpassOnboard.setNeverSave(next);

  hideUndoBanner();
  showToast(t('saved', 'Saved'));
  await renderNeverSave();
}

async function renderNeverSave() {
  const listEl = $('never-save-list');
  const emptyEl = $('never-save-empty');
  const emptyDescEl = $('never-save-empty-desc');
  if (!listEl || !emptyEl) return;

  const domains = await GitpassOnboard.getNeverSave();
  listEl.textContent = '';

  if (!domains.length) {
    emptyEl.style.display = 'block';
    if (emptyDescEl) emptyDescEl.classList.remove('hidden');
    return;
  }

  emptyEl.style.display = 'none';
  if (emptyDescEl) emptyDescEl.classList.add('hidden');

  for (const domain of domains) {
    const row = document.createElement('div');
    row.className = 't setting-row never-save-row';

    const info = document.createElement('div');
    info.className = 'never-save-info';
    const b = document.createElement('b');
    b.className = 'domain-name';
    b.textContent = domain;
    info.append(b);

    const actions = document.createElement('div');
    actions.className = 'setting-control';
    const btn = document.createElement('button');
    btn.className = 'btn btn-sec sec';
    btn.type = 'button';
    btn.setAttribute('data-domain', domain);
    btn.textContent = t('remove', 'Remove');
    btn.onclick = () => onRemoveDomain(domain);

    actions.append(btn);
    row.append(info, actions);
    listEl.append(row);
  }
}

async function renderNeverAutoSubmit() {
  const listEl = $('never-auto-submit-list');
  const emptyEl = $('never-auto-submit-empty');
  const emptyDescEl = $('never-auto-submit-empty-desc');
  if (!listEl || !emptyEl) return;

  const domains = await GitpassOnboard.getNeverAutoSubmit();
  listEl.textContent = '';

  if (!domains.length) {
    emptyEl.style.display = 'block';
    if (emptyDescEl) emptyDescEl.classList.remove('hidden');
    return;
  }

  emptyEl.style.display = 'none';
  if (emptyDescEl) emptyDescEl.classList.add('hidden');

  for (const domain of domains) {
    const row = document.createElement('div');
    row.className = 't setting-row never-save-row';

    const info = document.createElement('div');
    info.className = 'never-save-info';
    const b = document.createElement('b');
    b.className = 'domain-name';
    b.textContent = domain;
    info.append(b);

    const actions = document.createElement('div');
    actions.className = 'setting-control';
    const btn = document.createElement('button');
    btn.className = 'btn btn-sec sec';
    btn.type = 'button';
    btn.setAttribute('data-domain', domain);
    btn.textContent = t('remove', 'Remove');
    btn.onclick = async () => {
      const current = await GitpassOnboard.getNeverAutoSubmit();
      await GitpassOnboard.setNeverAutoSubmit(removeDomain(current, domain));
      showToast(t('saved', 'Saved'));
      await renderNeverAutoSubmit();
    };

    actions.append(btn);
    row.append(info, actions);
    listEl.append(row);
  }
}

async function onAddNeverAutoSubmit() {
  const input = $('never-auto-submit-add');
  if (!input) return;
  const domain = normalizeDomain(input.value);
  if (!domain) return;
  const current = await GitpassOnboard.getNeverAutoSubmit();
  if (!current.some((d) => (d || '').toLowerCase() === domain)) {
    await GitpassOnboard.setNeverAutoSubmit([...current, domain]);
  }
  input.value = '';
  showToast(t('saved', 'Saved'));
  await renderNeverAutoSubmit();
}

async function renderHiddenInlineMenu() {
  const listEl = $('hidden-inline-menu-list');
  const emptyEl = $('hidden-inline-menu-empty');
  const emptyDescEl = $('hidden-inline-menu-empty-desc');
  if (!listEl || !emptyEl) return;

  const domains = await GitpassOnboard.getHiddenInlineMenu();
  listEl.textContent = '';

  if (!domains.length) {
    emptyEl.style.display = 'block';
    if (emptyDescEl) emptyDescEl.classList.remove('hidden');
    return;
  }

  emptyEl.style.display = 'none';
  if (emptyDescEl) emptyDescEl.classList.add('hidden');

  for (const domain of domains) {
    const row = document.createElement('div');
    row.className = 't setting-row never-save-row';

    const info = document.createElement('div');
    info.className = 'never-save-info';
    const b = document.createElement('b');
    b.className = 'domain-name';
    b.textContent = domain;
    info.append(b);

    const actions = document.createElement('div');
    actions.className = 'setting-control';
    const btn = document.createElement('button');
    btn.className = 'btn btn-sec sec';
    btn.type = 'button';
    btn.setAttribute('data-domain', domain);
    btn.textContent = t('remove', 'Remove');
    btn.onclick = async () => {
      const current = await GitpassOnboard.getHiddenInlineMenu();
      await GitpassOnboard.setHiddenInlineMenu(removeDomain(current, domain));
      showToast(t('saved', 'Saved'));
      await renderHiddenInlineMenu();
    };

    actions.append(btn);
    row.append(info, actions);
    listEl.append(row);
  }
}

async function onAddHiddenInlineMenu() {
  const input = $('hidden-inline-menu-add');
  if (!input) return;
  const domain = normalizeDomain(input.value);
  if (!domain) return;
  const current = await GitpassOnboard.getHiddenInlineMenu();
  if (!current.some((d) => (d || '').toLowerCase() === domain)) {
    await GitpassOnboard.setHiddenInlineMenu([...current, domain]);
  }
  input.value = '';
  showToast(t('saved', 'Saved'));
  await renderHiddenInlineMenu();
}

async function checkStatus() {
  const card = $('connection-card');
  const dot = $('status-dot');
  const text = $('status-text');
  const actions = $('status-troubleshoot-wrap');
  if (!card || !dot || !text) return;

  dot.className = 'status-dot checking';
  text.textContent = t('checking', 'Checking…');
  if (actions) actions.classList.add('hidden');

  let probe = null;
  let err = null;
  try {
    probe = await GitpassOnboard.send({ type: 'onboard-probe' });
  } catch (e) {
    err = e;
  }

  const st = connectionState(probe, err);
  card.className = 'card status-card status-' + st.state;
  dot.className = 'status-dot ' + st.state;
  text.textContent = t(st.labelKey, st.defaultLabel);

  if (actions) {
    actions.classList.toggle('hidden', !st.canRetry);
  }
}

async function initSettings() {
  const s = await GitpassOnboard.getSettings();
  for (const k of Object.keys(GitpassOnboard.DEFAULTS)) {
    const el = $(k);
    if (!el) continue;
    el.checked = !!s[k];
    el.onchange = async () => {
      await GitpassOnboard.setSettings({ [k]: el.checked });
      showToast(t('saved', 'Saved'));
    };
  }
  // List density (#397): string setting, stored alongside the boolean toggles.
  const density = $('density');
  if (density) {
    density.value = s.density === 'compact' ? 'compact' : 'comfortable';
    density.onchange = async () => {
      await GitpassOnboard.setSettings({ density: density.value === 'compact' ? 'compact' : 'comfortable' });
      showToast(t('saved', 'Saved'));
    };
  }
}

async function renderShortcut() {
  const shortcutEl = $('shortcut');
  const fillShortcutEl = $('fill-shortcut');
  if (shortcutEl || fillShortcutEl) {
    try {
      const r = await GitpassOnboard.send({ type: 'onboard-shortcut' });
      if (shortcutEl) shortcutEl.textContent = r.shortcut || 'not set';
      if (fillShortcutEl) fillShortcutEl.textContent = r.fillShortcut || 'not set';
    } catch (_) {
      if (shortcutEl) shortcutEl.textContent = 'Cmd+Shift+X';
      if (fillShortcutEl) fillShortcutEl.textContent = 'Cmd+Shift+L';
    }
  }

  const scBtn = $('change-shortcut');
  if (scBtn) {
    scBtn.onclick = () => {
      if (globalThis.chrome && chrome.tabs && chrome.tabs.create) {
        chrome.tabs.create({ url: 'chrome://extensions/shortcuts' });
      }
    };
  }
}

function renderAbout() {
  const vEl = $('extension-version');
  if (!vEl) return;
  try {
    const v = (globalThis.chrome && chrome.runtime && chrome.runtime.getManifest && chrome.runtime.getManifest().version) || '—';
    vEl.textContent = v;
  } catch (_) {
    vEl.textContent = '—';
  }
}

async function init() {
  if (globalThis.GitpassI18n) {
    GitpassI18n.apply(document);
  }

  const retryBtn = $('status-retry-btn');
  if (retryBtn) {
    retryBtn.onclick = () => checkStatus();
  }

  renderAbout();
  await initSettings();
  await renderShortcut();
  await renderNeverSave();
  await renderNeverAutoSubmit();
  await renderHiddenInlineMenu();
  const hiddenAddBtn = $('hidden-inline-menu-add-btn');
  if (hiddenAddBtn) hiddenAddBtn.onclick = () => onAddHiddenInlineMenu();
  const hiddenAddInput = $('hidden-inline-menu-add');
  if (hiddenAddInput) hiddenAddInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); onAddHiddenInlineMenu(); } });
  const addBtn = $('never-auto-submit-add-btn');
  if (addBtn) addBtn.onclick = () => onAddNeverAutoSubmit();
  const addInput = $('never-auto-submit-add');
  if (addInput) addInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); onAddNeverAutoSubmit(); } });
  await checkStatus();

  // Live status updates from host/lock events without ever querying or showing item data
  if (globalThis.chrome && chrome.runtime && chrome.runtime.onMessage && chrome.runtime.onMessage.addListener) {
    chrome.runtime.onMessage.addListener((msg) => {
      if (!msg || msg.type !== 'popup:event') return;
      if (msg.event === 'unlock' || msg.event === 'lock' || msg.event === 'changed') {
        checkStatus();
      }
    });
  }

  if (typeof window !== 'undefined') {
    window.addEventListener('focus', () => checkStatus());
  }
}

if (typeof document !== 'undefined') {
  init();
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    connectionState,
    removeDomain,
    restoreDomain,
    normalizeDomain,
    checkStatus,
    renderNeverSave,
    renderNeverAutoSubmit,
    renderHiddenInlineMenu,
    onAddHiddenInlineMenu,
    onRemoveDomain,
    onUndoRemove,
    onAddNeverAutoSubmit,
  };
}
