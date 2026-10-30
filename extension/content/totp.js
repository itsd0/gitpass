// ---------- one-time code on the next page (#181) ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

// After gitpass fills a login that has a one-time code, the background remembers the item id (2 min, same
// registrable domain, no secrets). A code field on the next page then gets a chip under it; the code is
// only fetched when the user presses it (or right away with the opt-in "Fill one-time codes automatically").
let totpChip = null; // { host, ui, field }
const totpAsked = new WeakSet();
function closeTotpChip() { if (totpChip) { totpChip.ui.close(); totpChip.host.remove(); totpChip = null; } }
function placeTotpChip() {
  const r = totpChip.field.getBoundingClientRect();
  if (!totpChip.field.isConnected || !r.width || !r.height || (dd && dd.field === totpChip.field)) { totpChip.host.style.display = 'none'; return; }
  totpChip.host.style.display = '';
  totpChip.host.style.left = Math.max(4, Math.min(r.left + scrollX, scrollX + innerWidth - 308)) + 'px';
  totpChip.host.style.top = (r.bottom + scrollY + 4) + 'px';
}
function maybeOfferTotp() {
  if (typeof GitpassTotpChip === 'undefined' || totpChip) return;
  for (const f of forms) {
    const field = f.kind === 'totp' && f.fields.totp;
    if (!field || !field.isConnected || totpAsked.has(field)) continue;
    totpAsked.add(field); // one ask per code field per page load
    send('page:totp_offer').then((r) => { if (originOk(r) && r.offer && field.isConnected) showTotpChip(f, field, r.offer); }).catch(() => {});
    return;
  }
}
async function fillOffered(form, offer) {
  const r = await send('page:totp', { item_id: offer.item_id });
  if (originOk(r)) GitpassForms.fillTotp(freshForm(form), r.code);
}
function showTotpChip(form, field, offer) {
  if (!SETTINGS.autofill || totpChip) return;
  if (SETTINGS.autoFillTotp) { fillOffered(form, offer).catch(() => {}); return; }
  const { host, root } = shadowHost();
  host.style.zIndex = '2147483647';
  const ui = GitpassTotpChip.build(document, {
    title: offer.title || '', label: t('csTotpOffer', 'Fill one-time code'), dismissLabel: t('csTotpDismiss', 'Dismiss'), logo: LOGO,
    ttlMs: Math.max(1000, Number(offer.expires_in_ms) || 0),
    onFill: () => { closeTotpChip(); fillOffered(form, offer).catch(() => {}); },
    onDismiss: closeTotpChip,
  });
  root.append(ui.el);
  totpChip = { host, ui, field };
  if (dd && dd.field === field) closeDropdown();
  placeTotpChip();
  // Esc on the field dismisses; Tab moves into the chip (it lives at the end of the document), Shift+Tab back.
  field.addEventListener('keydown', (e) => {
    if (!e.isTrusted || !totpChip || totpChip.field !== field || dd) return;
    if (e.key === 'Escape') { e.preventDefault(); closeTotpChip(); }
    else if (e.key === 'Tab' && !e.shiftKey) { e.preventDefault(); ui.fill.focus(); }
  }, true);
  ui.el.addEventListener('keydown', (e) => { if (e.isTrusted && e.key === 'Tab' && e.shiftKey && root.activeElement === ui.fill) { e.preventDefault(); field.focus(); } });
}
