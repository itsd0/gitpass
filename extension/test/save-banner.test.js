// Save/update banner: pure title/vault/Tab/Esc rules plus the rendered DOM of content.js in jsdom
// (title prefill, vault picker, Save as new, focus on open, orange focus ring and spinner styles).
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');
const Banner = require('../lib/banner.js');

const rule = (css, sel) => {
  const esc = sel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const m = new RegExp(`(?:^|\\n)\\s*${esc}\\s*\\{([^}]*)\\}`).exec(css);
  return m && m[1];
};

function bootContent(pending) {
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
      sendMessage: async (m) => ({ ok: true, result: m.type === 'page:pending' ? pending : { origin: 'https://www.site.example', items: [] } }),
    },
    i18n: { getMessage: () => '' },
  };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'lib/banner.js', 'content.js']) {
    w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  }
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
