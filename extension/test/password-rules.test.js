// lib/password-rules.js (#188): passwordrules parser, field reader, and the clamp/validate step used by bg-page.
const test = require('node:test');
const assert = require('node:assert');
const { JSDOM } = require('jsdom');
const PR = require('../lib/password-rules.js');

const kinds = (r) => r.entries.map((e) => `${e.required ? 'req' : 'ok'}:${e.kind}${e.chars ? '[' + e.chars + ']' : ''}`);

test('parse reads lengths, classes and custom symbol sets', () => {
  const r = PR.parse('minlength: 8; maxlength: 16; required: lower; required: upper; required: digit; allowed: [-_!]');
  assert.strictEqual(r.minlength, 8);
  assert.strictEqual(r.maxlength, 16);
  assert.deepStrictEqual(kinds(r), ['req:lower', 'req:upper', 'req:digit', 'ok:special[-_!]']);
});

test('parse: comma lists, special, required custom set, ascii-printable and unicode', () => {
  assert.deepStrictEqual(kinds(PR.parse('required: lower, upper; required: special')), ['req:lower', 'req:upper', 'req:special']);
  assert.deepStrictEqual(kinds(PR.parse('required: [$]; allowed: digit')), ['req:special[$]', 'ok:digit']);
  assert.deepStrictEqual(kinds(PR.parse('allowed: ascii-printable')), ['ok:lower', 'ok:upper', 'ok:digit', 'ok:special']);
  assert.strictEqual(PR.parse('allowed: unicode').entries.length, 4);
});

test('parse ignores malformed input safely', () => {
  for (const bad of [undefined, null, 42, '', ';;;', 'garbage', 'minlength: abc', 'minlength: -3; maxlength: 99999', 'required: ; allowed: [', 'required: [abc]; allowed: nope', 'a:b:c', {}, 'x'.repeat(100000)]) {
    const r = PR.parse(bad);
    assert.ok(Array.isArray(r.entries));
    assert.ok(r.minlength === undefined || Number.isInteger(r.minlength));
  }
  const r = PR.parse('minlength: abc; maxlength: 99999; required: [abc]; allowed: nope');
  assert.strictEqual(r.minlength, undefined);
  assert.strictEqual(r.maxlength, undefined);
  assert.deepStrictEqual(r.entries, []);
});

test('cleanSymbols keeps printable ASCII symbols only, deduplicated and capped', () => {
  assert.strictEqual(PR.cleanSymbols('-_-a1 é€!'), '-_!');
  assert.strictEqual(PR.cleanSymbols(null), '');
  assert.ok(PR.cleanSymbols('!@#$%^&*()-_=+[]{};:,.<>/?~`|\\"\'').length <= 32);
});

function field(html) {
  const { document } = new JSDOM(`<body>${html}</body>`).window;
  return document.getElementById('pw');
}

test('fromField: passwordrules on the field, then on the form; maxlength/minlength attributes', () => {
  let f = field('<form><input id="pw" type="password" maxlength="16" passwordrules="minlength: 8; maxlength: 20; allowed: [-_]"></form>');
  let r = PR.fromField(f);
  assert.strictEqual(r.maxlength, 16); // the tighter limit wins
  assert.strictEqual(r.minlength, 8);
  assert.deepStrictEqual(kinds(r), ['ok:special[-_]']);

  f = field('<form passwordrules="maxlength: 12"><input id="pw" type="password"></form>');
  assert.strictEqual(PR.fromField(f).maxlength, 12);

  f = field('<form><input id="pw" type="password" minlength="10"></form>');
  const m = PR.fromField(f);
  assert.deepStrictEqual({ min: m.minlength, max: m.maxlength }, { min: 10, max: undefined });

  assert.strictEqual(PR.fromField(field('<form><input id="pw" type="password"></form>')), null);
  assert.strictEqual(PR.fromField(field('<input id="pw" type="password" maxlength="-1">')), null);
  assert.strictEqual(PR.fromField(null), null);
});

test("sanitize: no rules keeps today's default (20 chars, digits and symbols required)", () => {
  const s = PR.sanitize({});
  assert.strictEqual(s.ok, true);
  assert.strictEqual(s.len, 20);
  assert.deepStrictEqual(s.classes.filter((c) => c.required).map((c) => c.kind).sort(), ['digit', 'special']);
  assert.deepStrictEqual(s.classes.map((c) => c.kind), ['lower', 'upper', 'digit', 'special']);
  assert.strictEqual(PR.sanitize(null).len, 20);
});

test('sanitize clamps length to 8..64 and to the site limits', () => {
  assert.strictEqual(PR.sanitize({ maxlength: 16 }).len, 16);
  assert.strictEqual(PR.sanitize({ maxlength: 8 }).len, 8);
  assert.strictEqual(PR.sanitize({ minlength: 30 }).len, 30);
  assert.strictEqual(PR.sanitize({ minlength: 64 }).len, 64);
  assert.strictEqual(PR.sanitize({ maxlength: 5000 }).len, 20);
  assert.strictEqual(PR.sanitize({ minlength: 3, maxlength: 12 }).len, 12);
  assert.strictEqual(PR.sanitize({ minlength: 'x', maxlength: -4 }).len, 20);
});

test('sanitize rejects rules that cannot hold a strong password', () => {
  assert.deepStrictEqual(PR.sanitize({ maxlength: 4 }), { ok: false, reason: 'too_short' });
  assert.deepStrictEqual(PR.sanitize({ maxlength: 7 }), { ok: false, reason: 'too_short' });
  assert.strictEqual(PR.sanitize({ minlength: 70 }).reason, 'unsatisfiable');
  assert.strictEqual(PR.sanitize({ minlength: 40, maxlength: 20 }).reason, 'unsatisfiable');
  const four = ['lower', 'upper', 'digit', 'special'].map((kind) => ({ kind, required: true }));
  assert.strictEqual(PR.sanitize({ entries: four, maxlength: 8 }).ok, true);
});

test('sanitize validates entries: unknown kinds and bad symbol sets are dropped', () => {
  const s = PR.sanitize({ entries: [{ kind: 'lower', required: true }, { kind: 'emoji', required: true }, { kind: 'special', chars: 'a1 é' }, { kind: 'special', chars: '-_', required: false }, null, 'x'] });
  assert.strictEqual(s.ok, true);
  assert.deepStrictEqual(s.classes, [{ kind: 'lower', required: true }, { kind: 'special', required: false, chars: '-_' }]);
  assert.strictEqual(PR.sanitize({ entries: 'nope' }).classes.length, 4);
  assert.strictEqual(PR.sanitize({ entries: Array.from({ length: 500 }, () => ({ kind: 'digit', required: false })) }).classes.length, 1);
});

test('sanitize: only required lines allow the other classes; allowed lines restrict the pool', () => {
  const onlyReq = PR.sanitize({ entries: [{ kind: 'lower', required: true }, { kind: 'digit', required: true }] });
  assert.deepStrictEqual(onlyReq.classes.map((c) => `${c.required ? 'r' : 'a'}:${c.kind}`), ['r:lower', 'r:digit', 'a:upper', 'a:special']);
  const restricted = PR.sanitize({ entries: PR.parse('required: lower; required: upper; required: digit; allowed: [-_]').entries });
  assert.deepStrictEqual(restricted.classes.map((c) => c.kind + (c.chars || '')), ['lower', 'upper', 'digit', 'special-_']);
});

test('summary describes length and symbol limits for the row subtitle', () => {
  const s = (raw) => PR.summary(PR.sanitize(raw));
  assert.deepStrictEqual(s({ maxlength: 16 }), { len: 16, noSymbols: false, limitedSymbols: false });
  assert.deepStrictEqual(s({ maxlength: 16, entries: [{ kind: 'lower' }, { kind: 'digit' }] }), { len: 16, noSymbols: true, limitedSymbols: false });
  assert.deepStrictEqual(s({ entries: [{ kind: 'special', chars: '-_' }] }), { len: 20, noSymbols: false, limitedSymbols: true });
  assert.strictEqual(PR.summary(PR.sanitize({ maxlength: 3 })), null);
});
