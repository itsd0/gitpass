// ---------- detection state ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

let forms = [];
const badges = new Map(); // field -> { host, btn, role }
const bound = new WeakSet();
const decoratedInputs = new WeakSet();

function pruneDetachedBadges() {
  for (const [field, b] of badges) {
    if (!field.isConnected) {
      if (b.host) b.host.remove();
      badges.delete(field);
      try { delete field.dataset.gitpassDecorated; } catch (_) {}
    }
  }
}

function removeBadge(field) {
  const b = badges.get(field);
  if (b) {
    if (b.host) b.host.remove();
    badges.delete(field);
  }
  try { delete field.dataset.gitpassDecorated; } catch (_) {}
}

function hasOverlappingBadge(field, role) {
  const r1 = field.getBoundingClientRect ? field.getBoundingClientRect() : { left: 0, top: 0, width: 0, height: 0 };
  if (!r1.width || !r1.height) return false;
  for (const [existingField, b] of badges) {
    if (existingField === field || !existingField.isConnected) continue;
    const sameContainer = (field.form && existingField.form && field.form === existingField.form) ||
                          (field.parentElement && existingField.parentElement && field.parentElement === existingField.parentElement);
    if (!sameContainer) continue;
    if (b.role !== role) continue;
    const r2 = existingField.getBoundingClientRect ? existingField.getBoundingClientRect() : { left: 0, top: 0, width: 0, height: 0 };
    if (!r2.width || !r2.height) continue;
    const dx = Math.abs(r1.left - r2.left);
    const dy = Math.abs(r1.top - r2.top);
    if (dx < 4 && dy < 4) return true;
  }
  return false;
}

function rescan() {
  pruneDetachedBadges();
  if (!SETTINGS.autofill) { // in-page suggestions turned off: drop badges/dropdown, keep capture (governed by savePrompts)
    closeDropdown();
    for (const field of Array.from(badges.keys())) removeBadge(field);
    return;
  }
  forms = GitpassForms.detectForms(document);
  const live = new Set();
  for (const f of forms) {
    for (const { el: field, role } of GitpassForms.anchorFields(f)) {
      if (!field || !field.isConnected) continue;
      if (hasOverlappingBadge(field, role) && !badges.has(field)) continue;
      live.add(field);
      if (!badges.has(field)) addBadge(field, role);
      if (!bound.has(field)) { bound.add(field); bindField(field); }
    }
  }
  for (const [field] of badges) {
    if (!live.has(field) || !field.isConnected) removeBadge(field);
  }
  placeAll();
  maybeOfferTotp();
}
function ctxFor(field) { return GitpassForms.locate(forms, field); }

function addBadge(field, role) {
  if (badges.has(field)) return;
  decoratedInputs.add(field);
  try { field.dataset.gitpassDecorated = 'true'; } catch (_) {}
  const { host, root } = shadowHost();
  const btn = el('button', 'badge'); btn.type = 'button'; btn.tabIndex = -1;
  btn.title = t('csFillWith', 'Fill with gitpass');
  const img = el('img'); img.src = LOGO; img.alt = 'gitpass'; btn.append(img); root.append(btn);
  btn.addEventListener('mousedown', (e) => e.preventDefault()); // keep focus in the field
  btn.addEventListener('click', (e) => { if (!e.isTrusted) return; e.preventDefault(); e.stopPropagation(); field.focus(); openDropdown(field); });
  badges.set(field, { host, btn, role });
}
function placeAll() {
  pruneDetachedBadges();
  for (const [field, b] of badges) {
    const r = field.getBoundingClientRect();
    if (!r.width || !r.height) { b.host.style.display = 'none'; continue; }
    b.host.style.display = '';
    b.host.style.left = (r.right + window.scrollX - 28) + 'px';
    b.host.style.top = (r.top + window.scrollY + (r.height - 22) / 2) + 'px';
  }
  if (dd) placeDropdown();
  if (totpChip) placeTotpChip();
}

function bindField(field) {
  field.addEventListener('focus', (e) => { if (e.isTrusted && !(totpChip && totpChip.field === field)) openDropdown(field); }); // the code chip stands in for the menu
  field.addEventListener('blur', () => setTimeout(() => {
    if (dd && dd.field === field && !dd.hovered && !dd.root.activeElement && dd.mode === 'suggested') closeDropdown();
  }, 150));
  field.addEventListener('keydown', onFieldKey, true);
}
