const test = require('node:test');
const assert = require('node:assert');
const V = require('../popup/view.js');
const Icons = require('../lib/icons.js');

test('getMonogram extracts first character, uppercase, handling whitespace and unicode', () => {
  assert.strictEqual(V.getMonogram('GitHub'), 'G');
  assert.strictEqual(V.getMonogram('github'), 'G');
  assert.strictEqual(V.getMonogram('  amazon prime  '), 'A');
  assert.strictEqual(V.getMonogram(''), '?');
  assert.strictEqual(V.getMonogram('   '), '?');
  assert.strictEqual(V.getMonogram(null), '?');
  assert.strictEqual(V.getMonogram(undefined), '?');
  assert.strictEqual(V.getMonogram('1Password'), '1');
  assert.strictEqual(V.getMonogram('🍎 Apple ID'), '🍎');
  assert.strictEqual(V.getMonogram('မြန်မာ'), 'မ');

  // Parity with lib/icons.js
  assert.strictEqual(Icons.getMonogram('GitHub'), 'G');
  assert.strictEqual(Icons.getMonogram(''), '?');
});

test('monogramIcon produces accessible SVG with tinted circle matching desktop', () => {
  const svg = V.monogramIcon('GitHub', 20);
  assert.ok(svg.startsWith('<svg'), 'must be SVG element');
  assert.ok(svg.includes('width="20"'), 'width attribute must match size');
  assert.ok(svg.includes('height="20"'), 'height attribute must match size');
  assert.ok(svg.includes('aria-hidden="true"'), 'decorative SVG must have aria-hidden');
  assert.ok(svg.includes('>G</text>'), 'must render uppercase monogram letter');
  assert.ok(svg.includes('fill="#F28C28"'), 'must use gitpass orange accent');

  // XML escaping for special characters
  const escapedSvg = V.monogramIcon('<Special&>', 24);
  assert.ok(escapedSvg.includes('&lt;'), 'must escape <');
  assert.ok(!escapedSvg.includes('<<'), 'raw < not in XML text');
});

test('per-session icon cache stores icons and clears completely on lock', () => {
  const cache = V.createIconCache();
  assert.strictEqual(cache.size(), 0);

  cache.set('github.com', 'data:image/png;base64,GH_ICON');
  cache.set('google.com', 'data:image/png;base64,GOOGLE_ICON');

  assert.strictEqual(cache.has('github.com'), true);
  assert.strictEqual(cache.get('github.com'), 'data:image/png;base64,GH_ICON');
  assert.strictEqual(cache.has('google.com'), true);
  assert.strictEqual(cache.get('google.com'), 'data:image/png;base64,GOOGLE_ICON');
  assert.strictEqual(cache.has('example.com'), false);
  assert.strictEqual(cache.size(), 2);

  // Negative cache representation
  cache.set('example.com', null);
  assert.strictEqual(cache.has('example.com'), true);
  assert.strictEqual(cache.get('example.com'), null);

  // Simulate vault lock event: per-session cache is wiped
  cache.clear();
  assert.strictEqual(cache.size(), 0);
  assert.strictEqual(cache.has('github.com'), false);
  assert.strictEqual(cache.get('github.com'), undefined);
});

test('categoryIcon is preserved for non-login categories', () => {
  for (const cat of ['credit_card', 'identity', 'secure_note', 'wifi', 'ssh_key', 'api_credential']) {
    const svg = Icons.categoryIcon(cat, 32);
    assert.ok(svg.startsWith('<svg'), `category ${cat} must return SVG`);
    assert.ok(svg.includes('aria-hidden="true"'), `category ${cat} must be aria-hidden`);
  }
});
