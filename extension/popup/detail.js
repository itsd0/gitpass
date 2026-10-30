'use strict';
// ---------- detail ----------
let detailSeq = 0;
let detailNavHistory = [];
let isNavigatingDetail = false;

async function openRelatedItem(r) {
  if (!r || !r.available) return;
  if (S.detail) {
    detailNavHistory.push(S.detail.id);
  }
  await navigateToItem(r.id);
}

async function goBackDetail() {
  if (!detailNavHistory.length) return;
  const prevId = detailNavHistory.pop();
  await navigateToItem(prevId, true);
}

async function navigateToItem(itemId, isBack = false) {
  isNavigatingDetail = true;
  try {
    let idx = S.rows.findIndex((row) => row.id === itemId || row.id.endsWith(':' + itemId));
    if (idx >= 0) {
      await select(idx);
      return;
    }
    const inAll = S.all && S.all.find((row) => row.id === itemId || row.id.endsWith(':' + itemId));
    if (inAll) {
      S.query = '';
      const qIn = $('q');
      if (qIn) qIn.value = '';
      S.filter = 'all';
      renderFilter();
      applyFilter(false);
      idx = S.rows.findIndex((row) => row.id === itemId || row.id.endsWith(':' + itemId));
      if (idx >= 0) {
        await select(idx);
        return;
      }
    }
    const res = await send({ type: 'popup:get_item', item_id: itemId });
    if (res && res.item) {
      S.sel = -1;
      renderList();
      S.detail = res.item;
      renderDetail(res.item);
    }
  } catch (e) {
    console.error('navigateToItem error:', e);
    if (typeof showErr === 'function') showErr(e);
  } finally {
    isNavigatingDetail = false;
  }
}

async function select(idx) {
  if (!isNavigatingDetail) {
    detailNavHistory = [];
  }
  const target = S.rows[idx];
  if (S.edit) {
    if (target && target.id === S.edit.id) return;
    if (!confirmDiscard()) {
      const keep = S.rows.findIndex((r) => r.id === S.edit.id);
      if (keep >= 0) S.sel = keep;
      renderList();
      return;
    }
    S.edit = null;
  }
  S.sel = idx; S.revealed = {}; stopTotp();
  const list = $('list'), qInput = $('q');
  if (idx >= 0) {
    list.scrollTop = V.scrollToRow(idx, list.scrollTop, list.clientHeight || 380, rowHeight());
    qInput && qInput.setAttribute('aria-activedescendant', 'row-' + idx);
    list.setAttribute('aria-activedescendant', 'row-' + idx);
  } else {
    qInput && qInput.removeAttribute('aria-activedescendant');
    list.removeAttribute('aria-activedescendant');
  }
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

  let d, findings;
  try {
    const findingsPromise = it.category === 'login'
      ? (S.findingsCache[it.id]
          ? Promise.resolve(S.findingsCache[it.id])
          : send({ type: 'popup:item_findings', item_id: it.id }).catch(() => null))
      : Promise.resolve({ kinds: [] });

    const [res, findingsRes] = await Promise.all([
      send({ type: 'popup:get_item', item_id: it.id, include_deleted: Boolean(it.deleted_at) }),
      findingsPromise,
    ]);
    d = res.item;
    findings = findingsRes || { kinds: [] };
    if (it.category === 'login' && V.cacheableFindings(findingsRes)) {
      S.findingsCache[it.id] = findingsRes;
    }
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
  d.findings = findings;
  S.detail = d;
  renderDetail(d);
}

function renderDetail(d) {
  const pane = $('detail'); pane.textContent = '';
  const head = el('div', 'd-head');
  if (detailNavHistory.length > 0) {
    const backBtn = el('button', 'icon-btn back-item-btn');
    backBtn.type = 'button';
    backBtn.title = t('back', 'Back');
    backBtn.setAttribute('aria-label', t('back', 'Back'));
    backBtn.innerHTML = '<svg class="ui" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 12H5M12 19l-7-7 7-7"/></svg>';
    backBtn.onclick = () => goBackDetail();
    head.append(backBtn);
  }
  const ic = el('span', 'ic'); renderRowIcon(ic, d, 28);
  const g = el('div', 'grow'); g.append(el('h2', null, d.title), el('div', 'sub', d.vault_name + ' · ' + t(V.CAT_KEY[d.category], V.CAT_LABEL[d.category])));
  head.append(ic, g, moreMenu(d));
  pane.append(head);

  if (d.deleted_at) {
    const banner = el('div', 'trash-banner');
    const bannerMsg = el('div', null, t('inTrash', 'This item is in Recently Deleted.'));
    const trashSub = V.formatTrashSubtitle(d.deleted_at, Date.now(), t);
    const bannerSub = el('div', 'trash-sub' + (trashSub && trashSub.danger ? ' danger' : ''), trashSub ? trashSub.text : '');
    banner.append(bannerMsg, bannerSub);
    pane.append(banner);
  } else if (d.archived) {
    const banner = el('div', 'trash-banner');
    const bannerMsg = el('div', null, t('inArchive', 'This item is archived.'));
    banner.append(bannerMsg);
    pane.append(banner);
  }

  if (d.category === 'login' && V.hasWatchtowerFindings(d.findings)) {
    const strip = el('div', 'warning-strip');
    strip.setAttribute('role', 'status');
    strip.setAttribute('aria-label', t('watchtowerWarnings', 'Watchtower warnings'));

    const chips = el('div', 'warning-chips');
    chips.setAttribute('role', 'list');
    for (const k of d.findings.kinds) {
      const sev = V.findingSeverity(k);
      const chip = el('span', 'warning-chip ' + sev, V.findingLabel(k, d.findings, t));
      chip.setAttribute('role', 'listitem');
      chip.setAttribute('tabindex', '0');
      chips.append(chip);
    }
    strip.append(chips);

    const warnActs = el('div', 'warning-actions');
    const genBtn = el('button', 'btn btn-primary warn-action', t('generateAndUpdate', 'Generate & update'));
    genBtn.type = 'button';
    genBtn.onclick = () => openGenerator({ updateItem: d });
    warnActs.append(genBtn);

    if (d.findings.change_password_url) {
      const siteBtn = el('button', 'btn warn-action', t('changeOnSite', 'Change on site'));
      siteBtn.type = 'button';
      siteBtn.onclick = () => {
        if (globalThis.chrome && chrome.tabs && chrome.tabs.create) {
          chrome.tabs.create({ url: d.findings.change_password_url, active: true });
        } else {
          window.open(d.findings.change_password_url, '_blank', 'noopener');
        }
      };
      warnActs.append(siteBtn);
    }

    const wtBtn = el('button', 'btn btn-link warn-action', t('openWatchtower', 'Open Watchtower'));
    wtBtn.type = 'button';
    wtBtn.onclick = () => {
      send({ type: 'popup:open_in_app', view: 'watchtower' }).catch(showErr);
    };
    warnActs.append(wtBtn);

    const appBtn = el('button', 'btn btn-link warn-action', t('openInGitpass', 'Open in gitpass'));
    appBtn.type = 'button';
    appBtn.onclick = () => {
      send({ type: 'popup:open_in_app', item_id: d.id }).catch(showErr);
    };
    warnActs.append(appBtn);

    strip.append(warnActs);
    pane.append(strip);
  }
  const acts = el('div', 'd-actions');
  if (d.deleted_at) {
    const restoreBtn = el('button', 'btn btn-primary', t('restore', 'Restore'));
    restoreBtn.type = 'button';
    restoreBtn.onclick = () => restoreItem(d);
    acts.append(restoreBtn);
  } else {
    if (d.archived) {
      const unarchiveBtn = el('button', 'btn btn-primary', t('unarchive', 'Unarchive'));
      unarchiveBtn.type = 'button';
      unarchiveBtn.onclick = () => unarchiveItem(d);
      acts.append(unarchiveBtn);
    }
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
  if (d.category === 'login') {
    const edit = el('button', 'btn', t('edit', 'Edit'));
    edit.type = 'button';
    edit.dataset.action = 'edit';
    edit.title = t('shortcutEdit', 'Edit login') + ' (⌘E)';
    edit.onclick = () => startEdit();
    acts.append(edit);
  }
  }
  if (acts.children.length) pane.append(acts);
  const box = el('div', 'fields');
  for (const f of d.fields || []) box.append(fieldRow(d, f));
  if (box.children.length) pane.append(box);
  if (d.notes) pane.append(el('div', 'notes', d.notes));
  if (d.related_items && d.related_items.length) {
    const relSec = el('div', 'd-related');
    relSec.setAttribute('role', 'region');
    relSec.setAttribute('aria-label', t('relatedItems', 'Related items'));
    const relTitle = el('div', 'd-related-title', t('relatedItems', 'Related items'));
    const relList = el('div', 'd-related-list');
    relList.setAttribute('role', 'list');
    for (const r of d.related_items) {
      const isAvail = Boolean(r.available);
      const row = el('div', 'related-row' + (isAvail ? '' : ' unavailable'));
      row.setAttribute('role', 'listitem');
      row.setAttribute('tabindex', isAvail ? '0' : '-1');
      row.dataset.itemId = r.id;

      const ic = el('span', 'related-ic');
      if (isAvail && r.category) {
        ic.innerHTML = categoryIcon(r.category, 20);
      } else {
        ic.innerHTML = UI.link;
      }

      const tEl = el('span', 'related-title', isAvail ? r.title : t('itemNotAvailable', 'Item not available'));
      tEl.title = isAvail ? r.title : t('itemNotAvailable', 'Item not available');
      row.append(ic, tEl);

      if (isAvail) {
        row.onclick = () => openRelatedItem(r);
      }
      row.onkeydown = (e) => {
        if (e.key === 'Enter') {
          if (isAvail) {
            e.preventDefault();
            e.stopPropagation();
            openRelatedItem(r);
          }
        } else if (e.key === 'ArrowDown') {
          e.preventDefault();
          e.stopPropagation();
          const next = row.nextElementSibling;
          if (next && next.focus) next.focus();
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          e.stopPropagation();
          const prev = row.previousElementSibling;
          if (prev && prev.focus) prev.focus();
        }
      };
      relList.append(row);
    }
    relSec.append(relTitle, relList);
    pane.append(relSec);
  }
  if (d.attachments && d.attachments.length) {
    const attSec = el('div', 'd-attachments');
    attSec.setAttribute('role', 'region');
    attSec.setAttribute('aria-label', t('attachments', 'Attachments'));
    const attTitle = el('div', 'd-attachments-title', t('attachments', 'Attachments'));
    const attList = el('div', 'd-attachments-list');
    attList.setAttribute('role', 'list');
    for (const a of d.attachments) {
      const row = el('div', 'att-row');
      row.setAttribute('role', 'listitem');
      row.setAttribute('tabindex', '0');

      const ic = el('span', 'att-icon');
      ic.innerHTML = GitpassPopupIcons.fileIcon ? GitpassPopupIcons.fileIcon(a.name, a.mime) : UI.paperclip;

      const info = el('div', 'att-info');
      const nameEl = el('span', 'att-name', a.name);
      nameEl.title = a.name;
      const sizeEl = el('span', 'att-size', V.formatSize ? V.formatSize(a.size) : a.size + ' B');
      info.append(nameEl, sizeEl);

      const openBtn = el('button', 'btn att-open-btn');
      openBtn.type = 'button';
      openBtn.title = t('openInGitpass', 'Open in gitpass');
      openBtn.setAttribute('aria-label', t('openAttachmentInGitpass', 'Open $1 in gitpass', [a.name]));
      openBtn.innerHTML = UI.external + ' <span>' + t('openInApp', 'Open in gitpass') + '</span>';
      openBtn.onclick = (e) => {
        e.stopPropagation();
        send({ type: 'popup:open_in_app', item_id: d.id }).catch(showErr);
      };
      row.onkeydown = (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          send({ type: 'popup:open_in_app', item_id: d.id }).catch(showErr);
        }
      };
      row.append(ic, info, openBtn);
      attList.append(row);
    }
    attSec.append(attTitle, attList);
    pane.append(attSec);
  }
  if (d.tags && d.tags.length) {
    const tg = el('div', 'tags');
    for (const x of d.tags) {
      const pill = el('button', 'tag', x); pill.type = 'button';
      pill.title = t('filterByTag', 'Show items with this tag');
      pill.setAttribute('aria-label', t('filterByTag', 'Show items with this tag') + ': ' + x);
      pill.onclick = () => { S.filter = 'tag:' + x; renderFilter(); applyFilter(true); };
      tg.append(pill);
    }
    pane.append(tg);
  }
  if (d.updated_at) pane.append(el('div', 'meta-line', t('lastEdited', 'Last edited') + ' ' + new Date(d.updated_at).toLocaleString()));
}
