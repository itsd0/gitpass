// ---------- messaging ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

async function send(type, args) {
  const r = await chrome.runtime.sendMessage({ type, ...(args || {}) });
  if (!r || !r.ok) { const e = new Error((r && r.error) || 'no response'); e.code = (r && r.code) || codeOf(e.message); throw e; }
  return r.result;
}
function codeOf(m) { const x = /\b(locked|denied|not_found|origin_mismatch|bad_request|app_not_running|internal)\b/.exec(m || ''); return x ? x[1] : (/disconnect|native host/i.test(m) ? 'app_not_running' : null); }
function originOk(res) { return res && res.origin === location.origin; } // page navigated meanwhile -> drop

/** The focused, editable password field in the top document (for the popup generator's Fill action). */
function focusedPasswordField() {
  const a = document.activeElement;
  if (!a || !a.tagName || a.tagName.toLowerCase() !== 'input') return null;
  if ((a.getAttribute('type') || 'text').toLowerCase() !== 'password') return null;
  if (a.disabled || a.readOnly) return null;
  return a;
}

function shadowHost(fixed) {
  const host = document.createElement('gitpass-ui');
  const root = host.attachShadow({ mode: 'closed' });
  const st = document.createElement('style'); st.textContent = CSS; root.append(st);
  if (fixed) host.style.position = 'fixed';
  document.documentElement.appendChild(host);
  return { host, root };
}
const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };
