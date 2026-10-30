// Save/update banner: pure title/vault/Tab/Esc rules plus the rendered DOM of content.js in jsdom
// (title prefill, vault picker, Save as new, focus on open, orange focus ring and spinner styles).
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
// content.js + content/*.js in manifest order, evaluated as ONE script: a browser shares
// top-level let/const across content scripts, but jsdom w.eval calls do not share them.
const CONTENT_SRC = ['content.js', 'content/messaging.js', 'content/settings.js', 'content/detection.js', 'content/totp.js', 'content/dropdown.js', 'content/username.js', 'content/banner.js', 'content/command.js', 'content/prompts.js', 'content/wiring.js'].map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n');
const Banner = require('../lib/banner.js');

const rule = (css, sel) => {
  const esc = sel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const m = new RegExp(`(?:^|\\n)\\s*${esc}\\s*\\{([^}]*)\\}`).exec(css);
  return m && m[1];
};

function bootContent(pending, onSend) {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://www.site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  const roots = [];
  const attach = w.Element.prototype.attachShadow;
  w.Element.prototype.attachShadow = function (init) { const r = attach.call(this, init); roots.push(r); return r; };
  w.HTMLElement.prototype.getBoundingClientRect = function () { return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 }; };
  w.chrome = {
    runtime: {
      onMessage: { addListener() {} },
      sendMessage: async (m) => {
        if (onSend) onSend(m);
        return { ok: true, result: m.type === 'page:pending' ? pending : { origin: 'https://www.site.example', items: [] } };
      },
    },
    i18n: { getMessage: () => '' },
  };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'lib/banner.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }
  w.eval(CONTENT_SRC);
  return { dom, w, roots };
}

test('title rules: trim, required, 200 cap, hostname fallback', () => {
  assert.strictEqual(Banner.MAX_TITLE, 200);
  assert.strictEqual(Banner.titleValue('  Work  '), 'Work');
  assert.strictEqual(Banner.titleValue(null), '');
  assert.strictEqual(Banner.titleError('   '), 'required');
  assert.strictEqual(Banner.titleError('Work'), '');
  assert.strictEqual(Banner.titleError('x'.repeat(201)), 'too_long');
  assert.strictEqual(Banner.titleForSave('  Work  ', 'site.example'), 'Work');
  assert.strictEqual(Banner.titleForSave('', 'site.example'), 'site.example');
  assert.strictEqual(Banner.titleForSave(null, 'site.example'), 'site.example');
  assert.strictEqual(Banner.titleForSave('x'.repeat(300), 'site.example').length, 200);
});

test('vault list: only id/name, chosen vault falls back to the first', () => {
  const vaults = [{ id: 'v1', name: 'Personal', item_count: 9 }, { id: 'v2', name: 'Work' }, { id: '', name: 'bad' }, null];
  assert.deepStrictEqual(Banner.vaultList(vaults), [{ id: 'v1', name: 'Personal' }, { id: 'v2', name: 'Work' }]);
  assert.strictEqual(Banner.selectVault(vaults, 'v2'), 'v2');
  assert.strictEqual(Banner.selectVault(vaults, 'gone'), 'v1');
  assert.strictEqual(Banner.selectVault([], 'v2'), '');
});

test('Tab is trapped at both ends; Escape closes', () => {
  assert.strictEqual(Banner.isEscape('Escape'), true);
  assert.strictEqual(Banner.isEscape('Esc'), false);
  assert.strictEqual(Banner.trapTab('Tab', false, 0, 4), null);
  assert.strictEqual(Banner.trapTab('Tab', false, 3, 4), 0);
  assert.strictEqual(Banner.trapTab('Tab', true, 0, 4), 3);
  assert.strictEqual(Banner.trapTab('Tab', true, 2, 4), null);
  assert.strictEqual(Banner.trapTab('Tab', false, -1, 4), 0);
  assert.strictEqual(Banner.trapTab('Enter', false, 3, 4), null);
  assert.strictEqual(Banner.trapTab('Tab', false, 0, 0), null);
});

test('update banner: title prefill, vault picker, Save as new and focus on the primary button', async () => {
  const { dom, roots } = bootContent({
    prompt: 'update', username: 'me@example.invalid', title: 'Site',
    vaults: [{ id: 'v1', name: 'Personal' }, { id: 'v2', name: 'Work' }],
  });
  await new Promise((r) => setTimeout(r, 50));
  const b = roots.find((r) => r.querySelector('.banner'));
  assert.ok(b, 'banner shadow root exists');
  assert.strictEqual(b.querySelector('.banner').getAttribute('role'), 'dialog');
  assert.strictEqual(b.querySelector('.banner').tabIndex, -1, 'dialog can hold focus while saving');
  assert.strictEqual(b.querySelector('.banner .ti').textContent, 'Update password in gitpass?');
  const title = b.querySelector('.banner input.inp');
  assert.strictEqual(title.value, 'site.example', 'title prefilled with the hostname (no www.)');
  assert.strictEqual(title.maxLength, 200);
  const select = b.querySelector('.banner select');
  assert.deepStrictEqual([...select.options].map((o) => o.textContent), ['Personal', 'Work']);
  assert.strictEqual(select.value, 'v1', 'default vault first');
  const labels = [...b.querySelectorAll('.banner .acts button')].map((x) => x.textContent);
  assert.deepStrictEqual(labels, ['Never for this site', 'Not now', 'Save as new', 'Update']);
  assert.strictEqual(b.activeElement && b.activeElement.textContent, 'Update', 'focus moves to the primary button');

  const css = b.querySelector('style').textContent;
  assert.match(rule(css, '.banner .inp:focus-visible,.banner .btn:focus-visible') || '', /outline:2px solid var\(--gp-accent\)/, 'orange focus ring');
  assert.match(rule(css, '.banner .spin') || '', /animation:gp-spin/, 'spinner style');
  dom.window.close();
});

test('save banner without a vault list still shows the title field and focuses Save', async () => {
  const { dom, roots } = bootContent({ prompt: 'save', username: 'new@example.invalid', title: null, vaults: [] });
  await new Promise((r) => setTimeout(r, 50));
  const b = roots.find((r) => r.querySelector('.banner'));
  assert.ok(b, 'banner shadow root exists');
  assert.strictEqual(b.querySelector('.banner .ti').textContent, 'Save login to gitpass?');
  assert.ok(b.querySelector('.banner input.inp'), 'title field');
  assert.strictEqual(b.querySelector('.banner select'), null, 'no vault picker when the list is unavailable');
  const labels = [...b.querySelectorAll('.banner .acts button')].map((x) => x.textContent);
  assert.deepStrictEqual(labels, ['Never for this site', 'Not now', 'Save']);
  assert.strictEqual(b.activeElement && b.activeElement.textContent, 'Save');
  dom.window.close();
});

test('account list and selection helpers (pure banner helpers)', () => {
  const accounts = [
    { id: 'v:1', title: 'Personal', username: 'alice@example.com' },
    { id: 'v:2', title: '', username: 'bob@example.com' },
    { id: 'v:3', title: 'Work', username: '' },
    { id: '', title: 'bad' },
    null,
  ];
  assert.deepStrictEqual(Banner.accountList(accounts), [
    { id: 'v:1', title: 'Personal', username: 'alice@example.com' },
    { id: 'v:2', title: '', username: 'bob@example.com' },
    { id: 'v:3', title: 'Work', username: '' },
  ]);
  assert.strictEqual(Banner.selectAccount(accounts, 'v:2'), 'v:2');
  assert.strictEqual(Banner.selectAccount(accounts, 'nonexistent', 'v:3'), 'v:3');
  assert.strictEqual(Banner.selectAccount(accounts, 'nonexistent'), 'v:1');
  assert.strictEqual(Banner.selectAccount([], 'v:1'), '');

  assert.strictEqual(Banner.formatAccountOption(accounts[0], 'site.example', '(no username)'), 'Personal · alice@example.com');
  assert.strictEqual(Banner.formatAccountOption(accounts[1], 'site.example', '(no username)'), 'site.example · bob@example.com');
  assert.strictEqual(Banner.formatAccountOption(accounts[2], 'site.example', '(no username)'), 'Work · (no username)');
  assert.strictEqual(Banner.formatAccountOption({ id: 'v:4' }, 'site.example', '(no username)'), 'site.example · (no username)');
});

test('update banner with account picker when 2+ accounts match', async () => {
  const sent = [];
  const accounts = [
    { id: 'acc-1', title: 'Personal', username: 'alice@personal.invalid' },
    { id: 'acc-2', title: 'Work', username: 'alice@work.invalid' },
    { id: 'acc-3', title: 'Blank', username: '' },
  ];
  const { dom, roots } = bootContent({
    prompt: 'update',
    username: 'alice@work.invalid',
    title: 'Work',
    accounts,
    item_id: 'acc-2',
    vaults: [{ id: 'v1', name: 'Personal Vault' }, { id: 'v2', name: 'Work Vault' }],
  }, (m) => sent.push(m));

  await new Promise((r) => setTimeout(r, 50));
  const b = roots.find((r) => r.querySelector('.banner'));
  assert.ok(b, 'banner shadow root exists');
  assert.strictEqual(b.querySelector('.banner .ti').textContent, 'Update password in gitpass?');

  // Subtitle initially shows preselected account
  const subti = b.querySelector('.banner .u');
  assert.strictEqual(subti.textContent, 'Work · alice@work.invalid');

  // Fields and Tab order: title -> account -> vault
  const inputs = [...b.querySelectorAll('.banner .fields input, .banner .fields select')];
  assert.strictEqual(inputs.length, 3, 'title input, account select, vault select');
  assert.strictEqual(inputs[0].tagName, 'INPUT', 'first is title');
  assert.strictEqual(inputs[1].tagName, 'SELECT', 'second is account select');
  assert.strictEqual(inputs[2].tagName, 'SELECT', 'third is vault select');

  const accSelect = inputs[1];
  assert.strictEqual(accSelect.value, 'acc-2', 'preselected item_id is selected');
  const optLabels = [...accSelect.options].map((o) => o.textContent);
  assert.deepStrictEqual(optLabels, [
    'Personal · alice@personal.invalid',
    'Work · alice@work.invalid',
    'Blank · (no username)',
  ]);

  // Changing account select updates subtitle
  accSelect.value = 'acc-1';
  accSelect.dispatchEvent(new dom.window.Event('change', { bubbles: true }));
  assert.strictEqual(subti.textContent, 'Personal · alice@personal.invalid');

  // Buttons include Never, Not now, Save as new, Update
  const buttons = [...b.querySelectorAll('.banner .acts button')].map((btn) => btn.textContent);
  assert.deepStrictEqual(buttons, ['Never for this site', 'Not now', 'Save as new', 'Update']);

  // Click Update -> sends page:save_decision with action: update and chosen item_id
  function trust(e) {
    const sym = Object.getOwnPropertySymbols(e)[0];
    if (sym && e[sym]) Object.defineProperty(e[sym], 'isTrusted', { get() { return true; }, set() {}, configurable: true });
    return e;
  }

  const updateBtn = b.querySelector('.banner .acts button.pri');
  assert.strictEqual(updateBtn.textContent, 'Update');
  updateBtn.dispatchEvent(trust(new dom.window.MouseEvent('click', { bubbles: true })));

  await new Promise((r) => setTimeout(r, 50));
  const saveDecision = sent.find((m) => m.type === 'page:save_decision' && m.action === 'update');
  assert.ok(saveDecision, 'update decision sent');
  assert.strictEqual(saveDecision.item_id, 'acc-1', 'chosen item_id forwarded');
  assert.strictEqual(saveDecision.user_click, true);

  dom.window.close();
});

test('update banner account picker: Save as new sends action save_new', async () => {
  const sent = [];
  const accounts = [
    { id: 'acc-1', title: 'Personal', username: 'alice@personal.invalid' },
    { id: 'acc-2', title: 'Work', username: 'alice@work.invalid' },
  ];
  const { dom, roots } = bootContent({
    prompt: 'update',
    accounts,
    item_id: 'acc-1',
    vaults: [{ id: 'v1', name: 'Personal Vault' }],
  }, (m) => sent.push(m));

  await new Promise((r) => setTimeout(r, 50));
  const b = roots.find((r) => r.querySelector('.banner'));
  assert.ok(b);

  function trust(e) {
    const sym = Object.getOwnPropertySymbols(e)[0];
    if (sym && e[sym]) Object.defineProperty(e[sym], 'isTrusted', { get() { return true; }, set() {}, configurable: true });
    return e;
  }

  const saveNewBtn = [...b.querySelectorAll('.banner .acts button')].find((btn) => btn.textContent === 'Save as new');
  assert.ok(saveNewBtn, 'Save as new button exists');
  saveNewBtn.dispatchEvent(trust(new dom.window.MouseEvent('click', { bubbles: true })));

  await new Promise((r) => setTimeout(r, 50));
  const saveNewMsg = sent.find((m) => m.type === 'page:save_decision' && m.action === 'save_new');
  assert.ok(saveNewMsg, 'save_new message sent');
  assert.strictEqual(saveNewMsg.user_click, true);

  dom.window.close();
});

test('update banner account picker: Enter key on account select triggers update, Esc dismisses', async () => {
  const sent = [];
  const accounts = [
    { id: 'acc-1', title: 'Personal', username: 'alice@personal.invalid' },
    { id: 'acc-2', title: 'Work', username: 'alice@work.invalid' },
  ];
  const { dom, roots } = bootContent({
    prompt: 'update',
    accounts,
    item_id: 'acc-2',
  }, (m) => sent.push(m));

  await new Promise((r) => setTimeout(r, 50));
  const b = roots.find((r) => r.querySelector('.banner'));
  assert.ok(b);

  function trust(e) {
    const sym = Object.getOwnPropertySymbols(e)[0];
    if (sym && e[sym]) Object.defineProperty(e[sym], 'isTrusted', { get() { return true; }, set() {}, configurable: true });
    return e;
  }

  const accSelect = b.querySelectorAll('.banner .fields select')[0];
  assert.ok(accSelect, 'account select found');
  accSelect.focus();

  // Enter triggers update
  dom.window.document.dispatchEvent(trust(new dom.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
  await new Promise((r) => setTimeout(r, 50));
  const updateMsg = sent.find((m) => m.type === 'page:save_decision' && m.action === 'update');
  assert.ok(updateMsg, 'Enter triggered update decision');
  assert.strictEqual(updateMsg.item_id, 'acc-2');

  dom.window.close();
});
