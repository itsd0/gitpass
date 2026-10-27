// Tests for Bug 1: field badge deduplication across re-renders, repeated scans, and overlapping fields.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');

function setupDom(html) {
  const dom = new JSDOM(html, { url: 'https://x.com/i/flow/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  w.HTMLElement.prototype.getBoundingClientRect = function () {
    const left = this._rectLeft != null ? this._rectLeft : 50;
    const top = this._rectTop != null ? this._rectTop : 100;
    const width = this._rectWidth != null ? this._rectWidth : 200;
    const height = this._rectHeight != null ? this._rectHeight : 30;
    return { left, top, right: left + width, bottom: top + height, width, height };
  };
  w.chrome = {
    runtime: {
      onMessage: { addListener() {} },
      sendMessage: async (m) => {
        if (m.type === 'page:pending') return { ok: true, result: { prompt: null } };
        return { ok: true, result: { origin: 'https://x.com', items: [] } };
      }
    },
    storage: { local: { get(k, cb) { cb({ settings: { autofill: true, savePrompts: true } }); }, onChanged: { addListener() {} } } },
    i18n: { getMessage: () => '' }
  };
  return { dom, w };
}

function loadScripts(w) {
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'content.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }
}

test('Bug 1: exactly one badge per input across repeated scans', async () => {
  const html = `<!doctype html><html><body>
    <div id="login-modal">
      <input id="username" name="text" autocomplete="username" placeholder="Phone, email, or username">
    </div>
  </body></html>`;
  const { dom, w } = setupDom(html);
  loadScripts(w);
  await new Promise((r) => setTimeout(r, 60));

  const u = w.document.getElementById('username');
  assert.strictEqual(u.dataset.gitpassDecorated, 'true');

  const badgesInitial = w.document.querySelectorAll('gitpass-ui');
  assert.strictEqual(badgesInitial.length, 1, 'expected 1 badge initially');

  // Trigger repeated mutations / DOM changes that would cause rescan
  for (let i = 0; i < 5; i++) {
    const div = w.document.createElement('div');
    w.document.body.appendChild(div);
    div.remove();
  }
  await new Promise((r) => setTimeout(r, 350));

  const badgesAfter = w.document.querySelectorAll('gitpass-ui');
  assert.strictEqual(badgesAfter.length, 1, 'expected still exactly 1 badge after repeated scans');
  w.close();
});

test('Bug 1: React re-render replacing input node removes old icon and keeps single icon', async () => {
  const html = `<!doctype html><html><body>
    <div id="container">
      <input id="username-v1" name="text" autocomplete="username" placeholder="Phone, email, or username">
    </div>
  </body></html>`;
  const { dom, w } = setupDom(html);
  loadScripts(w);
  await new Promise((r) => setTimeout(r, 60));

  assert.strictEqual(w.document.querySelectorAll('gitpass-ui').length, 1);
  const oldInput = w.document.getElementById('username-v1');
  assert.strictEqual(oldInput.dataset.gitpassDecorated, 'true');

  // React re-render: unmount v1, mount v2 at identical position
  const container = w.document.getElementById('container');
  container.removeChild(oldInput);

  // Synchronous detachment check should prune the old badge immediately
  // Mount new element representing React replacement
  const newInput = w.document.createElement('input');
  newInput.id = 'username-v2';
  newInput.name = 'text';
  newInput.setAttribute('autocomplete', 'username');
  newInput.placeholder = 'Phone, email, or username';
  container.appendChild(newInput);

  await new Promise((r) => setTimeout(r, 350));

  const badges = w.document.querySelectorAll('gitpass-ui');
  assert.strictEqual(badges.length, 1, 'expected exactly 1 badge on the replaced input node');
  assert.strictEqual(newInput.dataset.gitpassDecorated, 'true');
  assert.strictEqual(oldInput.dataset.gitpassDecorated, undefined);
  w.close();
});

test('Bug 1: overlapping inputs (x.com stacked username inputs) are deduped to one badge', async () => {
  const html = `<!doctype html><html><body>
    <div id="wrapper" style="position:relative">
      <input id="u-under" name="text" autocomplete="username" style="position:absolute;left:50px;top:100px">
      <input id="u-over" name="text" autocomplete="username" style="position:absolute;left:50px;top:100px">
    </div>
  </body></html>`;
  const { dom, w } = setupDom(html);
  const u1 = dom.window.document.getElementById('u-under');
  const u2 = dom.window.document.getElementById('u-over');
  u1._rectLeft = 50; u1._rectTop = 100; u1._rectWidth = 200; u1._rectHeight = 30;
  u2._rectLeft = 50; u2._rectTop = 100; u2._rectWidth = 200; u2._rectHeight = 30;

  loadScripts(w);
  await new Promise((r) => setTimeout(r, 60));

  const badges = w.document.querySelectorAll('gitpass-ui');
  assert.strictEqual(badges.length, 1, 'two overlapping inputs must share at most 1 badge');
  w.close();
});

test('Bug 1: positioning updates on resize and scroll events', async () => {
  const html = `<!doctype html><html><body>
    <input id="u" name="username" autocomplete="username">
  </body></html>`;
  const { dom, w } = setupDom(html);
  const u = dom.window.document.getElementById('u');
  u._rectLeft = 100; u._rectTop = 200; u._rectWidth = 150; u._rectHeight = 24;

  loadScripts(w);
  await new Promise((r) => setTimeout(r, 60));

  const badge = w.document.querySelector('gitpass-ui');
  assert.ok(badge);
  // initial: right is 100 + 150 = 250 -> 250 - 28 = 222px; top is 200 + (24-22)/2 = 201px
  assert.strictEqual(badge.style.left, '222px');
  assert.strictEqual(badge.style.top, '201px');

  // Simulate scroll or movement
  u._rectLeft = 150; u._rectTop = 300;
  w.dispatchEvent(new w.Event('scroll'));

  assert.strictEqual(badge.style.left, '272px'); // (150 + 150) - 28 = 272px
  assert.strictEqual(badge.style.top, '301px'); // 300 + 1 = 301px
  w.close();
});
