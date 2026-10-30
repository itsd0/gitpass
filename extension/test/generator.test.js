// Unit tests for the popup password generator (issue #68): limits, persistence shape, modes, copy, fill and save-as-login.
'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');
const GEN = require('../popup/generator.js');
const root = path.join(__dirname, '..');
const SCRIPTS = ['lib/i18n.js', 'lib/icons.js', 'popup/icons.js', 'popup/view.js', 'popup/newitem.js', 'popup/generator.js', 'popup/skeletons.js', 'popup/recent.js', 'popup/totp-list.js', 'popup/detail.js', 'popup/edit.js', 'popup/largetype.js', 'popup/quick-actions.js', 'popup/panels.js', 'popup/settings.js', 'popup/popup.js'];
const ORIGIN = 'https://example.invalid/login';
const PASSWORD = 'FAKE-Gen-Abc-123';

// Popup DOM with a fake host: no real vault, no chrome.*.
function popup({ generate, stored, canFill = true } = {}) {
  const html = fs.readFileSync(path.join(root, 'popup', 'popup.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'chrome-extension://mock-id/popup/popup.html', runScripts: 'outside-only' });
  const w = dom.window;
  if (stored) w.__gitpassMockGenSettings = stored;
  const sent = [];
  w.__gitpassMockClipboard = '';
  w.__gitpassMock = (msg) => {
    sent.push(msg);
    const type = String(msg.type).replace(/^popup:/, '');
    const ok = (r) => Promise.resolve(r);
    switch (type) {
      case 'status': return ok({ unlocked: true, app_running: true, account: { name: 'Fake User' } });
      case 'list_vaults': return ok({ vaults: [{ id: 'v1', name: 'Personal' }] });
      case 'list_items': return ok({ items: [] });
      case 'match_active': return ok({ items: [], origin: ORIGIN });
      case 'can_fill_generated': return ok({ canFill });
      case 'fill_generated': return ok({ filled: true });
      case 'generate': {
        if (generate) return generate(msg);
        const words = ['amber', 'coral', 'lunar', 'ivory', 'ocean', 'maple', 'quartz', 'willow'];
        const sep = msg.separator != null ? msg.separator : '-';
        let wordList = Array.from({ length: msg.len }, (_, i) => words[i % words.length]);
        if (msg.capitalize) wordList = wordList.map((w) => w[0].toUpperCase() + w.slice(1));
        const pw = msg.pin ? '1234567890'.slice(0, msg.len) : msg.words ? wordList.join(sep) : PASSWORD;
        return ok({ password: pw });
      }
      default: return ok({});
    }
  };
  w.eval("'use strict';\n" + SCRIPTS.map((f) => fs.readFileSync(path.join(root, f), 'utf8')).join('\n'));
  return { dom, w, sent };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { await sleep(30); await sleep(30); };
const q = (w, sel) => w.document.querySelector(sel);
const button = (w, name) => [...w.document.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === name);
const lastGenerate = (sent) => sent.filter((m) => m.type === 'popup:generate').pop();
const key = (w, k, mods = {}) => w.document.dispatchEvent(new w.KeyboardEvent('keydown', { key: k, bubbles: true, ...mods }));
async function openGen(w) { q(w, '#genBtn').click(); await settle(); await settle(); }

test('mode limits match the desktop generator', () => {
  assert.deepStrictEqual(GEN.MODES, {
    random: { min: 8, max: 64, def: 20 },
    memorable: { min: 3, max: 15, def: 5 },
    pin: { min: 4, max: 12, def: 6 },
  });
  assert.strictEqual(GEN.clampLength('random', 2), 8);
  assert.strictEqual(GEN.clampLength('random', 999), 64);
  assert.strictEqual(GEN.clampLength('memorable', 99), 15);
  assert.strictEqual(GEN.clampLength('pin', 1), 4);
  assert.strictEqual(GEN.clampLength('pin', 'nope'), 6);
  assert.strictEqual(GEN.mode('bogus'), 'random');
});

test('stored settings never include the generated value', () => {
  const s = GEN.safeSettings({ mode: 'pin', length: 9, digits: false, symbols: false, password: 'FAKE-secret', extra: 'x' });
  assert.deepStrictEqual(s, { mode: 'pin', length: 9, digits: false, symbols: false, separator: '-', capitalize: false });
  assert.deepStrictEqual(GEN.safeSettings({ mode: 'bogus', length: 999 }), { mode: 'random', length: 64, digits: true, symbols: true, separator: '-', capitalize: false });
  assert.deepStrictEqual(GEN.safeSettings({ mode: 'memorable', length: 5, separator: '_', capitalize: true, password: 'FAKE-secret' }), { mode: 'memorable', length: 5, digits: true, symbols: true, separator: '_', capitalize: true });
  assert.strictEqual(GEN.safeSettings({ separator: 'invalid' }).separator, '-');
  assert.strictEqual(GEN.safeSettings({ separator: ' ' }).separator, ' ');
  assert.strictEqual(GEN.safeSettings({ capitalize: true }).capitalize, true);
  assert.strictEqual(GEN.safeSettings({ capitalize: false }).capitalize, false);
});

test('strength meter bands', () => {
  assert.strictEqual(GEN.strengthScore(''), 0);
  assert.strictEqual(GEN.strengthKey(GEN.strengthScore('aaaaaaaa')), 'weak');
  assert.ok(GEN.strengthScore('FAKE-Abc-123-Symbols!') > 70);
});

test('opens in random mode, shows strength and copies the generated value', async () => {
  const { w, sent } = popup();
  await settle();
  await openGen(w);
  assert.strictEqual(q(w, '#panel .gen-code').textContent, PASSWORD);
  assert.strictEqual(q(w, '#panel .gen-mode.sel').textContent, 'Random');
  const gen = lastGenerate(sent);
  assert.deepStrictEqual([gen.len, gen.digits, gen.symbols, gen.words, gen.pin], [20, true, true, false, false]);
  assert.ok(Number(q(w, '#panel .gen-strength').getAttribute('aria-valuenow')) > 0);
  button(w, 'Copy').click();
  await settle();
  assert.strictEqual(w.__gitpassMockClipboard, PASSWORD);
  const recMsg = sent.find((m) => m.type === 'popup:record_generated');
  assert.ok(recMsg, 'must record generated password on copy');
  assert.strictEqual(recMsg.password, PASSWORD);
});

test('mode tabs switch ranges and ask the host for words or PIN digits', async () => {
  const { w, sent } = popup();
  await settle();
  await openGen(w);

  button(w, 'Memorable').click();
  await settle();
  let gen = lastGenerate(sent);
  assert.deepStrictEqual([gen.words, gen.pin, gen.len], [true, false, 15]); // 20 clamps to the memorable max
  let range = q(w, '#panel input[type=range]');
  assert.deepStrictEqual([range.min, range.max], ['3', '15']);
  assert.match(q(w, '#panel').textContent, /Words/);
  assert.match(q(w, '#panel .gen-code').textContent, /-/, 'words are joined by a separator');

  button(w, 'PIN').click();
  await settle();
  gen = lastGenerate(sent);
  assert.deepStrictEqual([gen.words, gen.pin, gen.digits, gen.symbols, gen.len], [false, true, false, false, 12]);
  range = q(w, '#panel input[type=range]');
  assert.deepStrictEqual([range.min, range.max], ['4', '12']);
  assert.ok(q(w, '#panel .gen-toggles').classList.contains('hidden'));
});

test('length, toggles and mode persist without the generated password', async () => {
  const { w, sent } = popup();
  await settle();
  await openGen(w);

  const num = q(w, '#panel .gen-num');
  num.value = '30';
  num.dispatchEvent(new w.Event('change', { bubbles: true }));
  await settle();
  assert.strictEqual(lastGenerate(sent).len, 30);

  const digits = [...w.document.querySelectorAll('#panel .tog input')][0];
  digits.checked = false;
  digits.dispatchEvent(new w.Event('change', { bubbles: true }));
  await settle();
  assert.strictEqual(lastGenerate(sent).digits, false);

  assert.deepStrictEqual(JSON.parse(JSON.stringify(w.__gitpassMockGenSettings)), { mode: 'random', length: 30, digits: false, symbols: true, separator: '-', capitalize: false });
  assert.ok(!JSON.stringify(w.__gitpassMockGenSettings).includes(PASSWORD));
});

test('restores the last used mode and length', async () => {
  const { w } = popup({ stored: { mode: 'pin', length: 8, digits: false, symbols: false } });
  await settle();
  await openGen(w);
  assert.strictEqual(q(w, '#panel .gen-mode.sel').textContent, 'PIN');
  const range = q(w, '#panel input[type=range]');
  assert.deepStrictEqual([range.min, range.max, range.value], ['4', '12', '8']);
});

test('offers Fill on this page only when a password field is focused', async () => {
  const withField = popup({ canFill: true });
  await settle();
  await openGen(withField.w);
  const fill = button(withField.w, 'Fill on this page');
  assert.ok(fill && !fill.classList.contains('hidden'));
  fill.click();
  await settle();
  const msg = withField.sent.find((m) => m.type === 'popup:fill_generated');
  assert.strictEqual(msg.password, PASSWORD);

  const without = popup({ canFill: false });
  await settle();
  await openGen(without.w);
  assert.ok(button(without.w, 'Fill on this page').classList.contains('hidden'));
});

test('Save as new Login prefills title, URL and the generated password', async () => {
  const { w } = popup();
  await settle();
  await openGen(w);
  button(w, 'Save as new Login…').click();
  await settle();
  assert.strictEqual(q(w, '#panel select').value, 'login');
  assert.strictEqual(q(w, '#panel [data-field=title]').value, 'example.invalid');
  assert.strictEqual(q(w, '#panel [data-field=url]').value, ORIGIN);
  assert.strictEqual(q(w, '#panel [data-field=password]').value, PASSWORD);
});

test('locked host shows an error and never generates in the page', async () => {
  const { w } = popup({ generate: () => Promise.reject(Object.assign(new Error('locked'), { code: 'locked' })) });
  await settle();
  await openGen(w);
  assert.match(q(w, '#panel .err').textContent, /locked/i);
  assert.strictEqual(q(w, '#panel .gen-code').textContent, '');
  assert.ok(button(w, 'Copy').disabled);
});

test('host not running shows a friendly error', async () => {
  const { w } = popup({ generate: () => Promise.reject(Object.assign(new Error('no host'), { code: 'app_not_running' })) });
  await settle();
  await openGen(w);
  assert.match(q(w, '#panel .err').textContent, /not running/i);
  assert.strictEqual(q(w, '#panel .gen-code').textContent, '');
});

test('Command/Ctrl+Shift+G toggles the generator', async () => {
  const { w } = popup();
  await settle();
  key(w, 'G', { metaKey: true, shiftKey: true });
  await settle();
  assert.ok(q(w, '#panel .gen-modes'));
  key(w, 'G', { metaKey: true, shiftKey: true });
  assert.ok(q(w, '#panel').classList.contains('hidden'));
});

test('New item form links to the generator', async () => {
  const { w } = popup();
  await settle();
  q(w, '#newBtn').click();
  await settle();
  assert.ok(button(w, 'Generate a password…'));
});

test('memorable mode shows separator and capitalize controls; changes regenerate and persist', async () => {
  const { w, sent } = popup();
  await settle();
  await openGen(w);

  // Hidden in Random mode
  assert.ok(q(w, '.gen-sep-row').classList.contains('hidden'));
  assert.ok(q(w, '.gen-cap-row').classList.contains('hidden'));

  // Switch to Memorable mode
  button(w, 'Memorable').click();
  await settle();

  // Controls visible in Memorable mode
  assert.ok(!q(w, '.gen-sep-row').classList.contains('hidden'));
  assert.ok(!q(w, '.gen-cap-row').classList.contains('hidden'));

  let gen = lastGenerate(sent);
  assert.strictEqual(gen.words, true);
  assert.strictEqual(gen.separator, '-');
  assert.strictEqual(gen.capitalize, false);

  // Select '_' separator button
  const sepUnderscore = q(w, '.gen-sep[data-sep="_"]');
  assert.ok(sepUnderscore);
  sepUnderscore.click();
  await settle();

  gen = lastGenerate(sent);
  assert.strictEqual(gen.separator, '_');
  assert.strictEqual(gen.capitalize, false);
  assert.match(q(w, '#panel .gen-code').textContent, /_/);
  assert.strictEqual(JSON.parse(JSON.stringify(w.__gitpassMockGenSettings)).separator, '_');

  // Toggle capitalize
  const capTog = q(w, '.gen-cap-row input[type=checkbox]');
  assert.ok(capTog);
  capTog.checked = true;
  capTog.dispatchEvent(new w.Event('change', { bubbles: true }));
  await settle();

  gen = lastGenerate(sent);
  assert.strictEqual(gen.separator, '_');
  assert.strictEqual(gen.capitalize, true);
  assert.strictEqual(JSON.parse(JSON.stringify(w.__gitpassMockGenSettings)).capitalize, true);

  // Arrow key navigation on separator segmented control
  // From '_' (index 2), ArrowRight navigates to ' ' (index 3)
  const sepGroup = q(w, '.gen-seps');
  sepGroup.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
  await settle();

  gen = lastGenerate(sent);
  assert.strictEqual(gen.separator, ' ');
  assert.strictEqual(gen.capitalize, true);
  assert.strictEqual(JSON.parse(JSON.stringify(w.__gitpassMockGenSettings)).separator, ' ');

  // Switch to PIN mode: controls hidden again
  button(w, 'PIN').click();
  await settle();
  assert.ok(q(w, '.gen-sep-row').classList.contains('hidden'));
  assert.ok(q(w, '.gen-cap-row').classList.contains('hidden'));
});

test('restores the last used separator and capitalize in memorable mode', async () => {
  const { w, sent } = popup({ stored: { mode: 'memorable', length: 4, digits: true, symbols: true, separator: '.', capitalize: true } });
  await settle();
  await openGen(w);

  assert.strictEqual(q(w, '#panel .gen-mode.sel').textContent, 'Memorable');
  assert.strictEqual(q(w, '.gen-sep.sel').getAttribute('data-sep'), '.');
  assert.strictEqual(q(w, '.gen-cap-row input[type=checkbox]').checked, true);
  const gen = lastGenerate(sent);
  assert.strictEqual(gen.words, true);
  assert.strictEqual(gen.separator, '.');
  assert.strictEqual(gen.capitalize, true);
});
