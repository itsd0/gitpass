const test = require('node:test');
const assert = require('node:assert');
const V = require('../popup/view.js');

const items = [
  { id: 'v1:a', vault_id: 'v1', vault_name: 'Personal', category: 'login', title: 'GitHub', subtitle: 'me@x.com', url: 'https://github.com', favorite: false },
  { id: 'v1:b', vault_id: 'v1', vault_name: 'Personal', category: 'credit_card', title: 'Visa', subtitle: '•••• 4242', favorite: true },
  { id: 'v2:c', vault_id: 'v2', vault_name: 'Work', category: 'login', title: 'AWS', subtitle: 'admin', favorite: false },
  { id: 'v2:d', vault_id: 'v2', vault_name: 'Work', category: 'wifi', title: 'Office', subtitle: 'net', favorite: false },
];

test('buildFilters has suggestions, all, favorites, 9 categories and vaults', () => {
  const f = V.buildFilters([{ id: 'v1', name: 'Personal' }, { id: 'v2', name: 'Work' }]);
  assert.deepStrictEqual(f.slice(0, 3).map((x) => x.id), ['suggested', 'all', 'favorites']);
  assert.strictEqual(f.filter((x) => x.group === 'category').length, 9);
  assert.deepStrictEqual(f.filter((x) => x.group === 'vault').map((x) => x.id), ['vault:v1', 'vault:v2']);
});

test('filterItems by filter, query, favorites first', () => {
  assert.deepStrictEqual(V.filterItems(items, [], 'all', '').map((i) => i.id), ['v1:b', 'v2:c', 'v1:a', 'v2:d']);
  assert.deepStrictEqual(V.filterItems(items, [], 'cat:login', '').map((i) => i.id), ['v2:c', 'v1:a']);
  assert.deepStrictEqual(V.filterItems(items, [], 'vault:v2', '').map((i) => i.id), ['v2:c', 'v2:d']);
  assert.deepStrictEqual(V.filterItems(items, [], 'favorites', '').map((i) => i.id), ['v1:b']);
  assert.deepStrictEqual(V.filterItems(items, [items[0]], 'suggested', '').map((i) => i.id), ['v1:a']);
  // searching from Suggestions searches all items
  assert.deepStrictEqual(V.filterItems(items, [items[0]], 'suggested', 'aws').map((i) => i.id), ['v2:c']);
  assert.deepStrictEqual(V.filterItems(items, [], 'all', 'WORK').map((i) => i.id), ['v2:c', 'v2:d']);
});

test('virtualWindow renders only visible rows plus overscan', () => {
  const w = V.virtualWindow(0, 380, 52, 1000);
  assert.strictEqual(w.start, 0); assert.strictEqual(w.end, 12); assert.strictEqual(w.padTop, 0); assert.strictEqual(w.padBottom, (1000 - 12) * 52);
  const w2 = V.virtualWindow(5200, 380, 52, 1000);
  assert.strictEqual(w2.start, 96); assert.strictEqual(w2.end, 112);
  assert.strictEqual(w2.padTop + (w2.end - w2.start) * 52 + w2.padBottom, 1000 * 52);
  const w3 = V.virtualWindow(0, 380, 52, 3);
  assert.deepStrictEqual([w3.start, w3.end, w3.padBottom], [0, 3, 0]);
});

test('scrollToRow and moveSel', () => {
  assert.strictEqual(V.scrollToRow(0, 100, 380, 52), 0);
  assert.strictEqual(V.scrollToRow(10, 0, 380, 52), 11 * 52 - 380);
  assert.strictEqual(V.scrollToRow(3, 0, 380, 52), 0);
  assert.strictEqual(V.moveSel(-1, 1, 5), 0);
  assert.strictEqual(V.moveSel(-1, -1, 5), 4);
  assert.strictEqual(V.moveSel(4, 1, 5), 4);
  assert.strictEqual(V.moveSel(0, -1, 5), 0);
  assert.strictEqual(V.moveSel(0, 1, 0), -1);
});

test('keyAction maps keyboard shortcuts', () => {
  assert.strictEqual(V.keyAction({ key: 'ArrowDown' }), 'down');
  assert.strictEqual(V.keyAction({ key: 'ArrowUp' }), 'up');
  assert.strictEqual(V.keyAction({ key: 'Enter' }), 'fill');
  assert.strictEqual(V.keyAction({ key: 'Enter' }, { inForm: true }), null);
  assert.strictEqual(V.keyAction({ key: 'c', metaKey: true }), 'copyPassword');
  assert.strictEqual(V.keyAction({ key: 'c', ctrlKey: true }), 'copyPassword');
  assert.strictEqual(V.keyAction({ key: 'c', metaKey: true }, { hasSelection: true }), null);
  assert.strictEqual(V.keyAction({ key: 'C', metaKey: true, shiftKey: true }, { hasTotp: true }), 'copyTotp');
  assert.strictEqual(V.keyAction({ key: 'C', ctrlKey: true, shiftKey: true }, { hasTotp: true }), 'copyTotp');
  assert.strictEqual(V.keyAction({ key: 'C', metaKey: true, shiftKey: true }), 'copyPassword', 'no TOTP: shift falls back to the password');
  assert.strictEqual(V.keyAction({ key: 'C', metaKey: true, shiftKey: true }, { hasTotp: true, hasSelection: true }), null);
  assert.strictEqual(V.keyAction({ key: 'Escape' }), 'close');
  assert.strictEqual(V.keyAction({ key: 'Escape' }, { panel: 'gen' }), 'closePanel');
  assert.strictEqual(V.keyAction({ key: 'ArrowDown' }, { panel: 'gen' }), null);
  assert.strictEqual(V.keyAction({ key: 'x' }), null);
});

test('ringOffset, formatValue, mask', () => {
  const C = 100;
  assert.strictEqual(V.ringOffset(30, 30, C), 0);
  assert.strictEqual(V.ringOffset(15, 30, C), 50);
  assert.strictEqual(V.ringOffset(0, 30, C), 100);
  assert.strictEqual(V.ringOffset(99, 30, C), 0);
  assert.strictEqual(V.formatValue('totp', '123456'), '123 456');
  assert.strictEqual(V.formatValue('totp', '12345678'), '1234 5678');
  assert.strictEqual(V.formatValue('totp', '12345'), '12345');
  assert.strictEqual(V.formatValue('card_number', '4242424242424242'), '4242 4242 4242 4242');
  assert.strictEqual(V.formatValue('text', null), '');
  assert.ok(V.mask('password').startsWith('•'));
  assert.strictEqual(V.mask('card_number', '•••• •••• •••• 7575'), '•••• •••• •••• 7575');
  assert.strictEqual(V.mask('card_number'), '•••• •••• •••• ••••');
  assert.strictEqual(V.mask('password', '•••• 7575'), '••••••••••', 'non-card secrets (CVV) ignore any mask hint');
});

test('fillKind and primarySecretField per category', () => {
  assert.strictEqual(V.fillKind('login'), 'login');
  assert.strictEqual(V.fillKind('credit_card'), 'card');
  assert.strictEqual(V.fillKind('identity'), 'identity');
  for (const c of ['secure_note', 'api_credential', 'document', 'ssh_key', 'wifi']) assert.strictEqual(V.fillKind(c), null);
  assert.strictEqual(V.primarySecretField({ fields: [{ name: 'u', kind: 'text' }, { name: 'p', kind: 'password', concealed: true }] }).name, 'p');
  assert.strictEqual(V.primarySecretField({ fields: [{ name: 'cvv', kind: 'password', concealed: true }, { name: 'n', kind: 'card_number', concealed: true }] }).name, 'cvv');
  assert.strictEqual(V.primarySecretField({ fields: [] }), null);
  assert.strictEqual(V.primarySecretField(null), null);
});

test('viewFor chooses locked / appdown / nohost / unlocked', () => {
  assert.strictEqual(V.viewFor({ unlocked: true, app_running: true }), 'unlocked');
  assert.strictEqual(V.viewFor({ unlocked: false, app_running: true }), 'locked');
  assert.strictEqual(V.viewFor({ unlocked: false, app_running: false }), 'appdown');
  assert.strictEqual(V.viewFor(null, { code: 'app_not_running' }), 'appdown');
  assert.strictEqual(V.viewFor(null, new Error('Specified native messaging host not found.')), 'nohost');
});

test('popup routes all use the popup: prefix and in-page routes are untouched', () => {
  const src = require('fs').readFileSync(require('path').join(__dirname, '..', 'popup', 'popup.js'), 'utf8');
  const types = [...src.matchAll(/send\(\{ type: '([^']+)'/g)].map((m) => m[1]);
  assert.ok(types.length > 10);
  for (const ty of types) assert.ok(ty.startsWith('popup:'), ty);
});
