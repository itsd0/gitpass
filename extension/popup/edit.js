'use strict';
// ---------- inline edit (logins) ----------
function editDirty() { return !!(S.edit && S.edit.dirty); }
function confirmDiscard() { return !editDirty() || window.confirm(t('discardChanges', 'Discard unsaved changes?')); }
function isHttpUrl(v) { try { const u = new URL(v); return (u.protocol === 'http:' || u.protocol === 'https:') && !!u.hostname; } catch (_) { return false; } }
function withScheme(v) { const s = String(v || '').trim(); return s && !/^[a-z][a-z0-9+.-]*:/i.test(s) ? 'https://' + s : s; }
function focusEditButton() { const b = $('detail') && $('detail').querySelector('[data-action="edit"]'); if (b) b.focus(); }

function startEdit() {
  if (!S.detail || S.detail.category !== 'login' || S.edit) return;
  S.edit = { id: S.detail.id, dirty: false, passwordLoaded: false, passwordTouched: false, saving: false };
  renderEditForm(S.detail);
}

function renderEditForm(d) {
  const pane = $('detail'); pane.textContent = '';
  const head = el('div', 'd-head');
  const ic = el('span', 'ic'); renderRowIcon(ic, d, 28);
  const g = el('div', 'grow');
  g.append(el('h2', null, t('editLogin', 'Edit login')), el('div', 'sub', d.vault_name + ' · ' + t(V.CAT_KEY[d.category], V.CAT_LABEL[d.category])));
  head.append(ic, g);
  pane.append(head);

  const form = el('form', 'stack inline-edit');
  form.autocomplete = 'off'; form.noValidate = true;
  form.style.maxWidth = 'none';
  form.setAttribute('aria-label', t('editLogin', 'Edit login'));
  const label = (text, forId, required) => {
    const l = el('label', null, text + (required ? ' *' : ''));
    l.htmlFor = forId; form.append(l); return l;
  };
  const input = (id, type, value) => {
    const i = Object.assign(el('input'), { type });
    i.id = id; i.value = value || ''; i.spellcheck = false; form.append(i); return i;
  };

  label(t('title', 'Title'), 'edit-title', true);
  const titleInput = input('edit-title', 'text', d.title);
  titleInput.setAttribute('aria-required', 'true');

  label(t('username', 'Username'), 'edit-username');
  const userField = (d.fields || []).find((f) => f.name === 'username');
  const userInput = input('edit-username', 'text', userField ? userField.value : '');

  label(t('password', 'Password'), 'edit-password');
  const pwRow = el('div', 'edit-pw');
  const pwInput = Object.assign(el('input'), { type: 'password' });
  pwInput.id = 'edit-password'; pwInput.autocomplete = 'new-password'; pwInput.spellcheck = false;
  const eye = el('button', 'icon-btn'); eye.type = 'button'; eye.innerHTML = UI.eye; eye.title = t('reveal', 'Reveal');
  eye.onclick = () => { const on = pwInput.type === 'password'; pwInput.type = on ? 'text' : 'password'; eye.innerHTML = on ? UI.eyeOff : UI.eye; };
  const gen = el('button', 'icon-btn'); gen.type = 'button'; gen.innerHTML = UI.key; gen.title = t('generate', 'Generate');
  gen.onclick = () => openGenerator((pw) => {
    pwInput.value = pw;
    pwInput.dispatchEvent(new Event('input', { bubbles: true }));
    pwInput.focus();
  });
  pwRow.append(pwInput, eye, gen);
  form.append(pwRow);
  pwInput.addEventListener('input', () => { if (S.edit) S.edit.passwordTouched = true; });

  label(t('website', 'Website'), 'edit-url-0');
  const urlsWrap = el('div', 'edit-urls');
  form.append(urlsWrap);
  const urlInputs = [];
  let urlSeq = 0;
  const addUrlRow = (value) => {
    const row = el('div', 'edit-url');
    const i = Object.assign(el('input'), { type: 'url' });
    i.id = 'edit-url-' + (urlSeq++);
    i.value = value || ''; i.spellcheck = false;
    i.setAttribute('aria-label', t('website', 'Website'));
    const rm = el('button', 'icon-btn'); rm.type = 'button'; rm.innerHTML = UI.close; rm.title = t('removeWebsite', 'Remove website');
    rm.onclick = () => {
      if (urlInputs.length === 1) { i.value = ''; i.focus(); }
      else { urlInputs.splice(urlInputs.indexOf(i), 1); row.remove(); }
      if (S.edit) S.edit.dirty = true;
    };
    urlInputs.push(i);
    row.append(i, rm);
    urlsWrap.append(row);
    return i;
  };
  const initialUrls = [];
  if (d.url) initialUrls.push(d.url);
  for (const f of d.fields || []) if (f.kind === 'url' && f.name !== 'url' && f.value && !initialUrls.includes(f.value)) initialUrls.push(f.value);
  if (!initialUrls.length) addUrlRow('');
  for (const u of initialUrls) addUrlRow(u);
  const addUrl = el('button', 'btn btn-link add-url', t('addWebsite', 'Add website'));
  addUrl.type = 'button';
  addUrl.onclick = () => { if (S.edit) S.edit.dirty = true; addUrlRow('').focus(); };
  form.append(addUrl);

  label(t('notes', 'Notes'), 'edit-notes');
  const notesInput = el('textarea');
  notesInput.id = 'edit-notes'; notesInput.rows = 3; notesInput.value = d.notes || ''; notesInput.spellcheck = false;
  form.append(notesInput);

  const errEl = el('p', 'err'); errEl.setAttribute('role', 'alert'); form.append(errEl);

  const acts = el('div', 'edit-acts');
  const save = el('button', 'btn btn-primary', t('save', 'Save')); save.type = 'submit';
  const cancel = el('button', 'btn btn-link', t('cancel', 'Cancel')); cancel.type = 'button'; cancel.onclick = () => cancelEdit();
  acts.append(save, cancel); form.append(acts);

  form.addEventListener('input', (e) => {
    if (S.edit) S.edit.dirty = true;
    if (e.target && e.target.classList) e.target.classList.remove('bad');
  });
  form.onsubmit = (e) => { e.preventDefault(); void saveEdit(d, { titleInput, userInput, pwInput, urlInputs, notesInput, errEl, save }); };
  pane.append(form);
  titleInput.focus();

  // The host never returns secrets in item details; fetch the current password for the field.
  const edit = S.edit;
  send({ type: 'popup:reveal', item_id: d.id, field: 'password' })
    .then((r) => {
      if (S.edit !== edit || edit.passwordTouched || pwInput.value) return;
      pwInput.value = r.value || '';
      edit.passwordLoaded = true;
    })
    .catch(() => {});
}

async function saveEdit(d, ui) {
  if (!S.edit || S.edit.saving) return;
  const { titleInput, userInput, pwInput, urlInputs, notesInput, errEl, save } = ui;
  errEl.textContent = '';
  titleInput.classList.remove('bad');
  for (const i of urlInputs) i.classList.remove('bad');
  const title = titleInput.value.trim();
  if (!title) { errEl.textContent = t('titleRequired', 'Title is required'); titleInput.classList.add('bad'); titleInput.focus(); return; }
  const urls = [];
  for (const i of urlInputs) {
    const raw = i.value.trim();
    if (!raw) continue;
    const u = withScheme(raw);
    if (!isHttpUrl(u)) { errEl.textContent = t('websiteInvalid', 'Website must be an http(s) URL'); i.classList.add('bad'); i.focus(); return; }
    if (!urls.includes(u)) urls.push(u);
  }
  const msg = { type: 'popup:update_login', item_id: d.id, title, username: userInput.value.trim(), urls, notes: notesInput.value };
  if (S.edit.passwordLoaded || S.edit.passwordTouched) msg.password = pwInput.value;

  S.edit.saving = true;
  save.disabled = true;
  const prevHtml = save.innerHTML;
  save.innerHTML = '<span class="spinner"></span> <span>' + t('save', 'Save') + '</span>';
  try {
    await send(msg);
    S.edit = null; livePending = false;
    toast(t('saved', 'Saved'));
    await loadData();
    if (S.sel >= 0) await select(S.sel); // loadData's selection is async; wait for the detail to render
    focusEditButton();
  } catch (e) {
    if (S.edit) S.edit.saving = false;
    save.disabled = false;
    save.innerHTML = prevHtml;
    if (e.code === 'locked') { S.edit = null; refresh(); return; }
    errEl.textContent = errText(e);
  }
}

function cancelEdit() {
  if (!S.edit) return;
  if (!confirmDiscard()) return;
  S.edit = null;
  if (S.detail) renderDetail(S.detail); else select(S.sel);
  focusEditButton();
  if (livePending) { livePending = false; liveReload(); }
}

async function secretValue(d, f) {
  if (S.revealed[f.name] != null) return S.revealed[f.name];
  if (!f.concealed) return f.value || '';
  if (f.kind === 'totp') return (await send({ type: 'popup:totp', item_id: d.id })).code;
  return (await send({ type: 'popup:reveal', item_id: d.id, field: f.name })).value;
}

function fieldRow(d, f) {
  const row = el('div', 'f'); const g = el('div', 'grow');
  g.append(el('div', 'lbl', f.label || f.name));
  const val = el('div', 'val'); g.append(val); row.append(g);
  const acts = el('div', 'acts'); row.append(acts);
  if (f.kind === 'totp') { renderTotp(d, val); }
  else if (f.concealed) {
    val.textContent = V.mask(f.kind, f.masked);
    acts.append(iconBtn(UI.eye, t('reveal', 'Reveal'), async (b) => {
      if (b.disabled) return;
      if (S.revealed[f.name] != null) { delete S.revealed[f.name]; val.textContent = V.mask(f.kind, f.masked); b.innerHTML = UI.eye; return; }
      b.disabled = true;
      const prevHtml = b.innerHTML;
      b.innerHTML = '<span class="spinner"></span>';
      try {
        const v = await secretValue(d, f);
        S.revealed[f.name] = v;
        val.textContent = V.formatValue(f.kind, v);
        b.innerHTML = UI.eyeOff;
      } catch (err) {
        b.innerHTML = prevHtml;
        toast(errText(err));
      } finally {
        b.disabled = false;
      }
    }));
  } else if (f.kind === 'url' && f.value) {
    const a = el('a', null, f.value); a.href = f.value; a.target = '_blank'; a.rel = 'noopener'; val.append(a); val.classList.add('plain');
  } else { val.textContent = V.formatValue(f.kind, f.value); if (!/password|card_number/.test(f.kind)) val.classList.add('plain'); }
  if (f.concealed) {
    const label = t('largeTypeField', 'Show $1 in large type', [f.label || f.name]);
    const lt = iconBtn(UI.type, label, (b) => openLargeType(d, f, b));
    lt.dataset.action = 'large-type';
    lt.dataset.field = f.name;
    lt.setAttribute('aria-label', label);
    acts.append(lt);
  }
  acts.append(iconBtn(UI.copy, t('copy', 'Copy'), async (b) => {
    if (b.disabled) return;
    b.disabled = true;
    const prevHtml = b.innerHTML;
    b.innerHTML = '<span class="spinner"></span>';
    try {
      const v = await secretValue(d, f);
      await copy(v, t('copied', 'Copied'));
    } catch (err) {
      toast(errText(err));
    } finally {
      b.innerHTML = prevHtml;
      b.disabled = false;
    }
  }));
  return row;
}

// TOTP ring countdown. Fix for v0.5.1 popup.js:45 null classList: the old handler kept a setInterval that
// touched an element after the list re-rendered. Now there is one timer, guarded by isConnected and cleared on every re-render.
const RING_C = 2 * Math.PI * 7;
function renderTotp(d, val) {
  stopTotp();
  const wrap = el('div', 'totp'); const code = el('span', 'totp-code', '••• •••');
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg'); svg.setAttribute('viewBox', '0 0 18 18'); svg.setAttribute('class', 'ring');
  const bg = document.createElementNS(ns, 'circle'), fg = document.createElementNS(ns, 'circle');
  for (const c of [bg, fg]) { c.setAttribute('cx', '9'); c.setAttribute('cy', '9'); c.setAttribute('r', '7'); }
  bg.setAttribute('class', 'bg'); fg.setAttribute('class', 'fg'); fg.setAttribute('stroke-dasharray', RING_C.toFixed(3));
  svg.append(bg, fg); wrap.append(code, svg); val.append(wrap);
  let st = null, fetching = false;
  const tick = async () => {
    if (!wrap.isConnected) return stopTotp();
    if (!st || st.remaining <= 0) {
      if (fetching) return; fetching = true;
      try { const r = await send({ type: 'popup:totp', item_id: d.id }); st = { code: r.code, remaining: r.remaining, period: r.period || 30 }; S.revealed.totp = r.code; }
      catch (e) { code.textContent = '—'; code.classList.add('bad'); wrap.title = t('totpUnavailable', 'Code unavailable'); svg.classList.add('hidden'); return stopTotp(); } finally { fetching = false; }
      if (!wrap.isConnected) return stopTotp();
      code.textContent = V.formatValue('totp', st.code);
    } else st.remaining -= 1;
    fg.setAttribute('stroke-dashoffset', String(V.ringOffset(st.remaining, st.period, RING_C)));
    svg.classList.toggle('low', st.remaining <= 5);
  };
  S.totpTimer = setInterval(tick, 1000);
  setTimeout(tick, 0); // the row is still detached while fieldRow builds it; the isConnected guard needs it mounted
}
function stopTotp() { if (S.totpTimer) { clearInterval(S.totpTimer); S.totpTimer = null; } }
