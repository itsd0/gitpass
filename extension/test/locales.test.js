// Locale hygiene: en/my/th define the same keys, and every en key is referenced somewhere in the extension.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..');
const load = (loc) => JSON.parse(fs.readFileSync(path.join(root, '_locales', loc, 'messages.json'), 'utf8'));
const en = load('en');

// Keys built at runtime (t('genStrength' + level)) are matched by prefix; keys used through
// __MSG_name__ in the manifests are found by the normal scan.
const DYNAMIC_PREFIXES = ['genStrength'];

function sourceFiles(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (['node_modules', 'dist', '_locales', '.git'].includes(e.name)) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) sourceFiles(p, out);
    else if (/\.(js|html|json|css)$/.test(e.name) && e.name !== 'package-lock.json' && p !== __filename) out.push(p);
  }
  return out;
}

test('my and th have exactly the same keys as en', () => {
  const base = Object.keys(en).sort();
  for (const loc of ['my', 'th']) {
    const keys = Object.keys(load(loc)).sort();
    assert.deepStrictEqual(base.filter((k) => !keys.includes(k)), [], `${loc} is missing keys`);
    assert.deepStrictEqual(keys.filter((k) => !base.includes(k)), [], `${loc} has keys not in en`);
  }
});

test('every en key is referenced by the extension (no dead strings)', () => {
  const text = sourceFiles(root).map((f) => fs.readFileSync(f, 'utf8')).join('\n');
  const unused = Object.keys(en).filter((k) => {
    if (DYNAMIC_PREFIXES.some((p) => k.startsWith(p))) return false;
    // Not \b: `__MSG_extDesc__` has word characters (underscores) right next to the key.
    return !new RegExp('(?<![A-Za-z0-9])' + k + '(?![A-Za-z0-9])').test(text);
  });
  assert.deepStrictEqual(unused, [], 'unused locale keys: remove them from en/my/th or wire them up');
});

test('dynamic-prefix allowlist still matches real lookups', () => {
  const popup = fs.readFileSync(path.join(root, 'popup/panels.js'), 'utf8'); // genStrength lookup moved here by the popup.js split (issue #318)
  for (const p of DYNAMIC_PREFIXES) assert.ok(popup.includes(p), `${p} no longer used dynamically; drop it from the allowlist`);
});
