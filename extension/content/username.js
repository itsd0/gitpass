// ---------- username tracking across multi-step flows ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

const USERNAME_TTL_MS = 5 * 60 * 1000;
let inMemoryUsername = null;
let inMemoryUsernameTs = 0;

// Extension memory only. Never the page's sessionStorage/localStorage: those are readable and
// writable by the page itself. Across page loads the background keeps the value per tab+origin
// (bg-page.js `lastUsername`, 5 min TTL), and page:capture falls back to it.
function getStoredUsername() {
  if (inMemoryUsername && (Date.now() - inMemoryUsernameTs < USERNAME_TTL_MS)) return inMemoryUsername;
  return null;
}

function storeUsername(user) {
  if (!user || typeof user !== 'string') return;
  const trimmed = user.trim().slice(0, 512);
  if (!trimmed || trimmed === inMemoryUsername) { inMemoryUsernameTs = Date.now(); return; }
  inMemoryUsername = trimmed;
  inMemoryUsernameTs = Date.now();
  send('page:capture', { kind: 'username', username: trimmed }).catch(() => {});
}

function handlePotentialUsername(el) {
  if (!el || el.tagName !== 'INPUT') return;
  const type = (el.getAttribute('type') || 'text').toLowerCase();
  if (type === 'password' || type === 'hidden') return;
  const toks = (el.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/);
  const h = GitpassForms.hints ? GitpassForms.hints(el) : ((el.name || '') + ' ' + (el.id || '') + ' ' + (el.placeholder || ''));
  if (toks.includes('username') || toks.includes('email') || type === 'email' || GitpassForms.RX.username.test(h) || GitpassForms.RX.email.test(h)) {
    if (typeof el.value === 'string' && el.value.trim().length >= 1) {
      storeUsername(el.value);
    }
  }
}
