/* global chrome */
'use strict';
const $ = (id) => document.getElementById(id);
function send(msg) {
  return new Promise((res, rej) => chrome.runtime.sendMessage(msg, (r) => {
    if (chrome.runtime.lastError) return rej(new Error(chrome.runtime.lastError.message));
    r && r.ok ? res(r.result) : rej(new Error((r && r.error) || 'error'));
  }));
}
function showErr(e) { $('err').textContent = e ? String(e.message || e) : ''; }

function renderItems(el, items, canFill) {
  el.textContent = '';
  if (!items.length) { el.textContent = '—'; return; }
  for (const it of items) {
    const d = document.createElement('div'); d.className = 'item';
    const t = document.createElement('div'); t.textContent = it.title;
    const s = document.createElement('small'); s.textContent = it.username || '';
    d.append(t, s);
    const row = document.createElement('div'); row.className = 'row';
    if (canFill) {
      const f = document.createElement('button'); f.textContent = 'Fill';
      f.onclick = async () => { try { await send({ type: 'fillActive', item_id: it.id }); window.close(); } catch (e) { showErr(e); } };
      row.append(f);
    }
    if (it.has_totp) {
      const c = document.createElement('button'); c.textContent = 'Copy TOTP';
      c.onclick = async () => {
        try { const r = await send({ type: 'totp', item_id: it.id }); await navigator.clipboard.writeText(r.code); c.textContent = r.code + ' (' + r.remaining + 's)'; } catch (e) { showErr(e); }
      };
      row.append(c);
    }
    d.append(row); el.append(d);
  }
}

async function refresh() {
  showErr();
  let st;
  try { st = await send({ type: 'status' }); } catch (e) { showErr(e); $('locked').classList.remove('hidden'); return; }
  $('locked').classList.toggle('hidden', st.unlocked);
  $('unlocked').classList.toggle('hidden', !st.unlocked);
  if (st.unlocked) {
    try {
      const m = await send({ type: 'matchActive' });
      $('origin').textContent = m.origin || '(not an https page)';
      renderItems($('matches'), m.items, !!m.origin);
    } catch (e) { showErr(e); }
  }
}

$('unlockForm').onsubmit = async (e) => {
  e.preventDefault();
  const pw = $('pw').value; $('pw').value = '';
  try { await send({ type: 'unlock', password: pw }); } catch (err) { showErr(err); return; }
  refresh();
};
$('lock').onclick = async () => { await send({ type: 'lock' }).catch(showErr); refresh(); };
let t = null;
$('q').oninput = () => {
  clearTimeout(t);
  t = setTimeout(async () => {
    const q = $('q').value.trim();
    if (!q) { $('results').textContent = ''; return; }
    try { const r = await send({ type: 'search', query: q }); renderItems($('results'), r.items, false); } catch (e) { showErr(e); }
  }, 150);
};
$('gen').onclick = async () => {
  try { const r = await send({ type: 'generate', len: Number($('len').value) }); await navigator.clipboard.writeText(r.password); $('genOut').textContent = r.password; } catch (e) { showErr(e); }
};
refresh();
