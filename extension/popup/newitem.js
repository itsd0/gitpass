/* gitpass popup: "New item" types. Mirrors the desktop app's New Item templates
 * (apps/desktop/src/features/new-item/categories.ts): same category ids, section ids, field ids and kinds,
 * so an item created here is identical to one created in the GUI. Pure functions (unit-tested in node). */
(function (root) {
  'use strict';
  // ui: text | secret | password (secret + generator) | url | multiline | month_year | date | email | phone
  const f = (id, label, kind, ui, opt) => Object.assign({ id, label, kind, ui: ui || 'text' }, opt || {});
  const TYPES = [
    { id: 'login', label: 'Login', titleLabel: 'Title', section: 'login', url: true, fields: [
      f('username', 'username', 'text'), f('password', 'password', 'concealed', 'password'), f('otp', 'one-time password', 'totp', 'secret', { hint: 'otpauth:// URI or base32 secret' })] },
    { id: 'api_credential', label: 'API Key / Credential', titleLabel: 'Name', section: 'main', fields: [
      f('credential', 'credential', 'concealed', 'secret', { required: true, hintLabel: 'Key / secret' }), f('username', 'username', 'text'),
      f('type', 'type', 'text'), f('hostname', 'hostname', 'url', 'url', { hintLabel: 'URL (optional)' }), f('expires', 'expires', 'date', 'date')] },
    { id: 'secure_note', label: 'Secure Note', titleLabel: 'Title', section: 'main', noNotes: true, fields: [
      f('note', 'note', 'multiline', 'multiline', { required: true })] },
    { id: 'credit_card', label: 'Credit Card', titleLabel: 'Title', section: 'main', fields: [
      f('cardholder', 'cardholder name', 'text'), f('number', 'number', 'concealed', 'secret', { required: true, inputmode: 'numeric' }),
      f('expiry', 'expiry date', 'month_year', 'month_year'), f('cvv', 'verification number', 'concealed', 'secret', { inputmode: 'numeric' }),
      f('type', 'type', 'text'), f('pin', 'PIN', 'concealed', 'secret', { inputmode: 'numeric' })] },
    { id: 'identity', label: 'Identity', titleLabel: 'Title', sections: [
      { id: 'name', title: 'Identification', fields: [f('first', 'first name', 'text'), f('last', 'last name', 'text'), f('birth', 'birth date', 'date', 'date')] },
      { id: 'address', title: 'Address', fields: [f('address', 'address', 'multiline', 'multiline')] },
      { id: 'contact', title: 'Contact', fields: [f('email', 'email', 'email', 'email'), f('phone', 'phone', 'phone', 'phone')] }] },
    { id: 'wifi', label: 'Wi-Fi', titleLabel: 'Title', section: 'main', fields: [
      f('ssid', 'network name', 'text', 'text', { required: true }), f('password', 'wireless password', 'concealed', 'password'), f('security', 'security', 'text')] },
    { id: 'ssh_key', label: 'SSH Key', titleLabel: 'Title', section: 'main', fields: [
      f('private_key', 'private key', 'concealed', 'multiline', { required: true, secret: true }), f('public_key', 'public key', 'multiline', 'multiline'), f('fingerprint', 'fingerprint', 'text')] },
  ];
  function sectionsOf(type) { return type.sections || [{ id: type.section, title: null, fields: type.fields }]; }
  function typeById(id) { return TYPES.find((x) => x.id === id) || TYPES[0]; }
  const digits = (s) => String(s || '').replace(/[\s-]/g, '');
  function luhn(n) {
    let sum = 0, alt = false;
    for (let i = n.length - 1; i >= 0; i--) { let d = n.charCodeAt(i) - 48; if (alt) { d *= 2; if (d > 9) d -= 9; } sum += d; alt = !alt; }
    return sum % 10 === 0;
  }
  /** "MM/YY", "MM/YYYY", "MMYY" -> "MM/YY" (the GUI's month_year format) or null. */
  function normMonthYear(v) {
    const m = /^\s*(\d{1,2})\s*[/\-. ]?\s*(\d{2}|\d{4})\s*$/.exec(String(v || ''));
    if (!m) return null;
    const mm = Number(m[1]); if (mm < 1 || mm > 12) return null;
    return String(mm).padStart(2, '0') + '/' + m[2].slice(-2);
  }
  function isHttpUrl(v) { try { const u = new URL(v); return (u.protocol === 'https:' || u.protocol === 'http:') && !!u.hostname; } catch (_) { return false; } }
  function withScheme(v) { const s = String(v || '').trim(); return s && !/^[a-z][a-z0-9+.-]*:/i.test(s) ? 'https://' + s : s; }

  /** Returns null when valid, else { field, error } (field = 'title' | 'url' | field id). */
  function validate(typeId, values) {
    const type = typeById(typeId); const v = values || {};
    const val = (id) => String(v[id] == null ? '' : v[id]);
    if (!val('title').trim()) return { field: 'title', error: (type.titleLabel || 'Title') + ' is required' };
    if (type.url && val('url').trim() && !isHttpUrl(withScheme(val('url')))) return { field: 'url', error: 'Website must be an http(s) URL' };
    for (const s of sectionsOf(type)) for (const fl of s.fields) {
      const x = val(fl.id);
      if (fl.required && !x.trim()) return { field: fl.id, error: (fl.hintLabel || cap(fl.label)) + ' is required' };
      if (!x.trim()) continue;
      if (fl.ui === 'url' && !isHttpUrl(withScheme(x))) return { field: fl.id, error: cap(fl.label) + ' must be an http(s) URL' };
      if (fl.ui === 'email' && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(x.trim())) return { field: fl.id, error: 'Invalid email address' };
      if (fl.ui === 'month_year' && !normMonthYear(x)) return { field: fl.id, error: 'Use MM/YY' };
      if (fl.ui === 'date' && !/^\d{4}-\d{2}-\d{2}$/.test(x.trim())) return { field: fl.id, error: 'Use YYYY-MM-DD' };
    }
    if (type.id === 'credit_card') {
      const n = digits(val('number'));
      if (!/^\d{12,19}$/.test(n) || !luhn(n)) return { field: 'number', error: 'Card number is not valid' };
      if (val('cvv').trim() && !/^\d{3,4}$/.test(val('cvv').trim())) return { field: 'cvv', error: 'Verification number must be 3 or 4 digits' };
      if (val('pin').trim() && !/^\d{4,12}$/.test(val('pin').trim())) return { field: 'pin', error: 'PIN must be digits' };
    }
    if (type.id === 'identity' && !sectionsOf(type).some((s) => s.fields.some((fl) => val(fl.id).trim()))) return { field: 'first', error: 'Fill in at least one field' };
    if (type.id === 'ssh_key' && !/PRIVATE KEY/.test(val('private_key'))) return { field: 'private_key', error: 'Paste an OpenSSH/PEM private key' };
    if (type.id === 'login' && !val('username').trim() && !val('password')) return { field: 'password', error: 'Enter a username or password' };
    return null;
  }
  function cap(s) { return s.charAt(0).toUpperCase() + s.slice(1); }

  /** Build the desktop app's ItemInput (gui-core api::ItemInput) for the host `create_item` command. */
  function buildItem(typeId, values, vaultId) {
    const type = typeById(typeId); const v = values || {};
    const sections = sectionsOf(type).map((s) => ({ id: s.id, title: s.title, fields: s.fields.map((fl) => {
      let x = v[fl.id] == null ? '' : String(v[fl.id]);
      if (fl.kind !== 'concealed' && fl.kind !== 'multiline') x = x.trim();
      if (fl.ui === 'month_year' && x) x = normMonthYear(x);
      if (fl.ui === 'url' && x) x = withScheme(x);
      if (type.id === 'credit_card' && fl.id === 'number') x = digits(x);
      return { id: fl.id, label: fl.label, kind: fl.kind, value: x === '' ? null : x };
    }).filter((x) => x.value !== null) })).filter((s) => s.fields.length);
    const item = { category: type.id, title: String(v.title || '').trim(), sections };
    if (type.url && String(v.url || '').trim()) item.url = withScheme(v.url);
    if (!type.noNotes && String(v.notes || '').trim()) item.notes = String(v.notes);
    if (vaultId) item.vaultId = vaultId;
    return item;
  }
  const api = { TYPES, typeById, sectionsOf, validate, buildItem, luhn, normMonthYear };
  if (typeof module === 'object' && module.exports) module.exports = api; else root.GitpassNewItem = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
