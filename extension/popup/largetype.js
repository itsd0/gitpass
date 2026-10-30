'use strict';
// ---------- Large type (issue #278) ----------
// Desktop parity: one character per cell with its position under it; digits use the orange accent,
// symbols are blue and a space shows as ␣. The value comes from secretValue() (reveal/totp path),
// is never stored, and is removed from the DOM when the view closes or the popup locks.
const LARGE_TYPE_GROUP = 8;
let largeTypeTrigger = null;

function charKind(ch) {
  if (/\d/.test(ch)) return 'digit';
  if (/[a-zA-Z]/.test(ch)) return 'letter';
  if (/\s/.test(ch)) return 'space';
  return 'symbol';
}

function closeLargeType(restoreFocus = true) {
  const box = $('largetype');
  if (!box) return;
  const trigger = largeTypeTrigger;
  largeTypeTrigger = null;
  const wasOpen = !box.classList.contains('hidden');
  box.classList.add('hidden');
  box.removeAttribute('aria-label');
  box.textContent = ''; // never leave the secret in the DOM
  if (!restoreFocus || !wasOpen) return;
  if (trigger && trigger.isConnected && typeof trigger.focus === 'function') trigger.focus();
  else {
    const q = $('q');
    if (q && !$('unlocked').classList.contains('hidden')) q.focus();
  }
}

function largeTypeCells(value) {
  const chars = Array.from(String(value || ''));
  const grid = el('div', 'largetype-grid');
  grid.setAttribute('data-large-grid', '');
  for (let i = 0; i < chars.length; i += LARGE_TYPE_GROUP) {
    const group = el('div', 'largetype-group');
    group.setAttribute('data-large-group', '');
    chars.slice(i, i + LARGE_TYPE_GROUP).forEach((ch, offset) => {
      const pos = i + offset + 1;
      const cell = el('div', 'largetype-char');
      cell.setAttribute('data-large-char', '');
      cell.dataset.kind = charKind(ch);
      cell.dataset.position = String(pos);
      cell.append(el('span', 'largetype-glyph', ch === ' ' ? '␣' : ch), el('span', 'largetype-pos', String(pos)));
      group.append(cell);
    });
    grid.append(group);
  }
  return grid;
}

async function openLargeType(d, f, trigger) {
  if (!d || !f) return;
  const box = $('largetype');
  if (!box) return;
  let value;
  try { value = await secretValue(d, f); } catch (e) { toast(errText(e)); return; }
  if (!value) { toast(t('largeTypeEmpty', 'Nothing to show in large type')); return; }
  closeLargeType(false);
  largeTypeTrigger = trigger || null;
  box.textContent = '';
  box.setAttribute('aria-label', t('largeTypeView', 'Large type view'));

  const card = el('div', 'largetype-card');
  const head = el('div', 'largetype-head');
  const close = el('button', 'icon-btn');
  close.type = 'button';
  close.dataset.action = 'largetype-close';
  close.innerHTML = UI.close;
  close.setAttribute('aria-label', t('close', 'Close'));
  close.title = t('close', 'Close');
  close.onclick = () => closeLargeType();
  head.append(el('h3', null, f.label || t('largeType', 'Large type')), close);
  card.append(head, largeTypeCells(value));
  box.append(card);
  box.onclick = (e) => { if (e.target === box) closeLargeType(); };
  box.classList.remove('hidden');
  close.focus();
}

/** ⌘⌥L / Ctrl+Alt+L on the selected item (same shortcut as the desktop): its primary secret field. */
function openLargeTypeForSelected() {
  const d = S.detail;
  const f = d && V.primarySecretField(d);
  if (!f) { toast(t('largeTypeNone', 'No password or one-time code to show')); return; }
  let trigger = null;
  try {
    const name = globalThis.CSS && CSS.escape ? CSS.escape(f.name) : f.name;
    trigger = document.querySelector('#detail [data-action="large-type"][data-field="' + name + '"]');
  } catch (_) {}
  return openLargeType(d, f, trigger);
}

async function trashItem(d) {
  const idx = Math.max(0, S.sel);
  await send({ type: 'popup:trash_item', item_id: d.id });
  S.all = (S.all || []).filter((x) => x.id !== d.id);
  S.suggested = (S.suggested || []).filter((x) => x.id !== d.id);
  delete S.findingsCache[d.id];
  S.detail = null;
  S.revealed = {};
  stopTotp();
  S.sel = -1;
  applyFilter(false);
  if (S.rows.length) await select(Math.min(idx, S.rows.length - 1));
  else await select(-1);
  toastAction(t('movedToTrash', 'Moved to Recently Deleted'), t('undo', 'Undo'), async () => {
    await send({ type: 'popup:restore_item', item_id: d.id });
    toast(t('itemRestored', 'Item restored'));
    S.findingsCache = {};
    await loadData();
    const i = S.rows.findIndex((r) => r.id === d.id);
    if (i >= 0) await select(i);
  });
}

function moreMenu(d) {
  const w = el('div', 'menu-wrap');
  const m = el('div', 'menu hidden');
  m.setAttribute('role', 'menu');
  function closeMenu(restoreFocus = false) {
    m.classList.add('hidden');
    trigger.setAttribute('aria-expanded', 'false');
    if (restoreFocus && trigger.isConnected) trigger.focus();
  }
  function openMenu() {
    m.classList.remove('hidden');
    trigger.setAttribute('aria-expanded', 'true');
    const first = m.querySelector('button');
    if (first) first.focus();
  }
  const item = (svg, label, fn, opts) => {
    const b = el('button');
    b.type = 'button';
    b.setAttribute('role', 'menuitem');
    if (opts && opts.danger) b.classList.add('danger');
    b.innerHTML = svg;
    b.append(document.createTextNode(label));
    b.onclick = (e) => { e.stopPropagation(); closeMenu(true); Promise.resolve(fn()).catch(showErr); };
    m.append(b);
  };
  item(UI.external, t('openInApp', 'Open in gitpass'), () => send({ type: 'popup:open_in_app', item_id: d.id }));
  item(UI.link, t('copyLink', 'Copy link'), () => copy('gitpass://item/' + encodeURIComponent(d.id), t('linkCopied', 'Link copied')));
  item(UI.star, d.favorite ? t('unfavorite', 'Remove from Favorites') : t('favorite', 'Add to Favorites'), async () => {
    await send({ type: 'popup:set_favorite', item_id: d.id, favorite: !d.favorite });
    d.favorite = !d.favorite; const s = S.all.find((x) => x.id === d.id); if (s) s.favorite = d.favorite; applyFilter(false);
  });
  const sep = el('div', 'menu-sep');
  sep.setAttribute('role', 'separator');
  m.append(sep);
  item(UI.trash, t('moveToTrash', 'Move to Recently Deleted'), () => trashItem(d), { danger: true });
  const trigger = iconBtn(UI.more, t('more', 'More'), () => (m.classList.contains('hidden') ? openMenu() : closeMenu()));
  trigger.setAttribute('aria-haspopup', 'menu');
  trigger.setAttribute('aria-expanded', 'false');
  m.onkeydown = (e) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeMenu(true); return; }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopPropagation();
      const items = [...m.querySelectorAll('button')];
      const i = items.indexOf(document.activeElement);
      const n = e.key === 'ArrowDown' ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
      if (items[n]) items[n].focus();
    } else if (e.key === 'Tab') closeMenu();
  };
  document.addEventListener('click', (e) => { if (!w.contains(e.target)) closeMenu(); });
  w.append(trigger, m);
  return w;
}

async function openAndFill() {
  const d = S.detail; if (!d) return;
  const kind = V.fillKind(d.category); if (!kind) return;
  await send({ type: 'popup:open_fill', item_id: d.id, kind, url: d.url || null }).then(() => window.close(), showErr);
}

async function copyPrimary() {
  const d = S.detail; const f = V.primarySecretField(d); if (!f) return;
  await copy(await secretValue(d, f), f.kind === 'password' ? t('passwordCopied', 'Password copied') : t('copied', 'Copied'));
}

async function copyUsername() {
  const selected = S.rows[S.sel];
  if (!selected) return;
  const user = V.itemUsername(S.detail, selected);
  if (user) {
    await copy(user, t('usernameCopied', 'Username copied'));
  } else {
    toast(t('noUsername', 'No username'));
  }
}

function openWebsite() {
  const selected = S.rows[S.sel];
  if (!selected) return;
  const url = (S.detail && S.detail.url) || selected.url || '';
  if (url && V.isAllowedUrl(url)) {
    if (typeof chrome !== 'undefined' && chrome.tabs && typeof chrome.tabs.create === 'function') {
      chrome.tabs.create({ url, active: true });
    } else {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  }
}
