'use strict';
// ---------- Skeletons & error cards ----------
let listSkeletonTimer = null;
let listLoading = false;

function renderListSkeletons() {
  stopListTotp();
  const list = $('list'), inner = $('listInner');
  if (!list || !inner) return;
  list.setAttribute('aria-busy', 'true');
  list.setAttribute('aria-label', t('loadingItems', 'Loading items…'));
  inner.textContent = '';
  inner.style.paddingTop = '0px';
  inner.style.paddingBottom = '0px';
  for (let i = 0; i < 6; i++) {
    const row = el('div', 'row skeleton-row skeleton-shimmer');
    row.setAttribute('aria-hidden', 'true');
    const ic = el('span', 'skeleton-icon');
    const meta = el('div', 'skeleton-meta');
    meta.append(el('span', 'skeleton-bar t'), el('span', 'skeleton-bar u'));
    row.append(ic, meta);
    inner.append(row);
  }
}

function startListLoading(delayMs = (V ? V.SKELETON_DELAY_MS : 150)) {
  listLoading = true;
  const list = $('list');
  if (list) list.setAttribute('aria-busy', 'true');
  clearTimeout(listSkeletonTimer);
  listSkeletonTimer = setTimeout(() => {
    if (listLoading) renderListSkeletons();
  }, delayMs);
}

function stopListLoading() {
  listLoading = false;
  clearTimeout(listSkeletonTimer);
  listSkeletonTimer = null;
  const list = $('list');
  if (list) {
    list.removeAttribute('aria-busy');
    list.removeAttribute('aria-label');
  }
}

function renderListError(err, onRetry) {
  stopListTotp();
  const list = $('list'), inner = $('listInner');
  if (!list || !inner) return;
  list.removeAttribute('aria-busy');
  list.removeAttribute('aria-label');
  inner.textContent = '';
  inner.style.paddingTop = '0px';
  inner.style.paddingBottom = '0px';
  const card = el('div', 'inline-err');
  card.setAttribute('role', 'alert');
  const ic = el('div', 'inline-err-ic');
  ic.innerHTML = UI.alert || UI.warn;
  const msg = el('h3', 'inline-err-msg', t('errListLoad', "Couldn't load items"));
  const sub = el('p', 'inline-err-sub muted', errText(err));
  const btn = el('button', 'btn btn-primary btn-retry', t('retry', 'Retry'));
  btn.type = 'button';
  btn.onclick = () => { onRetry(); };
  card.append(ic, msg, sub, btn);
  inner.append(card);
  btn.focus();
}

function renderDetailSkeleton(it) {
  const pane = $('detail');
  if (!pane) return;
  pane.textContent = '';
  pane.setAttribute('aria-busy', 'true');
  pane.setAttribute('aria-label', t('loadingDetail', 'Loading item details…'));

  const head = el('div', 'd-head skeleton-head skeleton-shimmer');
  head.setAttribute('aria-hidden', 'true');
  const ic = el('span', 'skeleton-icon lg');
  const g = el('div', 'skeleton-meta grow');
  g.append(el('span', 'skeleton-bar h'), el('span', 'skeleton-bar sub'));
  head.append(ic, g);
  pane.append(head);

  const box = el('div', 'fields skeleton-fields');
  box.setAttribute('aria-hidden', 'true');
  for (let i = 0; i < 3; i++) {
    const f = el('div', 'f skeleton-field skeleton-shimmer');
    const fg = el('div', 'grow');
    fg.append(el('span', 'skeleton-bar lbl'), el('span', 'skeleton-bar val'));
    f.append(fg);
    box.append(f);
  }
  pane.append(box);
}

function renderDetailError(err, onRetry) {
  const pane = $('detail');
  if (!pane) return;
  pane.removeAttribute('aria-busy');
  pane.removeAttribute('aria-label');
  pane.textContent = '';
  const card = el('div', 'inline-err');
  card.setAttribute('role', 'alert');
  const ic = el('div', 'inline-err-ic');
  ic.innerHTML = UI.alert || UI.warn;
  const msg = el('h3', 'inline-err-msg', t('errDetailLoad', "Couldn't load item details"));
  const sub = el('p', 'inline-err-sub muted', errText(err));
  const btn = el('button', 'btn btn-primary btn-retry', t('retry', 'Retry'));
  btn.type = 'button';
  btn.onclick = () => { onRetry(); };
  card.append(ic, msg, sub, btn);
  pane.append(card);
  btn.focus();
}
