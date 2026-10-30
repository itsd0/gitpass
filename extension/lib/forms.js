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

  function findHiddenUsername(scope) {
    if (!scope || !scope.querySelectorAll) return null;
    const candidates = scope.querySelectorAll('input[type=hidden], input[hidden], input[disabled], input[readonly], input[style*="display: none"], input[style*="visibility: hidden"], [aria-hidden="true"] input');
    for (const el of candidates) {
      const toks = acTokens(el);
      if (toks.includes('username') || toks.includes('email')) return el;
      const type = (el.getAttribute('type') || '').toLowerCase();
      if (type === 'email') return el;
      const h = hints(el);
      if (RX.username.test(h) || RX.email.test(h)) return el;
    }
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
      fields.username = pickUsername(list, pw[0].el) || findHiddenUsername(scope);
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

  /** Payment-processor card iframes in this document (Stripe Elements etc.), best card-number frame first. */
  function paymentFrames(doc) {
    const P = root.GitpassPolicy;
    if (!P || !P.isPaymentFrameOrigin || !doc.querySelectorAll) return [];
    const out = [];
    for (const f of doc.querySelectorAll('iframe')) {
      let origin = null; try { origin = new URL(f.getAttribute('src') || '', doc.baseURI).origin; } catch (_) {}
      if (!origin || !P.isPaymentFrameOrigin(origin)) continue;
      const h = ((f.getAttribute('title') || '') + ' ' + (f.getAttribute('name') || '') + ' ' + (f.getAttribute('src') || '')).toLowerCase();
      if (/3ds|challenge|authenticat|controller|metrics|m\.stripe|apple|google.?pay|payment.?request|link-auth|hcaptcha|address/.test(h)) continue;
      if (!isVisible(f)) continue;
      out.push({ el: f, number: /card.?number|cardnumber|numberinput|card.?payment|encryptedcardnumber|\bnumber\b|elements-inner-card(?:-number)?\.html|componentname=card\b/.test(h) });
    }
    out.sort((a, b) => (b.number ? 1 : 0) - (a.number ? 1 : 0));
    return out.map((x) => x.el);
  }

  /** Detect all actionable forms in a document. */
  function detectForms(doc, opts) {
    const out = [];
    const loose = {}; // card-role fields that are not part of a card form (e.g. cardholder name next to Stripe iframes)
    for (const [scope, list] of groups(doc, opts)) {
      const f = classifyGroup(scope, list);
      if (f) out.push(f);
      if (!f || f.kind !== 'card') for (const x of list) if (CARD_ROLES.has(x.role) && !loose[x.role]) loose[x.role] = x.el;
    }
    // Card fields hosted in processor iframes: one logical card group anchored on the card-number frame;
    // gitpass fills those frames from the background (bg-page.js fillPaymentFrames), never from this page.
    if (!out.some((f) => f.kind === 'card')) {
      const frames = paymentFrames(doc);
      if (frames.length) {
        out.push({ kind: 'card', scope: doc, frames, fields: { username: null, password: null, newPassword: null, confirm: null, current: null, totp: null, totpDigits: [], card: loose, identity: {} } });
      }
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
      if (f.frames && f.frames.includes(el)) return { form: f, role: 'cc_number' };
    }
    return null;
  }

  const CARD_ANCHOR_ORDER = ['cc_number', 'cc_name', 'cc_exp', 'cc_exp_month', 'cc_exp_year', 'cc_csc'];
  function domFirst(els) {
    const list = els.filter(Boolean);
    if (list.length < 2 || !list[0].compareDocumentPosition) return list[0] || null;
    return list.reduce((a, b) => (a.compareDocumentPosition(b) & 4 /* FOLLOWING */ ? a : b));
  }
  /** The ONE field per logical form group that gets the gitpass badge / dropdown (like other password managers):
   *  login -> username (else password); signup -> username (else new password); change -> current (else new);
   *  card -> card number (else the first card field; Stripe-style iframes -> the card-number frame);
   *  identity -> the first identity field in the page; one-time code -> the code field.
   *  Picking an item from that one icon fills the whole group. */
  function anchorFields(form) {
    const F = form.fields;
    let el = null, role = null;
    const pick = (e, r) => { if (!el && e) { el = e; role = r; } };
    switch (form.kind) {
      case 'card':
        pick(F.card.cc_number, 'cc_number');
        if (form.frames && form.frames.length) pick(form.frames[0], 'cc_number');
        for (const r of CARD_ANCHOR_ORDER) pick(F.card[r], r);
        break;
      case 'identity': {
        const first = domFirst(Object.values(F.identity));
        if (first) pick(first, Object.keys(F.identity).find((r) => F.identity[r] === first));
        break;
      }
      case 'totp': pick(F.totp, 'totp'); break;
      case 'change_password': pick(F.current, 'current_password'); pick(F.newPassword, 'new_password'); break;
      case 'signup': pick(F.username, 'username'); pick(F.newPassword, 'new_password'); break;
      default: pick(F.username, 'username'); pick(F.password, 'password'); pick(F.totp, 'totp');
    }
    return el ? [{ el, role }] : [];
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
  /** Normalise a host field map (keys vary) into card values. gitpass-core v5 card ids: number, cardholder, cvv, expiry, pin, type. */
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

  /** Fill every card field in one document (used inside payment-processor iframes, where each frame
   *  may hold just one of number / expiry / CVC). Returns how many fields were set. */
  function fillCardDocument(doc, map) {
    const card = {};
    for (const el of doc.querySelectorAll('input, select')) {
      if (!isVisible(el)) continue;
      const r = classifyField(el);
      if (CARD_ROLES.has(r) && !card[r]) card[r] = el;
    }
    return Object.keys(card).length ? fillCard({ fields: { card } }, map) : 0;
  }

  function identityValues(m) {
    // gitpass-core v5 identity template ids: first, last, birth, address (multiline), email, phone.
    const given = pick(m, 'first', 'first_name', 'firstname', 'given_name');
    const family = pick(m, 'last', 'last_name', 'lastname', 'family_name', 'surname');
    const addr = pick(m, 'address1', 'street', 'address_line1', 'address');
    const addrLines = addr ? String(addr).split(/\r?\n/).map((x) => x.trim()).filter(Boolean) : [];
    return {
      given_name: given, family_name: family,
      full_name: pick(m, 'full_name', 'name') || [given, family].filter(Boolean).join(' ') || null,
      email: pick(m, 'email'), phone: pick(m, 'phone', 'tel', 'mobile'),
      address1: addrLines[0] || null, address2: pick(m, 'address2', 'address_line2'),
      city: pick(m, 'city', 'town'), state: pick(m, 'state', 'province', 'region'), postal: pick(m, 'zip', 'postal_code', 'postcode', 'zip_code'),
      country: pick(m, 'country'), company: pick(m, 'company', 'organization'),
    };
  }

  function fillIdentity(form, map) {
    const v = identityValues(map || {}); let n = 0;
    for (const [role, el] of Object.entries(form.fields.identity)) if (v[role]) n += setValue(el, v[role]);
    return n;
  }

  /** Luhn check on a digits-only card number (mirrors lib/banner.js cardLuhn). */
  function luhnCard(n) {
    if (!/^\d{12,19}$/.test(n || '')) return false;
    let sum = 0, alt = false;
    for (let i = n.length - 1; i >= 0; i--) { let d = n.charCodeAt(i) - 48; if (alt) { d *= 2; if (d > 9) d -= 9; } sum += d; alt = !alt; }
    return sum % 10 === 0;
  }

  /** 'MM/YY', 'MM/YYYY', 'MMYY', 'MM-YY' -> 'MM/YY'; anything else -> ''. */
  function normCardExpiry(v) {
    const m = /^\s*(\d{1,2})\s*[/\-. ]?\s*(\d{2}|\d{4})\s*$/.exec(String(v == null ? '' : v));
    if (!m) return '';
    const mm = Number(m[1]);
    if (mm < 1 || mm > 12) return '';
    return String(mm).padStart(2, '0') + '/' + m[2].slice(-2);
  }

  /** Separate month/year fields -> 'MM/YY'; '' when unusable. Month names accepted. */
  function normCardExpiryParts(month, year) {
    const months = { jan: 1, january: 1, feb: 2, february: 2, mar: 3, march: 3, apr: 4, april: 4,
      may: 5, jun: 6, june: 6, jul: 7, july: 7, aug: 8, august: 8, sep: 9, sept: 9, september: 9,
      oct: 10, october: 10, nov: 11, november: 11, dec: 12, december: 12 };
    const ms = String(month == null ? '' : month).trim().toLowerCase();
    let mm = null;
    if (/^\d{1,2}$/.test(ms)) { const n = Number(ms); if (n >= 1 && n <= 12) mm = n; }
    else if (months[ms]) mm = months[ms];
    if (mm == null) return '';
    const ys = String(year == null ? '' : year).trim();
    if (!/^\d{2}$/.test(ys) && !/^\d{4}$/.test(ys)) return '';
    return String(mm).padStart(2, '0') + '/' + ys.slice(-2);
  }

  /** Read what the user typed, for the save/update banner. Returns null if nothing worth saving. */
  function captureCredentials(form) {
    const F = form.fields;
    // Usernames are trimmed; passwords are taken verbatim (leading/trailing spaces are legal).
    const val = (e) => (e && typeof e.value === 'string' ? e.value.trim() : '');
    const raw = (e) => (e && typeof e.value === 'string' ? e.value : '');
    let u = val(F.username);
    if (!u && form.scope) {
      const h = findHiddenUsername(form.scope);
      if (h) u = val(h);
    }
    if (form.kind === 'login') { const p = raw(F.password); return p ? { kind: 'login', username: u, password: p } : null; }
    if (form.kind === 'signup') { const p = raw(F.newPassword); if (!p || (F.confirm && raw(F.confirm) !== p)) return null; return { kind: 'signup', username: u, password: p }; }
    if (form.kind === 'change_password') { const p = raw(F.newPassword); if (!p || (F.confirm && raw(F.confirm) !== p)) return null; return { kind: 'change_password', username: u, password: p }; }
    if (form.kind === 'login_username') { return u ? { kind: 'username', username: u } : null; }
    if (form.kind === 'card') {
      // Card capture (#196): number + name on card + expiry only. The CVV field
      // (cc_csc) is never read, so it cannot leak into the capture message. The
      // background re-validates (Luhn + sanitise) before anything is stored.
      const C = F.card || {};
      const num = val(C.cc_number).replace(/[\s-]/g, '');
      if (!luhnCard(num)) return null;
      let exp = '';
      if (C.cc_exp) exp = normCardExpiry(val(C.cc_exp));
      else if (C.cc_exp_month || C.cc_exp_year) exp = normCardExpiryParts(val(C.cc_exp_month), val(C.cc_exp_year));
      return { kind: 'card', number: num, name: val(C.cc_name), expiry: exp };
    }
    return null;
  }

  /** Visible submit controls of a <form>: <input type=submit|image>, and <button> that would
   *  submit (type=submit, or no type, or an invalid type — the HTML default for <button> is
   *  submit). Disabled, hidden, or display:none controls do not count. Non-<form> scopes
   *  never submit. */
  function visibleSubmitControls(scope, opts) {
    if (!scope || !scope.querySelectorAll || scope.tagName !== 'FORM') return [];
    const out = [];
    for (const el of scope.querySelectorAll('button, input')) {
      const tag = (el.tagName || '').toLowerCase();
      const type = ((el.getAttribute && el.getAttribute('type')) || '').toLowerCase();
      let submits = false;
      if (tag === 'input') submits = type === 'submit' || type === 'image';
      else if (tag === 'button') submits = type === 'submit' || !['button', 'reset'].includes(type);
      if (submits && isVisible(el, opts)) out.push(el);
    }
    return out;
  }

  /**
   * Auto-submit decision for issue #231 ("submit after fill", opt-in). Returns true only when
   * ALL of these hold:
   * - the detected form is a real <form> of kind login or login_username (cards, identities,
   *   TOTP, signup and change-password forms never auto-submit; formless groups never submit)
   * - the step that was filled is complete: a full login needs its username AND password filled;
   *   a password-only step needs its password; a username-only step needs its username. A
   *   partially filled form fills and stops.
   * - the form has exactly one visible submit control
   * - when origins are supplied, the fill origin attested by the background matches the page
   *   origin (mismatch, or an origin known on only one side, refuses). The item-URL match
   *   itself is enforced upstream: the host refuses a fill whose item URL does not match the
   *   page origin unless the user explicitly confirmed that cross-site fill, so a fill that
   *   reaches this decision was already approved for this page.
   * Cross-origin iframes cannot reach this decision: the content script runs top-frame only
   * (all_frames:false) and only fills forms in its own document.
   * `filled` is { username: bool, password: bool } as observed on the fields after the fill.
   * The caller still gates on the opt-in setting and the per-site exception list, and only
   * calls this from an explicit user fill action (popup Fill, inline pick, ⌘⇧L, right-click).
   */
  function shouldAutoSubmit(form, filled, opts) {
    if (!form || (form.kind !== 'login' && form.kind !== 'login_username')) return false;
    const scope = form.scope;
    if (!scope || scope.tagName !== 'FORM') return false;
    const o = opts || {};
    if ((o.pageOrigin || o.fillOrigin) && o.pageOrigin !== o.fillOrigin) return false;
    const F = form.fields || {};
    const hasUser = !!F.username;
    const hasPw = !!(F.password || F.current);
    const f = filled || {};
    if (hasUser && hasPw) { if (!f.username || !f.password) return false; }
    else if (hasPw) { if (!f.password) return false; }
    else if (hasUser) { if (!f.username) return false; }
    else return false;
    return visibleSubmitControls(scope, o).length === 1;
  }

  const api = { RX, classifyField, detectForms, locate, anchorFields, isVisible, hints, setValue, fillLogin, fillNewPassword, fillTotp, fillCard, fillCardDocument, paymentFrames, fillIdentity, cardValues, identityValues, captureCredentials, findHiddenUsername, CARD_ROLES, ID_ROLES, luhnCard, normCardExpiry, normCardExpiryParts, visibleSubmitControls, shouldAutoSubmit };
  root.GitpassForms = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
