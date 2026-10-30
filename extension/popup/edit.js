'use strict';
// ---------- inline edit (logins, secure notes, API credentials) ----------
function editDirty() { return !!(S.edit && S.edit.dirty); }
function confirmDiscard() { return !editDirty() || window.confirm(t('discardChanges', 'Discard unsaved changes?')); }
function isHttpUrl(v) { try { const u = new URL(v); return (u.protocol === 'http:' || u.protocol === 'https:') && !!u.hostname; } catch (_) { return false; } }
function withScheme(v) { const s = String(v || '').trim(); return s && !/^[a-z][a-z0-9+.-]*:/i.test(s) ? 'https://' + s : s; }
function focusEditButton() { const b = $('detail') && $('detail').querySelector('[data-action="edit"]'); if (b) b.focus(); }
function canEditCategory(cat) { return cat === 'login' || cat === 'secure_note' || cat === 'api_credential'; }
function editHeading(cat) {
  if (cat === 'secure_note') return t('editSecureNote', 'Edit secure note');
  if (cat === 'api_credential') return t('editApiCredential', 'Edit API credential');
  return t('editLogin', 'Edit login');
}
function isIsoDate(s) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return false;
  const month = Number(m[2]);
  const day = Number(m[3]);
  return month >= 1 && month <= 12 && day >= 1 && day <= 31;
}
function parseTags(s) {
  const out = [];
  for (const part of String(s || '').split(',')) {
    const tag = part.trim();
    if (tag && !out.includes(tag)) out.push(tag);
  }
  return out;
}
function fieldByName(d, names) {
  return (d.fields || []).find((f) => names.includes(f.name)) || null;
}
/** Notes textarea source: item notes when they have text, otherwise the template note/body field. */
function secureNoteSource(d) {
  const notes = d.notes == null ? '' : String(d.notes);
  if (notes.trim()) return { noteField: null, value: notes };
  const named = fieldByName(d, ['note', 'body']);
  const multi = named || (d.fields || []).find((f) => !f.concealed && f.kind === 'multiline');
  if (multi && !multi.concealed) return { noteField: multi.name, value: multi.value || '' };
  return { noteField: null, value: '' };
}

function startEdit() {
  if (!S.detail || S.detail.deleted_at || !canEditCategory(S.detail.category) || S.edit) return;
  S.edit = { id: S.detail.id, dirty: false, passwordLoaded: false, passwordTouched: false, credentialLoaded: false, credentialTouched: false, saving: false };
  renderEditForm(S.detail);
}

function renderEditForm(d) {
  if (d.category === 'secure_note') { renderNoteEditForm(d); return; }
  if (d.category === 'api_credential') { renderApiEditForm(d); return; }
  const pane = $('detail'); pane.textContent = '';
  const head = el('div', 'd-head');
  const ic = el('span', 'ic'); renderRowIcon(ic, d, 28);
  const g = el('div', 'grow');
  const heading = editHeading(d.category);
  g.append(el('h2', null, heading), el('div', 'sub', d.vault_name + ' · ' + t(V.CAT_KEY[d.category], V.CAT_LABEL[d.category])));
  head.append(ic, g);
  pane.append(head);

  const form = el('form', 'stack inline-edit');
  form.autocomplete = 'off'; form.noValidate = true;
  form.style.maxWidth = 'none';
  form.setAttribute('aria-label', heading);
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

function editShell(d) {
  const pane = $('detail'); pane.textContent = '';
  const heading = editHeading(d.category);
  const head = el('div', 'd-head');
  const ic = el('span', 'ic'); renderRowIcon(ic, d, 28);
  const g = el('div', 'grow');
  g.append(el('h2', null, heading), el('div', 'sub', d.vault_name + ' · ' + t(V.CAT_KEY[d.category], V.CAT_LABEL[d.category])));
  head.append(ic, g);
  pane.append(head);
  const form = el('form', 'stack inline-edit');
  form.autocomplete = 'off'; form.noValidate = true;
  form.style.maxWidth = 'none';
  form.setAttribute('aria-label', heading);
  const label = (text, forId, required) => {
    const l = el('label', null, text + (required ? ' *' : ''));
    l.htmlFor = forId; form.append(l); return l;
  };
  const input = (id, type, value) => {
    const i = Object.assign(el('input'), { type });
    i.id = id; i.value = value || ''; i.spellcheck = false; form.append(i); return i;
  };
  return { pane, form, label, input };
}

function finishEditForm(pane, form, titleInput, onSubmit) {
  const errEl = el('p', 'err'); errEl.setAttribute('role', 'alert'); form.append(errEl);
  const acts = el('div', 'edit-acts');
  const save = el('button', 'btn btn-primary', t('save', 'Save')); save.type = 'submit';
  const cancel = el('button', 'btn btn-link', t('cancel', 'Cancel')); cancel.type = 'button'; cancel.onclick = () => cancelEdit();
  acts.append(save, cancel); form.append(acts);
  form.addEventListener('input', (e) => {
    if (S.edit) S.edit.dirty = true;
    if (e.target && e.target.classList) e.target.classList.remove('bad');
  });
  form.onsubmit = (e) => { e.preventDefault(); void onSubmit(errEl, save); };
  pane.append(form);
  titleInput.focus();
  return { errEl, save };
}

async function commitEdit(msg, errEl, save) {
  if (!S.edit || S.edit.saving) return;
  S.edit.saving = true;
  save.disabled = true;
  const prevHtml = save.innerHTML;
  save.innerHTML = '<span class="spinner"></span> <span>' + t('save', 'Save') + '</span>';
  try {
    await send(msg);
    S.edit = null; livePending = false;
    toast(t('saved', 'Saved'));
    await loadData();
    if (S.sel >= 0) await select(S.sel);
    focusEditButton();
  } catch (e) {
    if (S.edit) S.edit.saving = false;
    save.disabled = false;
    save.innerHTML = prevHtml;
    if (e.code === 'locked') { S.edit = null; refresh(); return; }
    errEl.textContent = errText(e);
  }
}

function renderNoteEditForm(d) {
  const { pane, form, label, input } = editShell(d);
  const src = secureNoteSource(d);
  label(t('title', 'Title'), 'edit-title', true);
  const titleInput = input('edit-title', 'text', d.title);
  titleInput.setAttribute('aria-required', 'true');
  label(t('notes', 'Notes'), 'edit-notes');
  const notesInput = el('textarea');
  notesInput.id = 'edit-notes'; notesInput.rows = 6; notesInput.value = src.value; notesInput.spellcheck = false;
  form.append(notesInput);
  label(t('filterTags', 'Tags'), 'edit-tags');
  const tagsInput = input('edit-tags', 'text', (d.tags || []).join(', '));
  tagsInput.placeholder = t('tagsPh', 'Comma separated');
  finishEditForm(pane, form, titleInput, (errEl, save) => {
    errEl.textContent = '';
    titleInput.classList.remove('bad');
    const title = titleInput.value.trim();
    if (!title) { errEl.textContent = t('titleRequired', 'Title is required'); titleInput.classList.add('bad'); titleInput.focus(); return; }
    const msg = { type: 'popup:update_login', item_id: d.id, title, notes: notesInput.value, tags: parseTags(tagsInput.value) };
    if (src.noteField) msg.note_field = src.noteField;
    return commitEdit(msg, errEl, save);
  });
}

function renderApiEditForm(d) {
  const { pane, form, label, input } = editShell(d);
  label(t('title', 'Title'), 'edit-title', true);
  const titleInput = input('edit-title', 'text', d.title);
  titleInput.setAttribute('aria-required', 'true');

  label(t('username', 'Username'), 'edit-username');
  const userField = fieldByName(d, ['username']);
  const userInput = input('edit-username', 'text', userField && !userField.concealed ? userField.value : '');

  label(t('credential', 'Credential'), 'edit-credential');
  const credRow = el('div', 'edit-pw');
  const credInput = Object.assign(el('input'), { type: 'password' });
  credInput.id = 'edit-credential'; credInput.autocomplete = 'off'; credInput.spellcheck = false;
  credInput.setAttribute('aria-label', t('credential', 'Credential'));
  const show = el('button', 'icon-btn'); show.type = 'button'; show.innerHTML = UI.eye;
  show.dataset.action = 'show-credential';
  const showLabel = (open) => {
    const text = open ? t('hideCredential', 'Hide credential') : t('showCredential', 'Show credential');
    show.title = text; show.setAttribute('aria-label', text); show.setAttribute('aria-pressed', open ? 'true' : 'false');
  };
  showLabel(false);
  show.setAttribute('aria-controls', 'edit-credential');
  show.onclick = async () => {
    if (show.disabled) return;
    const hidden = credInput.type === 'password';
    if (!hidden) { credInput.type = 'password'; show.innerHTML = UI.eye; showLabel(false); return; }
    show.disabled = true;
    const prev = show.innerHTML;
    show.innerHTML = '<span class="spinner"></span>';
    try {
      await loadCredential(d, credInput);
    } catch (err) {
      show.innerHTML = prev;
      show.disabled = false;
      toast(errText(err));
      return;
    }
    if (!credInput.isConnected) return;
    credInput.type = 'text';
    show.innerHTML = UI.eyeOff;
    show.disabled = false;
    showLabel(true);
  };
  credInput.addEventListener('input', () => { if (S.edit) S.edit.credentialTouched = true; });
  credRow.append(credInput, show);
  form.append(credRow);

  label(t('hostname', 'Hostname'), 'edit-hostname');
  const hostField = fieldByName(d, ['hostname', 'host']);
  const hostInput = input('edit-hostname', 'url', (hostField && hostField.value) || d.url || '');
  hostInput.setAttribute('aria-label', t('hostname', 'Hostname'));

  label(t('expiry', 'Expiry'), 'edit-expires');
  const expField = fieldByName(d, ['expires', 'expiry']);
  const expValue = (expField && expField.value) || '';
  const expInput = input('edit-expires', /^\d{4}-\d{2}-\d{2}$/.test(expValue) || !expValue ? 'date' : 'text', expValue);
  expInput.setAttribute('aria-label', t('expiry', 'Expiry'));

  label(t('notes', 'Notes'), 'edit-notes');
  const notesInput = el('textarea');
  notesInput.id = 'edit-notes'; notesInput.rows = 3; notesInput.value = d.notes || ''; notesInput.spellcheck = false;
  form.append(notesInput);

  finishEditForm(pane, form, titleInput, (errEl, save) => saveApiEdit(d, { titleInput, userInput, credInput, hostInput, expInput, notesInput, errEl, save }));
}

async function loadCredential(d, input) {
  if (!S.edit || S.edit.credentialLoaded || S.edit.credentialTouched || input.value) return;
  const edit = S.edit;
  const r = await send({ type: 'popup:reveal', item_id: d.id, field: 'credential' });
  if (S.edit !== edit || edit.credentialTouched || input.value) return;
  input.value = r.value || '';
  edit.credentialLoaded = true;
}

async function saveApiEdit(d, ui) {
  if (!S.edit || S.edit.saving) return;
  const { titleInput, userInput, credInput, hostInput, expInput, notesInput, errEl, save } = ui;
  errEl.textContent = '';
  titleInput.classList.remove('bad');
  hostInput.classList.remove('bad');
  expInput.classList.remove('bad');
  const title = titleInput.value.trim();
  if (!title) { errEl.textContent = t('titleRequired', 'Title is required'); titleInput.classList.add('bad'); titleInput.focus(); return; }
  const hostRaw = hostInput.value.trim();
  let hostname = '';
  if (hostRaw) {
    hostname = withScheme(hostRaw);
    if (!isHttpUrl(hostname)) { errEl.textContent = t('websiteInvalid', 'Website must be an http(s) URL'); hostInput.classList.add('bad'); hostInput.focus(); return; }
  }
  const expires = expInput.value.trim();
  if (expires && !isIsoDate(expires)) { errEl.textContent = t('expiryInvalid', 'Expiry must be YYYY-MM-DD'); expInput.classList.add('bad'); expInput.focus(); return; }
  const msg = { type: 'popup:update_login', item_id: d.id, title, username: userInput.value.trim(), hostname, expires, notes: notesInput.value };
  if (S.edit.credentialLoaded || S.edit.credentialTouched) msg.credential = credInput.value;
  return commitEdit(msg, errEl, save);
}

async function secretValue(d, f) {
  if (S.revealed[f.name] != null) return S.revealed[f.name];
  if (!f.concealed) return f.value || '';
  if (f.kind === 'totp') return (await send({ type: 'popup:totp', item_id: d.id })).code;
  return (await send({ type: 'popup:reveal', item_id: d.id, field: f.name })).value;
}

/** Shows a revealed secret in `node`. Colour-coded (digits orange, symbols blue, spaces as a dim ·) unless the
 *  "Colour-code revealed passwords" option is off or the field is a card number; textContent stays the exact value
 *  and the node carries the whole value as one aria-label so screen readers read it as a single string (#336). */
function paintSecret(node, value, colored) {
  node.textContent = '';
  node.removeAttribute('role'); node.removeAttribute('aria-label');
  if (!colored) { node.textContent = value; return; }
  for (const run of V.secretRuns(value)) node.append(el('span', run.kind === 'letter' ? null : 'sec-' + run.kind, run.text));
  node.setAttribute('role', 'img'); node.setAttribute('aria-label', value);
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
      if (S.revealed[f.name] != null) { delete S.revealed[f.name]; paintSecret(val, V.mask(f.kind, f.masked), false); b.innerHTML = UI.eye; return; }
      b.disabled = true;
      const prevHtml = b.innerHTML;
      b.innerHTML = '<span class="spinner"></span>';
      try {
        const v = await secretValue(d, f);
        const colored = f.kind !== 'card_number' && (await popupSettings()).colorPasswords !== false;
        S.revealed[f.name] = v;
        paintSecret(val, V.formatValue(f.kind, v), colored);
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
  const copyBtn = iconBtn(UI.copy, t('copy', 'Copy'), async (b) => {
    if (b.disabled) return;
    b.disabled = true;
    const prevHtml = b.innerHTML;
    b.innerHTML = '<span class="spinner"></span>';
    try {
      const v = await secretValue(d, f);
      const secret = fieldIsSecret(f);
      const copiedLabel = secret && (f.kind === 'password' || f.name === 'password') ? t('passwordCopied', 'Password copied') : t('copied', 'Copied');
      await copy(v, copiedLabel, 30, { concealed: secret });
    } catch (err) {
      toast(errText(err));
    } finally {
      b.innerHTML = prevHtml;
      b.disabled = false;
    }
  });
  if (d.deleted_at) {
    copyBtn.disabled = true;
    copyBtn.title = t('inTrash', 'This item is in Recently Deleted.');
  }
  acts.append(copyBtn);
  if (d.category === 'login' && f.name === 'password' && Array.isArray(d.password_history) && d.password_history.length > 0) {
    row.classList.add('has-history');
    g.append(passwordHistoryDisclosure(d));
  }
  return row;
}

function passwordHistoryDisclosure(d) {
  const wrap = el('div', 'pw-history');
  const toggle = el('button', 'pw-history-toggle');
  toggle.type = 'button';
  toggle.setAttribute('aria-expanded', 'false');
  toggle.setAttribute('data-action', 'password-history-toggle');
  const chev = el('span', 'pw-history-chev');
  chev.innerHTML = UI.chevron;
  const countText = el('span', null, t('passwordHistory', 'Password history ($1)', [String(d.password_history.length)]));
  toggle.append(chev, countText);
  wrap.append(toggle);

  const list = el('div', 'pw-history-list');
  list.style.display = 'none';

  toggle.onclick = () => {
    const open = toggle.getAttribute('aria-expanded') === 'true';
    toggle.setAttribute('aria-expanded', String(!open));
    chev.classList.toggle('open', !open);
    list.style.display = open ? 'none' : 'flex';
  };

  d.password_history.forEach((entry, idx) => {
    const itemRow = el('div', 'pw-history-item');
    itemRow.dataset.index = String(idx);
    const info = el('div', 'pw-history-info');
    const dateStr = entry.replaced_at ? (typeof entry.replaced_at === 'number' || /^\d+$/.test(entry.replaced_at) ? new Date(Number(entry.replaced_at) * (Number(entry.replaced_at) < 1e11 ? 1000 : 1)).toLocaleString() : new Date(entry.replaced_at).toLocaleString()) : '';
    const dateEl = el('div', 'pw-history-date', t('passwordHistoryReplaced', 'Replaced $1', [dateStr]));
    const pwVal = el('div', 'pw-history-val', '••••••••••');
    info.append(dateEl, pwVal);
    itemRow.append(info);

    const itemActs = el('div', 'pw-history-acts');
    const eyeBtn = iconBtn(UI.eye, t('revealPasswordHistory', 'Reveal historical password'), async (b) => {
      if (b.disabled) return;
      const historyKey = 'password_history:' + idx;
      if (S.revealed[historyKey] != null) {
        delete S.revealed[historyKey];
        pwVal.textContent = '••••••••••';
        b.innerHTML = UI.eye;
        return;
      }
      b.disabled = true;
      const prevHtml = b.innerHTML;
      b.innerHTML = '<span class="spinner"></span>';
      try {
        const res = await send({ type: 'popup:reveal', item_id: d.id, field: historyKey });
        S.revealed[historyKey] = res.value;
        const colored = (await popupSettings()).colorPasswords !== false;
        paintSecret(pwVal, res.value, colored);
        b.innerHTML = UI.eyeOff;
      } catch (err) {
        b.innerHTML = prevHtml;
        toast(errText(err));
      } finally {
        b.disabled = false;
      }
    });
    eyeBtn.dataset.action = 'reveal-history';

    const copyBtn = iconBtn(UI.copy, t('copyPasswordHistory', 'Copy historical password'), async (b) => {
      if (b.disabled) return;
      b.disabled = true;
      const prevHtml = b.innerHTML;
      b.innerHTML = '<span class="spinner"></span>';
      try {
        const historyKey = 'password_history:' + idx;
        let val = S.revealed[historyKey];
        if (val == null) {
          const res = await send({ type: 'popup:reveal', item_id: d.id, field: historyKey });
          val = res.value;
        }
        await copy(val, t('passwordCopied', 'Password copied'), 30, { concealed: true });
      } catch (err) {
        toast(errText(err));
      } finally {
        b.innerHTML = prevHtml;
        b.disabled = false;
      }
    });
    copyBtn.dataset.action = 'copy-history';

    itemActs.append(eyeBtn, copyBtn);
    itemRow.append(itemActs);
    list.append(itemRow);
  });

  wrap.append(list);
  return wrap;
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
