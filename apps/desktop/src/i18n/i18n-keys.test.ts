import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import { en } from './en';
import { my } from './my';
import { th } from './th';

// Keys built at runtime are matched by prefix.
const DYNAMIC_PREFIXES = [
  'bulk.toast.',
  'category.',
  'generator.mode.',
  'itemKeys.',
  'settings.',
  'settings.theme.',
  'watchtower.score.',
];

function sourceFiles(dir: string, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (['node_modules', 'dist', '.git'].includes(e.name)) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      sourceFiles(p, out);
    } else if (
      /\.(tsx?|jsx?|html)$/.test(e.name) &&
      !p.endsWith('/en.ts') &&
      !p.endsWith('/my.ts') &&
      !p.endsWith('/th.ts') &&
      !p.endsWith('/i18n-keys.test.ts')
    ) {
      out.push(p);
    }
  }
  return out;
}

describe('i18n-keys', () => {
  it('my and th have exactly the same keys as en', () => {
    const base = Object.keys(en).sort();
    for (const [loc, dict] of [
      ['my', my],
      ['th', th],
    ] as const) {
      const keys = Object.keys(dict).sort();
      expect(base.filter((k) => !keys.includes(k)), `${loc} is missing keys`).toEqual([]);
      expect(keys.filter((k) => !base.includes(k)), `${loc} has keys not in en`).toEqual([]);
    }
  });

  it('every en key is referenced in desktop src (no dead strings)', () => {
    const srcDir = path.resolve(__dirname, '..');
    const text = sourceFiles(srcDir)
      .map((f) => fs.readFileSync(f, 'utf8'))
      .join('\n');
    const unused = Object.keys(en).filter((k) => {
      if (DYNAMIC_PREFIXES.some((p) => k.startsWith(p))) return false;
      const escaped = k.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      return !new RegExp(`(?<![A-Za-z0-9_.-])${escaped}(?![A-Za-z0-9_.-])`).test(text);
    });
    expect(unused, 'unused locale keys: remove them from en/my/th or wire them up').toEqual([]);
  });

  it('dynamic-prefix allowlist still matches real lookups', () => {
    const srcDir = path.resolve(__dirname, '..');
    const text = sourceFiles(srcDir)
      .map((f) => fs.readFileSync(f, 'utf8'))
      .join('\n');
    for (const p of DYNAMIC_PREFIXES) {
      expect(text.includes(p), `${p} no longer used dynamically; drop it from the allowlist`).toBe(true);
    }
  });
});
