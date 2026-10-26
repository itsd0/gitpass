// gitpass form detection + fill helpers. Pure DOM logic, no extension APIs, so node tests (jsdom) can load it.
// Detects: login, multi-step login (username-only / password-only steps), sign-up, change-password,
// one-time-code (incl. split digit boxes), credit card and identity forms.
(function (root) {
  'use strict';

  const RX = {
    search: /search|query|\bq\b|filter|coupon|promo|captcha|newsletter/i,
    username: /user.?name|user.?id|login|log.?in|e-?mail|account|identifier|sign.?in|nickname|\buser\b|member/i,
    email: /e-?mail/i,
    confirm: /confirm|repeat|retype|re-?type|again|verify|re-?enter|password.?2|pass.?2|passwd2|cpassword/i,
    current: /current|old|existing|present/i,
    newpw: /new|create|choose|set.?pass|regist|sign.?up|join/i,
    totp: /\botp\b|one.?time|totp|2fa|mfa|two.?factor|verification.?code|verify.?code|auth.?code|authenticat|security.?code|passcode|\bpin\b|sms.?code|^code$|\bcode\b/i,
    signupForm: /sign.?up|regist|create.?(an.?)?account|\bjoin\b|new.?account|enrol/i,
    loginForm: /log.?in|sign.?in|auth|session|account/i,
    changeForm: /change.?pass|reset.?pass|update.?pass|new.?pass|set.?pass/i,
    ccNumber: /card.?num|cc.?num|ccnum|card.?no|cardnumber|credit.?card|debit.?card|\bpan\b/i,
    ccName: /card.?holder|name.?on.?card|cc.?name|holder.?name|cardname/i,
    ccExpMonth: /exp\w*.?month|cc.?month|card.?month|exp.?mm|\bmm\b/i,
    ccExpYear: /exp\w*.?year|cc.?year|card.?year|exp.?yy|\byy(yy)?\b/i,
    ccExp: /exp(iry|iration)?(.?date)?|valid.?(thru|through|until)|mm.?\/?.?yy/i,
    ccCsc: /cvv|cvc|csc|cvn|cid\b|security.?code|card.?code|card.?verification/i,
    givenName: /first.?name|given.?name|fname|forename/i,
    familyName: /last.?name|family.?name|surname|lname/i,
    fullName: /full.?name|^name$|your.?name|\bname\b/i,
    phone: /phone|mobile|\btel\b|cell/i,
    address1: /address.?(line)?.?1|street|addr1|^address$|\baddress\b/i,
    address2: /address.?(line)?.?2|addr2|apt|suite|unit/i,
    city: /city|town|locality/i,
    state: /state|province|region|county/i,
    postal: /zip|postal|post.?code|postcode/i,
    country: /country/i,
    company: /company|organi[sz]ation|business/i,
  };

  // autocomplete token -> role
  const AC = {
    username: 'username', email: 'email', 'current-password': 'current_password', 'new-password': 'new_password',
    'one-time-code': 'totp', 'cc-number': 'cc_number', 'cc-name': 'cc_name', 'cc-given-name': 'cc_name',
    'cc-exp': 'cc_exp', 'cc-exp-month': 'cc_exp_month', 'cc-exp-year': 'cc_exp_year', 'cc-csc': 'cc_csc',
    'given-name': 'given_name', 'family-name': 'family_name', name: 'full_name', tel: 'phone', 'tel-national': 'phone',
    'street-address': 'address1', 'address-line1': 'address1', 'address-line2': 'address2', 'address-level2': 'city',
    'address-level1': 'state', 'postal-code': 'postal', country: 'country', 'country-name': 'country', organization: 'company',
  };
  const CARD_ROLES = new Set(['cc_number', 'cc_name', 'cc_exp', 'cc_exp_month', 'cc_exp_year', 'cc_csc']);
  const ID_ROLES = new Set(['given_name', 'family_name', 'full_name', 'phone', 'address1', 'address2', 'city', 'state', 'postal', 'country', 'company', 'email']);
  const TEXTISH = new Set(['', 'text', 'email', 'tel', 'number', 'search', 'url']);
  const SKIP_TYPES = new Set(['hidden', 'submit', 'button', 'reset', 'image', 'checkbox', 'radio', 'file', 'range', 'color', 'date', 'datetime-local', 'month', 'week', 'time']);

  function isVisible(el, opts) {
    if (!el || el.disabled || el.readOnly || el.hidden) return false;
    if (el.type === 'hidden') return false;
    const view = el.ownerDocument && el.ownerDocument.defaultView;
    for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
      if (n.hidden || (n.getAttribute && n.getAttribute('aria-hidden') === 'true')) return false;
      const st = view && view.getComputedStyle ? view.getComputedStyle(n) : (n.style || {});
      if (st.display === 'none' || st.visibility === 'hidden') return false;
    }
    if (opts && opts.ignoreLayout) return true;
    const r = el.getBoundingClientRect ? el.getBoundingClientRect() : { width: 1, height: 1 };
    return r.width > 0 && r.height > 0;
  }

  function labelText(el) {
    const out = [];
    try { if (el.labels) for (const l of el.labels) out.push(l.textContent); } catch (_) { /* old DOM */ }
    const lb = el.getAttribute && el.getAttribute('aria-labelledby');
    if (lb && el.ownerDocument) for (const id of lb.split(/\s+/)) { const n = el.ownerDocument.getElementById(id); if (n) out.push(n.textContent); }
    // Nearest preceding label-ish text in the same parent (common in div soup forms).
    const prev = el.previousElementSibling;
    if (prev && /^(LABEL|SPAN|DIV|P)$/.test(prev.tagName) && prev.textContent.length < 60) out.push(prev.textContent);
    return out.join(' ');
  }

  /** All the text that hints at what a field is. */
  function hints(el) {
    const a = (k) => (el.getAttribute && el.getAttribute(k)) || '';
    return [a('name'), a('id'), a('placeholder'), a('aria-label'), a('data-testid'), a('title'), labelText(el)].join(' ').replace(/\s+/g, ' ').trim();
  }

  function acTokens(el) {
    const v = ((el.getAttribute && el.getAttribute('autocomplete')) || '').toLowerCase().trim();
    return v ? v.split(/\s+/) : [];
  }

  /** Classify one field. Returns a role string or null. */
  function classifyField(el) {
    if (!el || !el.tagName) return null;
    const tag = el.tagName.toLowerCase();
    if (tag !== 'input' && tag !== 'select') return null;
    const type = tag === 'select' ? 'select' : (el.getAttribute('type') || 'text').toLowerCase();
    if (SKIP_TYPES.has(type)) return null;
    // 1. autocomplete attribute (last recognised token wins: "section-x shipping street-address")
    const toks = acTokens(el);
    if (!toks.includes('off')) for (let i = toks.length - 1; i >= 0; i--) if (AC[toks[i]]) {
      const r = AC[toks[i]];
      if (type === 'password' && r === 'username') break; // nonsense combo; fall back to heuristics
      return r;
    }
    const h = hints(el);
    // 2. password inputs
    if (type === 'password') {
      if (RX.confirm.test(h)) return 'confirm_password';
      if (RX.current.test(h)) return 'current_password';
      if (RX.ccCsc.test(h)) return 'cc_csc';
      if (RX.totp.test(h) && !/password|passwd|pwd/i.test(h)) return 'totp';
      if (RX.newpw.test(h)) return 'new_password';
      return 'password';
    }
    if (type !== 'select' && !TEXTISH.has(type)) return null;
    if (type === 'search' || (RX.search.test(h) && !RX.username.test(h))) return null;
    // 3. cards
    if (RX.ccNumber.test(h)) return 'cc_number';
    if (RX.ccCsc.test(h)) return 'cc_csc';
    if (RX.ccName.test(h)) return 'cc_name';
    if (type !== 'select' && /mm\s*\/?\s*yy|expir\w*.?date|valid.?thru/i.test(h)) return 'cc_exp';
    if (RX.ccExpMonth.test(h)) return 'cc_exp_month';
    if (RX.ccExpYear.test(h)) return 'cc_exp_year';
    if (RX.ccExp.test(h) && type !== 'select') return 'cc_exp';
    // 4. one-time codes
    const ml = Number(el.getAttribute('maxlength') || 0);
    const im = (el.getAttribute('inputmode') || '').toLowerCase();
    if (type !== 'select' && RX.totp.test(h) && !RX.email.test(h)) return 'totp';
    if (type !== 'select' && ml === 1 && (im === 'numeric' || type === 'number' || type === 'tel')) return 'totp_digit';
    // 5. username / email
    if (type === 'email') return 'email';
    if (type !== 'select' && RX.email.test(h)) return 'email';
    if (type !== 'select' && RX.username.test(h)) return 'username';
    // 6. identity
    if (RX.givenName.test(h)) return 'given_name';
    if (RX.familyName.test(h)) return 'family_name';
    if (type === 'tel' || RX.phone.test(h)) return 'phone';
    if (RX.postal.test(h)) return 'postal';
    if (RX.address2.test(h)) return 'address2';
    if (RX.address1.test(h)) return 'address1';
    if (RX.city.test(h)) return 'city';
    if (RX.country.test(h)) return 'country';
    if (RX.state.test(h)) return 'state';
    if (RX.company.test(h)) return 'company';
    if (RX.fullName.test(h)) return 'full_name';
    return type === 'select' ? null : 'text';
  }

  function formHints(scope) {
    if (!scope || !scope.getAttribute) return '';
    const a = (k) => scope.getAttribute(k) || '';
    let s = [a('id'), a('name'), a('action'), a('class'), a('aria-label')].join(' ');
    const btns = scope.querySelectorAll('button, input[type=submit], [role=button]');
    for (const b of btns) s += ' ' + (b.textContent || b.value || '');
    const hd = scope.querySelector('h1, h2, h3, legend');
    if (hd) s += ' ' + hd.textContent;
    return s;
  }

  /** Group fields into forms. Formless fields are grouped by document. */
  function groups(doc, opts) {
    const map = new Map();
    const els = doc.querySelectorAll('input, select');
    for (const el of els) {
      if (!isVisible(el, opts)) continue;
      const role = classifyField(el);
      if (!role) continue;
      const key = el.form || doc;
      if (!map.has(key)) map.set(key, []);
      map.get(key).push({ el, role });
    }
    return map;
  }

  function first(list, ...roles) { for (const r of roles) { const f = list.find((x) => x.role === r); if (f) return f.el; } return null; }

  /** Pick the username field: the last text/email/username field before the first password field. */
  function pickUsername(list, pwEl) {
    const idx = pwEl ? list.findIndex((x) => x.el === pwEl) : list.length;
    const before = list.slice(0, idx < 0 ? list.length : idx);
    for (const pref of [['username'], ['email'], ['text', 'phone']])
      for (let i = before.length - 1; i >= 0; i--) if (pref.includes(before[i].role)) return before[i].el;
    return null;
  }

  function classifyGroup(scope, list) {
    const pw = list.filter((x) => /password$/.test(x.role));
    const fh = formHints(scope === scope.ownerDocument ? null : scope);
    const fields = { username: null, password: null, newPassword: null, confirm: null, current: null, totp: null, totpDigits: [], card: {}, identity: {} };
    for (const x of list) {
      if (CARD_ROLES.has(x.role) && !fields.card[x.role]) fields.card[x.role] = x.el;
      if (x.role === 'totp' && !fields.totp) fields.totp = x.el;
      if (x.role === 'totp_digit') fields.totpDigits.push(x.el);
    }
    if (fields.totpDigits.length >= 4) fields.totp = fields.totp || fields.totpDigits[0]; else fields.totpDigits = [];
    if (fields.card.cc_number) return { kind: 'card', scope, fields };

    if (pw.length) {
      const cur = first(pw, 'current_password');
      const neu = pw.filter((x) => x.role === 'new_password');
      const conf = first(pw, 'confirm_password');
      const plain = pw.filter((x) => x.role === 'password');
      let kind;
      if (cur && (neu.length || conf || plain.length)) kind = 'change_password';
      else if (pw.length >= 3) kind = 'change_password';
      else if (neu.length || conf || pw.length === 2) kind = RX.changeForm.test(fh) && !pickUsername(list, pw[0].el) ? 'change_password' : 'signup';
      else if (RX.signupForm.test(fh) && !RX.loginForm.test(fh.replace(/sign.?up|create.?account/gi, '')) && list.length >= 3) kind = 'signup';
      else kind = 'login';
      if (kind === 'login') {
        fields.password = pw[0].el;
      } else if (kind === 'signup') {
        fields.newPassword = (neu[0] || plain[0] || pw[0]).el;
        fields.confirm = conf || (pw[1] && pw[1].el !== fields.newPassword ? pw[1].el : null);
      } else {
        const rest = pw.filter((x) => x.el !== cur);
        fields.current = cur || (pw.length >= 3 ? pw[0].el : null);
        const restNoCur = fields.current === pw[0].el && !cur ? pw.slice(1) : rest;
        fields.newPassword = (restNoCur.find((x) => x.role !== 'confirm_password') || restNoCur[0]).el;
        fields.confirm = conf || (restNoCur.find((x) => x.el !== fields.newPassword) || {}).el || null;
      }
      fields.username = pickUsername(list, pw[0].el);
      return { kind, scope, fields };
    }
    if (fields.totp) return { kind: 'totp', scope, fields };
    // identity: two or more address-book fields
    const idf = list.filter((x) => ID_ROLES.has(x.role));
    if (idf.length >= 2 && idf.some((x) => x.role !== 'email')) {
      for (const x of idf) if (!fields.identity[x.role]) fields.identity[x.role] = x.el;
      return { kind: 'identity', scope, fields };
    }
    // multi-step login, step 1: a lone username/email field with a login-ish context or autocomplete=username
    const userish = list.filter((x) => ['username', 'email', 'text', 'phone'].includes(x.role));
    if (userish.length >= 1 && userish.length <= 2 && list.length <= 3) {
      const u = first(list, 'username', 'email') || (RX.loginForm.test(fh) ? userish[0].el : null);
      if (u && (acTokens(u).includes('username') || RX.loginForm.test(fh) || RX.username.test(hints(u)) || u.type === 'email')) {
        fields.username = u;
        return { kind: 'login_username', scope, fields };
      }
    }
    return null;
  }

  /** Detect all actionable forms in a document. */
  function detectForms(doc, opts) {
    const out = [];
    for (const [scope, list] of groups(doc, opts)) {
      const f = classifyGroup(scope, list);
      if (f) out.push(f);
    }
    return out;
  }

  /** Which detected form + role does this element belong to? */
  function locate(forms, el) {
    for (const f of forms) {
      const F = f.fields;
      const direct = { username: F.username, password: F.password, new_password: F.newPassword, confirm_password: F.confirm, current_password: F.current, totp: F.totp };
      for (const [role, e] of Object.entries(direct)) if (e === el) return { form: f, role };
      if (F.totpDigits.includes(el)) return { form: f, role: 'totp' };
      for (const [role, e] of Object.entries(F.card)) if (e === el) return { form: f, role };
      for (const [role, e] of Object.entries(F.identity)) if (e === el) return { form: f, role };
    }
    return null;
  }

  /** Fields that get a badge / dropdown. */
  function anchorFields(form) {
    const F = form.fields, out = [];
    const push = (e, role) => { if (e && !out.some((x) => x.el === e)) out.push({ el: e, role }); };
    push(F.username, 'username'); push(F.password, 'password'); push(F.current, 'current_password');
    push(F.newPassword, 'new_password'); push(F.totp, 'totp');
    for (const [r, e] of Object.entries(F.card)) push(e, r);
    for (const [r, e] of Object.entries(F.identity)) push(e, r);
    return out;
  }

  /** Set a value the way a user would, so React/Vue/Angular see it. */
  function setValue(el, value) {
    if (!el || value == null) return false;
    value = String(value);
    if (el.tagName && el.tagName.toLowerCase() === 'select') return setSelect(el, value);
    el.focus && el.focus();
    const proto = Object.getPrototypeOf(el);
    const desc = Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) desc.set.call(el, value); else el.value = value;
    const view = (el.ownerDocument && el.ownerDocument.defaultView) || root;
    const Ev = view.Event || Event;
    el.dispatchEvent(new Ev('input', { bubbles: true }));
    el.dispatchEvent(new Ev('change', { bubbles: true }));
    return true;
  }

  function setSelect(el, value) {
    const v = value.toLowerCase();
    const n = Number(value);
    let opt = Array.from(el.options).find((o) => o.value.toLowerCase() === v || o.text.trim().toLowerCase() === v);
    if (!opt && !Number.isNaN(n)) opt = Array.from(el.options).find((o) => Number(o.value) === n || Number(o.text) === n || Number(o.value) === n % 100);
    if (!opt && v.length > 2) opt = Array.from(el.options).find((o) => o.text.toLowerCase().startsWith(v));
    if (!opt) return false;
    el.value = opt.value;
    const view = (el.ownerDocument && el.ownerDocument.defaultView) || root;
    el.dispatchEvent(new (view.Event || Event)('change', { bubbles: true }));
    return true;
  }

  /** Fill an OTP code; handles split digit boxes. */
  function fillTotp(form, code) {
    const F = form.fields;
    if (F.totpDigits.length >= 4) { String(code).split('').forEach((d, i) => F.totpDigits[i] && setValue(F.totpDigits[i], d)); return true; }
    return setValue(F.totp, code);
  }

  /** Fill a login. Works for full login forms and the single steps of multi-step logins. */
  function fillLogin(form, creds) {
    const F = form.fields; let n = 0;
    if (F.username && creds.username) n += setValue(F.username, creds.username);
    const pw = F.password || F.current;
    if (pw && creds.password) n += setValue(pw, creds.password);
    if (creds.totp && F.totp) n += fillTotp(form, creds.totp);
    return n;
  }

  function fillNewPassword(form, password) {
    const F = form.fields;
    let n = setValue(F.newPassword, password) ? 1 : 0;
    if (F.confirm) n += setValue(F.confirm, password);
    return n;
  }

  const pick = (m, ...keys) => { for (const k of keys) if (m[k] != null && m[k] !== '') return m[k]; return null; };
  /** Normalise a host field map (keys vary) into card values. */
  function cardValues(m) {
    const out = {
      cc_number: pick(m, 'number', 'card_number', 'ccnum', 'cc_number'),
      cc_name: pick(m, 'cardholder', 'cardholder_name', 'name', 'cc_name', 'holder'),
      cc_csc: pick(m, 'cvv', 'cvc', 'csc', 'verification_number', 'security_code', 'cc_csc'),
      cc_exp_month: pick(m, 'exp_month', 'expiry_month', 'month'),
      cc_exp_year: pick(m, 'exp_year', 'expiry_year', 'year'),
    };
    const exp = pick(m, 'expiry', 'expiration', 'expiry_date', 'exp');
    if (exp && (!out.cc_exp_month || !out.cc_exp_year)) {
      const mm = String(exp).match(/(\d{1,4})\D+(\d{1,4})/);
      if (mm) { const [a, b] = [mm[1], mm[2]]; if (a.length === 4) { out.cc_exp_year = a; out.cc_exp_month = b; } else { out.cc_exp_month = a; out.cc_exp_year = b; } }
    }
    if (out.cc_exp_month) out.cc_exp_month = String(out.cc_exp_month).padStart(2, '0');
    if (out.cc_exp_year && String(out.cc_exp_year).length === 2) out.cc_exp_year = '20' + out.cc_exp_year;
    return out;
  }

  function fillCard(form, map) {
    const v = cardValues(map || {}), C = form.fields.card; let n = 0;
    if (C.cc_number) n += setValue(C.cc_number, String(v.cc_number || '').replace(/\s+/g, ''));
    if (C.cc_name) n += setValue(C.cc_name, v.cc_name);
    if (C.cc_csc) n += setValue(C.cc_csc, v.cc_csc);
    if (C.cc_exp_month) n += setValue(C.cc_exp_month, v.cc_exp_month);
    if (C.cc_exp_year) {
      const el = C.cc_exp_year, ml = Number(el.getAttribute && el.getAttribute('maxlength'));
      n += setValue(el, ml === 2 ? String(v.cc_exp_year).slice(-2) : v.cc_exp_year);
    }
    if (C.cc_exp && v.cc_exp_month && v.cc_exp_year) {
      const ph = ((C.cc_exp.getAttribute('placeholder') || '') + ' ' + (C.cc_exp.getAttribute('maxlength') || '')).toLowerCase();
      const yy = /yyyy/.test(ph) ? v.cc_exp_year : String(v.cc_exp_year).slice(-2);
      n += setValue(C.cc_exp, v.cc_exp_month + (/\s\/\s/.test(ph) ? ' / ' : '/') + yy);
    }
    return n;
  }

  function identityValues(m) {
    const given = pick(m, 'first_name', 'firstname', 'given_name');
    const family = pick(m, 'last_name', 'lastname', 'family_name', 'surname');
    return {
      given_name: given, family_name: family,
      full_name: pick(m, 'full_name', 'name') || [given, family].filter(Boolean).join(' ') || null,
      email: pick(m, 'email'), phone: pick(m, 'phone', 'tel', 'mobile'),
      address1: pick(m, 'address1', 'address', 'street', 'address_line1'), address2: pick(m, 'address2', 'address_line2'),
      city: pick(m, 'city', 'town'), state: pick(m, 'state', 'province', 'region'), postal: pick(m, 'zip', 'postal_code', 'postcode', 'zip_code'),
      country: pick(m, 'country'), company: pick(m, 'company', 'organization'),
    };
  }

  function fillIdentity(form, map) {
    const v = identityValues(map || {}); let n = 0;
    for (const [role, el] of Object.entries(form.fields.identity)) if (v[role]) n += setValue(el, v[role]);
    return n;
  }

  /** Read what the user typed, for the save/update banner. Returns null if nothing worth saving. */
  function captureCredentials(form) {
    const F = form.fields;
    const val = (e) => (e && typeof e.value === 'string' ? e.value : '');
    if (form.kind === 'login') { const p = val(F.password); return p ? { kind: 'login', username: val(F.username), password: p } : null; }
    if (form.kind === 'signup') { const p = val(F.newPassword); if (!p || (F.confirm && val(F.confirm) !== p)) return null; return { kind: 'signup', username: val(F.username), password: p }; }
    if (form.kind === 'change_password') { const p = val(F.newPassword); if (!p || (F.confirm && val(F.confirm) !== p)) return null; return { kind: 'change_password', username: val(F.username), password: p }; }
    if (form.kind === 'login_username') { const u = val(F.username); return u ? { kind: 'username', username: u } : null; }
    return null;
  }

  const api = { RX, classifyField, detectForms, locate, anchorFields, isVisible, hints, setValue, fillLogin, fillNewPassword, fillTotp, fillCard, fillIdentity, cardValues, identityValues, captureCredentials, CARD_ROLES, ID_ROLES };
  root.GitpassForms = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
