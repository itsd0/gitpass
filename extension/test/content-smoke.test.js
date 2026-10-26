// Smoke test: content.js loads in a jsdom page with a stubbed chrome API, adds badges, and shows the
// pending save banner. (Trusted-event paths can't be exercised in jsdom: synthetic events have isTrusted=false,
// which also proves that untrusted clicks don't trigger fills.)
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');

test('content.js boots, badges login fields, ignores untrusted clicks', async () => {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window, sent = [];
  w.HTMLElement.prototype.getBoundingClientRect = function () { return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 }; };
  w.chrome = { runtime: { onMessage: { addListener() {} }, sendMessage: async (m) => { sent.push(m.type); return m.type === 'page:pending' ? { ok: true, result: { prompt: 'save', username: 'me' } } : { ok: true, result: { origin: 'https://site.example', items: [], password: 'x', username: 'u' } }; } }, i18n: { getMessage: () => '' } };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'content.js']) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  await new Promise((r) => setTimeout(r, 50));
  const hosts = w.document.querySelectorAll('gitpass-ui');
  assert.ok(hosts.length >= 4, 'badges (2 forms x 2 fields) + banner, got ' + hosts.length); 
  assert.ok(sent.includes('page:pending'));
  w.document.getElementById('pw').dispatchEvent(new w.FocusEvent('focus'));
  await new Promise((r) => setTimeout(r, 20));
  assert.ok(!sent.includes('page:match'), 'untrusted focus must not open the dropdown');
  assert.strictEqual(w.document.getElementById('pw').value, '');
  w.close();
});
