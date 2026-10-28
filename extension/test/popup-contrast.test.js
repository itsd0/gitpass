const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..');

const lum = (rgb) => {
  const c = rgb.map((v) => v / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};

const contrast = (c1, c2) => {
  const [hi, lo] = [lum(c1), lum(c2)].sort((a, b) => b - a);
  return (hi + 0.05) / (lo + 0.05);
};

test('popup.css uses orange accent tokens matching desktop and satisfies contrast requirements', () => {
  const css = fs.readFileSync(path.join(root, 'popup', 'popup.css'), 'utf8');

  // Verify orange accents: dark theme :root and light theme prefers-color-scheme
  assert.match(css, /--accent:\s*242\s+140\s+40;/);
  assert.match(css, /--accent:\s*194\s+65\s+12;/);

  // Separate --orange token is dropped
  assert.ok(!css.includes('--orange:'), 'separate --orange token dropped in favor of --accent');
  assert.ok(!css.includes('var(--orange)'), 'no remaining var(--orange) usages');

  // No legacy blue accents remain
  assert.ok(!css.includes('26 115 232'), 'no light blue token');
  assert.ok(!css.includes('46 125 246'), 'no dark blue token');
  assert.ok(!css.includes('#1a73e8'), 'no legacy blue hex');

  const lightAccent = [194, 65, 12];
  const darkAccent = [242, 140, 40];
  const white = [255, 255, 255];
  const lightBg = [248, 248, 250];
  const darkBg = [30, 30, 32];

  // Contrast checks
  assert.ok(contrast(lightAccent, white) >= 4.5, 'white text on light accent >= 4.5:1');
  assert.ok(contrast(lightAccent, lightBg) >= 4.5, 'light accent on light bg >= 4.5:1');
  assert.ok(contrast(darkAccent, darkBg) >= 4.5, 'dark accent on dark bg >= 4.5:1');
});
