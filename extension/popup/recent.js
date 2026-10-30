'use strict';
// ---------- Recently used ----------
function renderRecent() {
  const sec = $('recentSection');
  const rList = $('recentList');
  if (!sec || !rList) return;
  const shouldShow = !S.query && (!S.suggested || !S.suggested.length) && (S.filter === 'all' || S.filter === 'suggested');
  if (!shouldShow) {
    sec.classList.add('hidden');
    rList.textContent = '';
    return;
  }
  const recent = V && V.getRecentlyUsed ? V.getRecentlyUsed(S.all, 5) : [];
  if (!recent.length) {
    sec.classList.add('hidden');
    rList.textContent = '';
    return;
  }
  sec.classList.remove('hidden');
  rList.textContent = '';
  const currentSelectedId = S.rows[S.sel] && S.rows[S.sel].id;
  for (let i = 0; i < recent.length; i++) {
    const it = recent[i];
    const isSel = it.id === currentSelectedId;
    const row = el('div', 'row' + (isSel ? ' sel' : ''));
    row.id = 'recent-row-' + i;
    row.setAttribute('role', 'option');
    row.setAttribute('aria-selected', isSel ? 'true' : 'false');
    row.setAttribute('tabindex', '-1');
    row.dataset.itemId = it.id;
    const ic = el('span', 'ic');
    renderRowIcon(ic, it, 20);
    const meta = el('div', 'meta');
    const tDiv = el('div', 't');
    tDiv.textContent = it.title;
    const uDiv = el('div', 'u');
    uDiv.textContent = V ? V.rowSubtitle(it, '') : (it.subtitle || '—');
    meta.append(tDiv, uDiv);
    row.append(ic, meta);
    if (it.favorite) {
      const f = el('span', 'fav');
      f.innerHTML = UI.star;
      row.append(f);
    }
    if (it.has_attachments) {
      const att = el('span', 'att');
      att.innerHTML = UI.paperclip;
      att.setAttribute('role', 'img');
      att.setAttribute('aria-label', t('hasAttachments', 'Has attachments'));
      row.append(att);
    }
    if (it.has_totp) row.append(totpChip(it));
    row.onclick = () => {
      const idx = S.rows.findIndex((r) => r.id === it.id);
      if (idx >= 0) select(idx);
    };
    row.ondblclick = () => { if (!it.deleted_at) openAndFill(); };
    rList.append(row);
  }
}
