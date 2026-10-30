// Issue #341: sites on the hiddenInlineMenu list get no inline menu badges/dropdown,
// and re-showing the site via chrome.storage.onChanged restores them.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const CONTENT_SRC = ['content.js', 'content/messaging.js', 'content/settings.js', 'content/detection.js', 'content/totp.js', 'content/dropdown.js', 'content/username.js', 'content/banner.js', 'content/command.js', 'content/prompts.js', 'content/wiring.js'].map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n');

function boot(hidden, storageOverrides) {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  let listener = null;
  w.HTMLElement.prototype.getBoundingClientRect = function () { return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 }; };
  w.chrome = {
    runtime: { onMessage: { addListener() {} }, sendMessage: async (m) => m.type === 'page:pending' ? { ok: true, result: {} } : { ok: true, result: { origin: 'https://site.example', items: [], password: 'x', username: 'u' } } },
    i18n: { getMessage: () => '' },
    storage: {
      local: { get: (k, cb) => { const store = Object.assign({ settings: {}, neverAutoSubmit: [], hiddenInlineMenu: hidden }, storageOverrides || {}); setTimeout(() => cb(k === 'settings' ? { settings: store.settings } : { [k]: store[k] }), 0); } },
      onChanged: { addListener: (fn) => { listener = fn; } },
    },
  };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'lib/banner.js']) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  w.eval(CONTENT_SRC);
  return { w, fire: (changes) => listener && listener(changes, 'local') };
}

test('hidden site gets no inline badges', async () => {
  const { w } = boot(['site.example']);
  await new Promise((r) => setTimeout(r, 60));
  assert.strictEqual(w.document.querySelectorAll('gitpass-ui').length, 0);
  w.close();
});

test('showing the site again via storage change restores badges', async () => {
  const { w, fire } = boot(['site.example']);
  await new Promise((r) => setTimeout(r, 60));
  assert.strictEqual(w.document.querySelectorAll('gitpass-ui').length, 0);
  fire({ hiddenInlineMenu: { newValue: [] } });
  await new Promise((r) => setTimeout(r, 30));
  assert.ok(w.document.querySelectorAll('gitpass-ui').length >= 1, 'badges return after un-hiding');
  w.close();
});

test('other sites are unaffected', async () => {
  const { w } = boot(['other.example']);
  await new Promise((r) => setTimeout(r, 60));
  assert.ok(w.document.querySelectorAll('gitpass-ui').length >= 1);
  w.close();
});
