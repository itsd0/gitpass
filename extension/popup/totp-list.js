'use strict';
// ---------- TOTP codes in the list ----------
// One shared timer for every visible row. Codes are fetched from the host per item (`popup:totp` returns the
// current code + seconds remaining only, never the secret) and cached until the period rolls over.
const MINI_RING_C = 2 * Math.PI * 5;

function totpRemaining(c) { return Math.max(0, Math.ceil((c.expiresAt - Date.now()) / 1000)); }
function chipsFor(id) { return [...document.querySelectorAll('#listInner [data-totp-id]')].filter((x) => x.dataset.totpId === id); }
function clearTotpCache() { S.totpCache = {}; S.totpFetching = {}; S.totpGen += 1; }

function totpRing() {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg'); svg.setAttribute('viewBox', '0 0 12 12'); svg.setAttribute('class', 'ring');
  const bg = document.createElementNS(ns, 'circle'), fg = document.createElementNS(ns, 'circle');
  for (const c of [bg, fg]) { c.setAttribute('cx', '6'); c.setAttribute('cy', '6'); c.setAttribute('r', '5'); }
  bg.setAttribute('class', 'bg'); fg.setAttribute('class', 'fg'); fg.setAttribute('stroke-dasharray', MINI_RING_C.toFixed(3));
  svg.append(bg, fg);
  return svg;
}

function paintTotpChip(chip, c) {
  const code = chip.querySelector('.totp-code'), svg = chip.querySelector('.ring');
  if (!code || !svg) return;
  const fg = svg.querySelector('.fg');
  chip.title = t('copyTotp', 'Copy TOTP');
  svg.classList.remove('hidden');
  paintRing(svg, fg, c ? totpRemaining(c) : 0, c && c.period);
  if (c && c.error) {
    code.textContent = '—'; code.classList.add('bad');
    chip.title = t('totpUnavailable', 'Code unavailable');
    svg.classList.add('hidden');
    return;
  }
  code.classList.remove('bad');
  code.textContent = c && c.code ? V.formatValue('totp', c.code) : '••• •••';
}

function paintRing(svg, fg, remaining, period) {
  fg.setAttribute('stroke-dashoffset', String(V.ringOffset(remaining, period, MINI_RING_C)));
  svg.classList.toggle('low', !!period && remaining <= 5);
}

function totpChip(it) {
  const b = el('button', 'totp-mini'); b.type = 'button'; b.dataset.totpId = it.id;
  b.setAttribute('aria-label', t('copyTotpCode', 'Copy one-time code'));
  b.append(el('span', 'totp-code'), totpRing());
  b.onclick = async (e) => {
    e.stopPropagation();
    if (b.disabled) return;
    b.disabled = true;
    try {
      await copyTotp(it.id);
    } catch (err) {
      toast(errText(err));
    } finally {
      b.disabled = false;
    }
  };
  paintTotpChip(b, S.totpCache[it.id]);
  return b;
}

async function fetchTotp(id) {
  const gen = S.totpGen;
  const r = await send({ type: 'popup:totp', item_id: id });
  const c = { code: r.code, period: r.period || 30, expiresAt: Date.now() + (Number(r.remaining) || 0) * 1000 };
  if (gen === S.totpGen) S.totpCache[id] = c; // a lock while in flight must not repopulate the cache
  return c;
}

async function copyTotp(id) {
  let c = S.totpCache[id];
  if (!c || c.error || totpRemaining(c) <= 0) c = await fetchTotp(id);
  for (const chip of chipsFor(id)) paintTotpChip(chip, c);
  await copy(c.code, t('copied', 'Copied'));
}

function startListTotp() { if (!S.listTotpTimer) { S.listTotpTimer = setInterval(tickListTotp, 1000); tickListTotp(); } }
function stopListTotp() { if (S.listTotpTimer) { clearInterval(S.listTotpTimer); S.listTotpTimer = null; } }

async function tickListTotp() {
  const gen = S.totpGen;
  const chips = [...document.querySelectorAll('#listInner [data-totp-id]')];
  if (!chips.length) return stopListTotp();
  for (const id of new Set(chips.map((x) => x.dataset.totpId))) {
    const c = S.totpCache[id];
    if (c && c.error) continue;
    if (c && totpRemaining(c) > 0) { for (const chip of chipsFor(id)) paintTotpChip(chip, c); continue; }
    if (S.totpFetching[id]) continue;
    S.totpFetching[id] = true;
    try {
      const fresh = await fetchTotp(id);
      if (gen !== S.totpGen) return stopListTotp();
      for (const chip of chipsFor(id)) paintTotpChip(chip, fresh);
    } catch (_) {
      if (gen !== S.totpGen) return stopListTotp();
      S.totpCache[id] = { error: true };
      for (const chip of chipsFor(id)) paintTotpChip(chip, S.totpCache[id]);
    } finally { delete S.totpFetching[id]; }
  }
}
