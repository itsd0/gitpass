/* global chrome, GitpassI18n, GitpassIcons */
'use strict';
const $ = (id) => document.getElementById(id);
const t = (k, f) => GitpassI18n.t(k, f);
// `window.__gitpassMock` lets the preview page render without Chrome APIs.
function send(msg) {
  if (window.__gitpassMock) return window.__gitpassMock(msg);
  return new Promise((res, rej) => chrome.runtime.sendMessage(msg, (r) => {
    if (chrome.runtime.lastError) return rej(new Error(chrome.runtime.lastError.message));
    r && r.ok ? res(r.result) : rej(new Error((r && r.error) || 'error'));
  }));
}
const ICON = {
  copy: '<svg viewBox="0 0 24 24"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/></svg>',
  clock: '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="8"/><path d="M12 8v4l3 2"/></svg>',
};
function show(view) { for (const v of ['nohost', 'locked', 'unlocked']) $(v).classList.toggle('hidden', v !== view); }
function setErr(id, e) { $(id).textContent = e ? String(e.message || e) : ''; }
let toastT = null;
function toast(msg) { const el = $('toast'); el.textContent = msg; el.classList.remove('hidden'); clearTimeout(toastT); toastT = setTimeout(() => el.classList.add('hidden'), 1600); }
async function copy(text, label) { await navigator.clipboard.writeText(text); toast(label || t('copied', 'Copied')); }
function isHostError(e) { return /native messaging host|not found|forbidden|disconnected|not allowed/i.test(String(e && e.message)); }

function button(cls, html, title, onclick) {
  const b = document.createElement('button'); b.className = 'act ' + cls; b.innerHTML = html; b.title = title; b.type = 'button';
  b.onclick = async (e) => { e.stopPropagation(); try { await onclick(b); } catch (err) { setErr('err', err); } };
  return b;
}
function renderItems(el, items, canFill, emptyMsg) {
  el.textContent = '';
  if (!items.length) { const d = document.createElement('div'); d.className = 'empty'; d.textContent = emptyMsg; el.append(d); return; }
  for (const it of items) {
    const row = document.createElement('div'); row.className = 'row';
    const ic = document.createElement('span'); ic.innerHTML = GitpassIcons.categoryIcon(it.category || 'login', 30);
    const meta = document.createElement('div'); meta.className = 'meta';
    const ti = document.createElement('div'); ti.className = 't'; ti.textContent = it.title;
    const un = document.createElement('div'); un.className = 'u'; un.textContent = it.username || '—';
    meta.append(ti, un);
    const acts = document.createElement('div'); acts.className = 'acts';
    if (canFill) {
      acts.append(button('fill', t('fill', 'Fill'), t('fillTitle', 'Fill on this page'), async () => { await send({ type: 'fillActive', item_id: it.id }); window.close(); }));
      acts.append(button('', ICON.copy, t('copyPassword', 'Copy password'), async () => { const r = await send({ type: 'copyActive', item_id: it.id }); await copy(r.password, t('passwordCopied', 'Password copied')); }));
    }
    if (it.has_totp) {
      acts.append(button('totp', ICON.clock, t('copyTotp', 'Copy TOTP'), async (b) => {
        const r = await send({ type: 'totp', item_id: it.id }); await copy(r.code, t('totpCopied', 'TOTP copied'));
        let left = r.remaining; b.textContent = r.code.slice(0, 3) + ' ' + r.code.slice(3) + ' · ' + left + 's';
        clearInterval(b._t); b._t = setInterval(() => { left -= 1; if (left <= 0) { clearInterval(b._t); b.innerHTML = ICON.clock; } else b.textContent = r.code.slice(0, 3) + ' ' + r.code.slice(3) + ' · ' + left + 's'; }, 1000);
      }));
    }
    row.append(ic, meta, acts); el.append(row);
  }
}

async function refresh() {
  setErr('err'); setErr('unlockErr');
  let st;
  try { st = await send({ type: 'status' }); } catch (e) {
    $('hostErr').textContent = String(e.message || e);
    $('hostCmd').textContent = 'cd ~/gitpass && cargo build --release -p gitpass-host\n"$HOME/Library/Application Support/gitpass/bin/gitpass-host" install --browser chrome --extension-id ' + (globalThis.chrome && chrome.runtime && chrome.runtime.id || '<id>') + ' --vault ~/.gitpass';
    show('nohost'); return;
  }
  if (!st.unlocked) { $('skRow').classList.toggle('hidden', !!st.secret_key_file); show('locked'); $('pw').focus(); return; }
  show('unlocked');
  try {
    const m = await send({ type: 'matchActive' });
    $('origin').textContent = m.origin ? m.origin.replace(/^https:\/\//, '') : t('notHttps', 'not an https page');
    renderItems($('matches'), m.items, !!m.origin, m.origin ? t('noMatches', 'No saved logins for this site') : t('notFillable', 'Open an https page to fill'));
  } catch (e) { setErr('err', e); }
  $('q').focus();
}

// Eye toggles
document.querySelectorAll('.eye').forEach((b) => b.addEventListener('click', () => {
  const i = $(b.dataset.for); const on = i.type === 'password'; i.type = on ? 'text' : 'password'; b.classList.toggle('on', on);
}));

$('unlockForm').onsubmit = async (e) => {
  e.preventDefault();
  const btn = $('unlockBtn'); if (btn.disabled) return;
  const msg = { type: 'unlock', password: $('pw').value };
  const sk = $('sk').value.trim(); if (sk) msg.secret_key = sk;
  if (!msg.password) { setErr('unlockErr', t('pwRequired', 'Enter your master password')); return; }
  btn.disabled = true; btn.querySelector('.spinner').classList.remove('hidden'); setErr('unlockErr');
  try { await send(msg); $('pw').value = ''; $('sk').value = ''; await refresh(); }
  catch (err) { setErr('unlockErr', /unlock failed/i.test(err.message) ? t('wrongCreds', 'Wrong master password or Secret Key') : err); $('pw').select(); }
  finally { btn.disabled = false; btn.querySelector('.spinner').classList.add('hidden'); }
};
$('lock').onclick = async () => { await send({ type: 'lock' }).catch((e) => setErr('err', e)); refresh(); };
$('retry').onclick = refresh;

let qt = null;
$('q').oninput = () => {
  clearTimeout(qt);
  qt = setTimeout(async () => {
    const q = $('q').value.trim();
    $('resultsWrap').classList.toggle('hidden', !q);
    if (!q) return;
    try { const r = await send({ type: 'search', query: q }); renderItems($('results'), r.items, false, t('noResults', 'No items found')); } catch (e) { setErr('err', e); }
  }, 150);
};

// Generator: local CSPRNG with rejection sampling (no host round-trip, nothing stored).
function generate() {
  const len = Number($('len').value);
  let set = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ';
  if ($('digits').checked) set += '23456789';
  if ($('symbols').checked) set += '!@#$%^&*-_=+?';
  const out = []; const buf = new Uint8Array(1); const lim = 256 - (256 % set.length);
  while (out.length < len) { crypto.getRandomValues(buf); if (buf[0] < lim) out.push(set[buf[0] % set.length]); }
  $('genOut').textContent = out.join(''); $('lenV').textContent = len;
}
for (const id of ['len', 'digits', 'symbols']) $(id).addEventListener('input', generate);
$('regen').onclick = generate;
$('gen').onclick = async () => { try { await copy($('genOut').textContent, t('passwordCopied', 'Password copied')); } catch (e) { setErr('err', e); } };

GitpassI18n.apply(document);
generate();
refresh();
