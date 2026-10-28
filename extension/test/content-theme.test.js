// Theme test: the injected shadow-DOM styles ship a dark fallback palette and a light override chosen
// by prefers-color-scheme. Parses the style text of the first shadow root content.js creates.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs'), path = require('path');
const { JSDOM } = require('jsdom');
const root = path.join(__dirname, '..');

function bootContent() {
  const html = fs.readFileSync(path.join(__dirname, 'pages', 'login.html'), 'utf8');
  const dom = new JSDOM(html, { url: 'https://site.example/login', runScripts: 'outside-only', pretendToBeVisual: true });
  const w = dom.window;
  const roots = [];
  const attach = w.Element.prototype.attachShadow;
  w.Element.prototype.attachShadow = function (init) { const r = attach.call(this, init); roots.push(r); return r; };
  w.HTMLElement.prototype.getBoundingClientRect = function () { return { left: 0, top: 0, right: 200, bottom: 30, width: 200, height: 30 }; };
  w.chrome = { runtime: { onMessage: { addListener() {} }, sendMessage: async () => ({ ok: true, result: { origin: 'https://site.example', items: [] } }) }, i18n: { getMessage: () => '' } };
  for (const f of ['lib/policy.js', 'lib/forms.js', 'lib/i18n.js', 'lib/icons.js', 'content.js']) w.eval(fs.readFileSync(path.join(root, f), 'utf8'));
  return { dom, roots };
}

const rule = (css, sel) => {
  const esc = sel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const m = new RegExp(`(?:^|\\n)\\s*${esc}\\s*\\{([^}]*)\\}`).exec(css);
  return m && m[1];
};
const vars = (block) => {
  const out = {};
  for (const m of (block || '').matchAll(/(--gp-[a-z-]+)\s*:\s*([^;}]+)/g)) out[m[1]] = m[2].trim();
  return out;
};
const lum = (hex) => {
  const c = hex.replace('#', '').match(/../g).map((x) => parseInt(x, 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const contrast = (a, b) => {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

test('injected UI ships dark fallback + light prefers-color-scheme palette with AA contrast', async () => {
  const { dom, roots } = bootContent();
  await new Promise((r) => setTimeout(r, 50));
  assert.ok(roots.length, 'content.js created at least one shadow root');
  const css = roots[0].querySelector('style').textContent;

  const host = rule(css, ':host');
  assert.ok(host && /--gp-bg:#1C1C1E/.test(host), 'dark palette is the fallback on :host');
  assert.ok(/color-scheme:light dark/.test(host || ''), ':host declares color-scheme light dark');

  const media = /@media\s*\(prefers-color-scheme:\s*light\)\s*\{\s*:host\s*\{([^}]*)\}\}/.exec(css);
  assert.ok(media, 'light palette media query');
  const light = vars(media[1]);
  assert.strictEqual(light['--gp-bg'], '#FFFFFF');
  assert.strictEqual(light['--gp-fg'], '#1C1C1E');
  assert.ok(!('--gp-accent' in light), 'orange accent stays constant across themes');

  // Surfaces are token-driven, so both themes apply to every injected component.
  for (const sel of ['.menu', '.banner', '.pk-card']) {
    const r = rule(css, sel) || '';
    assert.match(r, /background:var\(--gp-bg\)/, `${sel} background uses --gp-bg`);
    assert.match(r, /color:var\(--gp-fg\)/, `${sel} text uses --gp-fg`);
    assert.match(r, /1px solid var\(--gp-border\)/, `${sel} border uses --gp-border`);
  }
  assert.match(rule(css, '.badge') || '', /background:var\(--gp-bg\)/);
  assert.match(rule(css, '.badge') || '', /var\(--gp-badge-shadow\)/);
  assert.match(rule(css, '.it.active,.it.active:hover') || '', /inset 3px 0 0 var\(--gp-accent\)/);
  assert.match(rule(css, '.btn.pri') || '', /background:var\(--gp-accent\)/);

  // No dark surface colors may leak outside the palette blocks.
  const withoutPalette = css
    .replace(/:host\{[^}]*\}/, '')
    .replace(/@media\s*\(prefers-color-scheme:\s*light\)\s*\{\s*:host\s*\{[^}]*\}\}/, '');
  assert.ok(!/#1C1C1E|#ECEDF0|#969AA4|#3c3e44|#2a2b30|#25262a/.test(withoutPalette), 'rules only use palette variables');

  const dark = vars(host);
  for (const theme of [dark, { ...dark, ...light }]) {
    assert.ok(contrast(theme['--gp-fg'], theme['--gp-bg']) >= 4.5, 'fg on bg >= 4.5');
    assert.ok(contrast(theme['--gp-muted'], theme['--gp-bg']) >= 4.5, 'muted on bg >= 4.5');
    assert.ok(contrast(theme['--gp-muted'], theme['--gp-field']) >= 4.5, 'muted on field >= 4.5');
    assert.ok(contrast(theme['--gp-accent-text'], theme['--gp-bg']) >= 4.5, 'orange text on bg >= 4.5');
    assert.ok(contrast(theme['--gp-accent-fg'], theme['--gp-accent']) >= 4.5, 'primary button text >= 4.5');
    assert.ok(contrast(theme['--gp-fg'], theme['--gp-active']) >= 4.5, 'active row text >= 4.5');
    assert.ok(contrast(theme['--gp-danger'], theme['--gp-bg']) >= 4.5, 'danger text >= 4.5');
    assert.ok(contrast(theme['--gp-ok'], theme['--gp-bg']) >= 4.5, 'success text >= 4.5');
  }
  dom.window.close();
});
