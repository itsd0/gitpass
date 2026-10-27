import { describe, expect, it } from 'vitest';
import { dictionaries, translate } from '.';
import { en } from './en';
describe('i18n', () => {
  it('translates present keys', () => { expect(translate('th', 'nav.settings')).toBe('การตั้งค่า'); expect(translate('my', 'nav.settings')).toBe('ဆက်တင်များ'); });
  it('falls back to English when a key is missing in the language', () => { expect(dictionaries.my['about.license']).toBeUndefined(); expect(translate('my', 'about.license')).toBe(en['about.license']); });
  it('falls back to the key when unknown everywhere', () => { expect(translate('th', 'does.not.exist')).toBe('does.not.exist'); });
  it('interpolates variables', () => { expect(translate('en', 'share.done', { name: 'Alex' })).toBe('Vault shared with Alex'); expect(translate('th', 'sync.ahead', { n: 3 })).toBe('นำหน้า 3'); });
  it('translations only use known keys and keep placeholders', () => {
    for (const d of [dictionaries.my, dictionaries.th]) for (const [k, v] of Object.entries(d)) {
      expect(k in en).toBe(true); const ph = ((en as Record<string, string>)[k]!.match(/\{\w+\}/g) ?? []).sort(); expect((v!.match(/\{\w+\}/g) ?? []).sort()).toEqual(ph); }
  });
  it('covers all common shell strings in my and th', () => {
    const shell = Object.keys(en).filter((k) => k.startsWith('nav.'));
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of shell) expect(d[k as keyof typeof en]).toBeTruthy();
  });
  it('covers all kit.* strings in my and th', () => {
    const kitKeys = Object.keys(en).filter((k) => k.startsWith('kit.'));
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of kitKeys) expect(d[k as keyof typeof en]).toBeTruthy();
  });
});
