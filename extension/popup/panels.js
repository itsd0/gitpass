'use strict';
// ---------- panels: generator / new login / shortcuts ----------
let prevFocus = null;

function closePanel() {
  const p = $('panel');
  p.classList.add('hidden');
  p.classList.remove('shortcuts-panel');
  p.classList.remove('wt-panel');
  p.textContent = '';
  p.removeAttribute('aria-modal');
  p.removeAttribute('aria-label');
  const wasShortcuts = S.panel === 'shortcuts';
  const wasWatchtower = S.panel === 'watchtower';
  S.panel = null;
  wtDrillKind = null;
  if ((wasShortcuts || wasWatchtower) && prevFocus && typeof prevFocus.focus === 'function' && prevFocus.isConnected) {
    prevFocus.focus();
  } else {
    $('q').focus();
  }
  prevFocus = null;
  if (livePending) { livePending = false; liveReload(); }
  else if (wasWatchtower && typeof refreshWatchtowerBadge === 'function') refreshWatchtowerBadge();
}

function panelHead(title) { const h = el('h3', null, title); h.append(iconBtn(UI.close, t('close', 'Close'), closePanel)); return h; }

// ---------- site settings panel (issue #341) ----------
// Per-site toggles for the active tab's origin: save prompts, submit-after-fill
// (only when the global opt-in is on) and the inline menu. Backed by the
// GitpassSiteSettings storage helpers (popup/sitesettings.js).
async function openSiteSettings(origin) {
  const SS = typeof GitpassSiteSettings !== 'undefined' ? GitpassSiteSettings : null;
  if (!SS) return;
  const domain = SS.domainOf(origin);
  if (!domain) return;

  prevFocus = document.activeElement;
  const p = $('panel');
  p.textContent = '';
  p.classList.remove('hidden');
  p.classList.remove('shortcuts-panel');
  p.setAttribute('role', 'dialog');
  p.setAttribute('aria-modal', 'true');
  p.setAttribute('aria-label', t('siteSettings', 'Site settings'));
  S.panel = 'site-settings';

  p.append(panelHead(t('siteSettingsFor', 'Site settings: $1', [domain])));

  const globals = await popupSettings();
  const [neverSave, neverAuto, hiddenMenu] = await Promise.all([SS.getNeverSave(), SS.getNeverAutoSubmit(), SS.getHiddenInlineMenu()]);

  const list = el('div', 'site-settings-list');
  const mkRow = (labelKey, fallback, on, onFlip) => {
    const l = el('label', 'tog site-settings-row');
    const c = Object.assign(el('input'), { type: 'checkbox' });
    c.checked = on;
    c.onchange = async () => {
      c.disabled = true;
      try { await onFlip(c.checked); } catch (_) { /* keep prior state on storage failure */ c.checked = !c.checked; }
      c.disabled = false;
    };
    l.append(c, el('span', null, t(labelKey, fallback)));
    list.append(l);
    return c;
  };

  mkRow('siteSaveLogins', 'Offer to save logins on this site', !neverSave.includes(domain), async (v) => {
    await SS.setSavePrompts(domain, v);
    await refreshNeverSave();
    renderFilter();
  });
  if (globals.autoSubmitAfterFill) {
    mkRow('siteAutoSubmit', 'Submit after fill on this site', !neverAuto.includes(domain), (v) => SS.setAutoSubmit(domain, v));
  }
  mkRow('siteInlineMenu', 'Show inline menu on this site', !hiddenMenu.includes(domain), (v) => SS.setInlineMenuShown(domain, v));

  p.append(list);
  const hint = el('p', 'muted site-settings-hint', t('siteSettingsHint', 'Changes apply the next time the page loads or rescans.'));
  p.append(hint);
  const first = list.querySelector('input');
  if (first) first.focus();
}

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
  const alt = isMac ? '⌥' : 'Alt+';

  const list = el('div', 'shortcuts-list');
  list.setAttribute('role', 'list');

  const shortcuts = [
    { keys: ['↑', '↓'], label: t('shortcutNav', 'Navigate list') },
    { keys: ['←', '→'], label: t('shortcutFilterChips', 'Move between filter chips') },
    { keys: [isMac ? '↵' : 'Enter'], label: t('shortcutFill', 'Fill item') },
    { keys: [`${cmd}C`], label: t('shortcutCopyPw', 'Copy password') },
    { keys: [`${cmd}${alt}L`], label: t('shortcutLargeType', 'Show in large type') },
    { keys: [`${cmd}${shift}C`], label: t('shortcutCopyTotp', 'Copy one-time code (TOTP)') },
    { keys: [`${cmd}${shift}U`], label: t('shortcutCopyUser', 'Copy username') },
    { keys: [`${cmd}${alt}R`], label: t('shortcutRestore', 'Restore item') },
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

// ---------- Watchtower overview panel (issue #376) ----------
let wtData = null;
let wtDrillKind = null;

function scoreBand(score) {
  const s = Math.max(0, Math.min(100, Math.round(score)));
  if (s >= 85) return { score: s, labelKey: 'watchtowerScoreExcellent', fallback: 'Excellent', color: '#22c55e' };
  if (s >= 70) return { score: s, labelKey: 'watchtowerScoreGood', fallback: 'Good', color: '#84cc16' };
  if (s >= 50) return { score: s, labelKey: 'watchtowerScoreFair', fallback: 'Fair', color: '#f59e0b' };
  return { score: s, labelKey: 'watchtowerScorePoor', fallback: 'Poor', color: '#ef4444' };
}

async function openWatchtower() {
  prevFocus = document.activeElement;
  const p = $('panel');
  p.textContent = '';
  p.classList.remove('hidden');
  p.classList.remove('shortcuts-panel');
  p.classList.add('wt-panel');
  p.setAttribute('role', 'dialog');
  p.setAttribute('aria-modal', 'true');
  p.setAttribute('aria-label', t('watchtower', 'Watchtower'));
  S.panel = 'watchtower';
  wtDrillKind = null;

  renderWatchtowerLoading(p);

  try {
    const res = await send({ type: 'popup:audit_summary' });
    wtData = res;
    renderWatchtowerOverview(p, res);
  } catch (err) {
    renderWatchtowerError(p, err, () => openWatchtower());
  }
}

function renderWatchtowerLoading(p) {
  p.textContent = '';
  p.setAttribute('aria-busy', 'true');
  p.setAttribute('aria-label', t('loadingWatchtower', 'Loading Watchtower…'));
  p.append(panelHead(t('watchtower', 'Watchtower')));
  const box = el('div', 'wt-skeleton-box');
  for (let i = 0; i < 4; i++) {
    const row = el('div', 'row skeleton-row skeleton-shimmer wt-skeleton-row');
    row.setAttribute('aria-hidden', 'true');
    const ic = el('span', 'skeleton-icon');
    const meta = el('div', 'skeleton-meta grow');
    meta.append(el('span', 'skeleton-bar t'), el('span', 'skeleton-bar u'));
    row.append(ic, meta);
    box.append(row);
  }
  p.append(box);
}

function renderWatchtowerError(p, err, onRetry) {
  p.textContent = '';
  p.removeAttribute('aria-busy');
  p.append(panelHead(t('watchtower', 'Watchtower')));
  const card = el('div', 'inline-err');
  card.setAttribute('role', 'alert');
  const ic = el('div', 'inline-err-ic');
  ic.innerHTML = UI.alert || UI.warn;
  const msg = el('h3', 'inline-err-msg', t('errWatchtowerLoad', "Couldn't load Watchtower"));
  const sub = el('p', 'inline-err-sub muted', typeof errText === 'function' ? errText(err) : (err && err.message) || String(err));
  const btn = el('button', 'btn btn-primary btn-retry', t('retry', 'Retry'));
  btn.type = 'button';
  btn.onclick = onRetry;
  card.append(ic, msg, sub, btn);
  p.append(card);
  btn.focus();
}

function renderWatchtowerOverview(p, data) {
  p.textContent = '';
  p.removeAttribute('aria-busy');
  wtDrillKind = null;

  p.append(panelHead(t('watchtower', 'Watchtower')));

  const findings = (data && Array.isArray(data.findings)) ? data.findings.filter((f) => f.items && f.items.length > 0) : [];

  if (!findings.length) {
    const emptyBox = el('div', 'empty wt-empty');
    const ic = el('div', 'wt-empty-icon');
    ic.innerHTML = UI.shield;
    const msg = el('p', 'wt-empty-msg', t('watchtowerNothingNeedsAttention', 'Nothing needs attention'));
    emptyBox.append(ic, msg);
    p.append(emptyBox);
    const closeBtn = p.querySelector('button');
    if (closeBtn) closeBtn.focus();
    return;
  }

  // Score summary card
  const scoreCard = el('div', 'wt-score-card');
  const b = scoreBand(data.score != null ? data.score : 100);
  const r = 26;
  const circ = 2 * Math.PI * r;
  const arc = circ * 0.75;
  const filled = (arc * b.score) / 100;

  const gauge = el('div', 'wt-score-gauge');
  gauge.setAttribute('role', 'img');
  gauge.setAttribute('aria-label', t('watchtowerScore', 'Security score: $1 of 100', [b.score]));
  gauge.innerHTML = `<svg viewBox="0 0 64 64" width="56" height="56" class="wt-gauge-svg">
    <circle cx="32" cy="32" r="${r}" fill="none" stroke="currentColor" class="wt-gauge-bg" stroke-width="5" stroke-dasharray="${arc} ${circ}" stroke-linecap="round"/>
    <circle cx="32" cy="32" r="${r}" fill="none" stroke="${b.color}" stroke-width="5" stroke-dasharray="${filled} ${circ}" stroke-linecap="round"/>
  </svg>
  <div class="wt-gauge-text">
    <span class="wt-gauge-num">${b.score}</span>
  </div>`;

  const scoreInfo = el('div', 'wt-score-info');
  const scoreLbl = el('div', 'wt-score-label', t(b.labelKey, b.fallback));
  scoreLbl.style.color = b.color;
  const scoreSub = el('div', 'wt-score-sub muted', t('watchtowerScore', 'Security score: $1 of 100', [b.score]));
  scoreInfo.append(scoreLbl, scoreSub);
  scoreCard.append(gauge, scoreInfo);
  p.append(scoreCard);

  // Finding rows
  const list = el('div', 'wt-findings-list');
  list.setAttribute('role', 'list');

  findings.forEach((f) => {
    const row = el('div', 'wt-finding-row');
    row.setAttribute('role', 'listitem');
    row.setAttribute('tabindex', '0');
    row.dataset.kind = f.kind;

    const sev = V ? V.findingSeverity(f.kind) : (f.kind === 'weak' || f.kind === 'reused' || f.kind === 'pwned' || f.kind === 'expired' ? 'danger' : 'warn');
    row.classList.add('sev-' + sev);

    const dot = el('span', 'wt-dot ' + sev);
    dot.setAttribute('aria-hidden', 'true');

    const kindLabel = V ? V.findingLabel(f.kind, null, t) : f.kind;
    const countStr = String(f.items.length);
    const rowLabel = el('span', 'wt-finding-label', kindLabel + ' · ' + countStr);

    const chev = el('span', 'wt-chev');
    chev.innerHTML = UI.chevron;
    chev.setAttribute('aria-hidden', 'true');

    row.append(dot, rowLabel, chev);

    const activate = () => renderWatchtowerDrillDown(p, f);
    row.onclick = activate;
    row.onkeydown = (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        activate();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        const next = row.nextElementSibling || list.firstElementChild;
        if (next) next.focus();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const prev = row.previousElementSibling || list.lastElementChild;
        if (prev) prev.focus();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        closePanel();
      }
    };
    list.append(row);
  });

  p.append(list);

  const firstRow = list.querySelector('.wt-finding-row');
  if (firstRow) firstRow.focus();
}

function renderWatchtowerDrillDown(p, finding) {
  p.textContent = '';
  wtDrillKind = finding.kind;

  // Header with back button
  const head = el('div', 'wt-drill-head');
  const backBtn = el('button', 'icon-btn wt-back-btn');
  backBtn.type = 'button';
  backBtn.title = t('back', 'Back');
  backBtn.setAttribute('aria-label', t('back', 'Back'));
  backBtn.innerHTML = UI.chevron;
  backBtn.onclick = () => {
    if (wtData) renderWatchtowerOverview(p, wtData);
    else openWatchtower();
  };

  const kindLabel = V ? V.findingLabel(finding.kind, null, t) : finding.kind;
  const title = el('h3', 'wt-drill-title', kindLabel + ' (' + finding.items.length + ')');
  const closeBtn = iconBtn(UI.close, t('close', 'Close'), closePanel);
  head.append(backBtn, title, closeBtn);
  p.append(head);

  const list = el('div', 'wt-items-list');
  list.setAttribute('role', 'list');

  finding.items.forEach((it) => {
    const row = el('div', 'wt-item-row');
    row.setAttribute('role', 'listitem');
    row.setAttribute('tabindex', '0');
    row.dataset.itemId = it.id;

    const iconWrap = el('span', 'wt-item-icon');
    if (typeof renderRowIcon === 'function') {
      renderRowIcon(iconWrap, it, 20);
    } else if (typeof categoryIcon === 'function') {
      iconWrap.innerHTML = categoryIcon(it.category || 'login', 20);
    }

    const meta = el('div', 'wt-item-meta');
    const tEl = el('span', 'wt-item-title', it.title || it.url || it.id);
    const subText = it.username || it.url || '';
    const uEl = el('span', 'wt-item-sub muted', subText);
    meta.append(tEl, uEl);

    row.append(iconWrap, meta);

    const activate = () => {
      openItemFromWatchtower(it.id);
    };

    row.onclick = activate;
    row.onkeydown = (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        activate();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        const next = row.nextElementSibling || list.firstElementChild;
        if (next) next.focus();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const prev = row.previousElementSibling || list.lastElementChild;
        if (prev) prev.focus();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        if (wtData) renderWatchtowerOverview(p, wtData);
        else openWatchtower();
      }
    };

    list.append(row);
  });

  p.append(list);

  const firstItem = list.querySelector('.wt-item-row');
  if (firstItem) firstItem.focus();
}

function openItemFromWatchtower(itemId) {
  closePanel();
  let idx = S.rows.findIndex((r) => r.id === itemId);
  if (idx < 0) {
    S.filter = 'all';
    renderFilter();
    applyFilter(false);
    idx = S.rows.findIndex((r) => r.id === itemId);
  }
  if (idx >= 0) {
    select(idx);
  } else {
    const it = S.all.find((r) => r.id === itemId) || { id: itemId, category: 'login', title: '' };
    S.sel = -1;
    renderDetailSkeleton(it);
    send({ type: 'popup:get_item', item_id: itemId })
      .then((res) => renderDetail(res.item))
      .catch((err) => renderDetailError(err, () => openItemFromWatchtower(itemId)));
  }
}

async function generate(len, digits, symbols, extra) {
  const msg = { type: 'popup:generate', len, digits, symbols };
  if (extra && extra.words) msg.words = true;
  if (extra && extra.pin) msg.pin = true;
  if (extra && extra.separator != null) msg.separator = extra.separator;
  if (extra && extra.capitalize != null) msg.capitalize = !!extra.capitalize;
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

  const rowSep = el('div', 'gen-row gen-sep-row');
  const sepLab = el('span', 'gen-sep-lab', t('genSeparator', 'Separator'));
  const sepGroup = el('div', 'gen-seps');
  sepGroup.setAttribute('role', 'radiogroup');
  sepGroup.setAttribute('aria-label', t('genSeparator', 'Separator'));
  const sepBtns = {};
  for (const sep of WORD_SEPARATORS) {
    const b = el('button', 'gen-sep', sep === ' ' ? '␣' : sep);
    b.type = 'button';
    b.setAttribute('role', 'radio');
    b.setAttribute('data-sep', sep);
    b.setAttribute('aria-label', sep === ' ' ? t('genSeparatorSpace', 'Space') : sep);
    b.onclick = () => setSeparator(sep);
    b.onkeydown = onSepKeyDown;
    sepGroup.append(b);
    sepBtns[sep] = b;
  }
  sepGroup.onkeydown = onSepKeyDown;
  rowSep.append(sepLab, sepGroup);
  p.append(rowSep);

  const rowCap = el('div', 'gen-row gen-toggles gen-cap-row');
  const lCap = el('label', 'tog');
  const togCap = Object.assign(el('input'), { type: 'checkbox' });
  lCap.append(togCap, el('span', null, t('genCapitalize', 'Capitalize')));
  rowCap.append(lCap);
  p.append(rowCap);

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
    const isMem = st.mode === 'memorable';
    rowSep.classList.toggle('hidden', !isMem);
    rowCap.classList.toggle('hidden', !isMem);
    for (const sep of WORD_SEPARATORS) {
      const on = sep === st.separator;
      const b = sepBtns[sep];
      b.classList.toggle('sel', on);
      b.setAttribute('aria-checked', String(on));
      b.tabIndex = on ? 0 : -1;
    }
    togCap.checked = !!st.capitalize;
  }
  function onSepKeyDown(e) {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft' && e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const dir = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : -1;
    const idx = WORD_SEPARATORS.indexOf(st.separator);
    const next = WORD_SEPARATORS[(idx + dir + WORD_SEPARATORS.length) % WORD_SEPARATORS.length];
    setSeparator(next);
    if (sepBtns[next] && typeof sepBtns[next].focus === 'function') sepBtns[next].focus();
  }
  function setSeparator(sep) {
    if (!WORD_SEPARATORS.includes(sep)) return;
    st.separator = sep;
    syncControls();
    persist();
    void run();
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
      const msg = {
        type: 'popup:generate',
        len: st.length,
        digits: st.mode === 'pin' ? false : st.digits,
        symbols: st.mode === 'pin' ? false : st.symbols,
        words: st.mode === 'memorable',
        pin: st.mode === 'pin',
      };
      if (st.mode === 'memorable') {
        msg.separator = st.separator;
        msg.capitalize = !!st.capitalize;
      }
      const r = await send(msg);
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
  togCap.onchange = () => { st.capitalize = togCap.checked; persist(); void run(); };
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
