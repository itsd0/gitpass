import { describe, expect, it } from 'vitest';
import { dictionaries, translate } from '.';
import { en } from './en';
describe('i18n', () => {
  it('translates present keys', () => { expect(translate('th', 'nav.settings')).toBe('การตั้งค่า'); expect(translate('my', 'nav.settings')).toBe('ဆက်တင်များ'); });
  it('falls back to English when a key is missing in the language', () => { expect(dictionaries.my['about.license']).toBeUndefined(); expect(translate('my', 'about.license')).toBe(en['about.license']); });
  it('falls back to the key when unknown everywhere', () => { expect(translate('th', 'does.not.exist')).toBe('does.not.exist'); });
  it('interpolates variables', () => { expect(translate('en', 'share.done', { name: 'Alex' })).toBe('Vault shared with Alex'); expect(translate('th', 'sync.ahead', { n: 3 })).toBe('นำหน้า 3'); });
  it('localizes the change-password label with the item title', () => {
    expect(translate('en', 'watchtower.changePasswordFor', { title: 'Fake Bank' })).toBe('Change password for Fake Bank');
    expect(translate('th', 'watchtower.changePasswordFor', { title: 'Fake Bank' })).toBe('เปลี่ยนรหัสผ่านสำหรับ Fake Bank');
    expect(translate('my', 'watchtower.changePasswordFor', { title: 'Fake Bank' })).toBe('Fake Bank အတွက် စကားဝှက် ပြောင်းရန်');
  });
  it('translations only use known keys and keep placeholders', () => {
    for (const d of [dictionaries.my, dictionaries.th]) for (const [k, v] of Object.entries(d)) {
      expect(k in en).toBe(true); const ph = ((en as Record<string, string>)[k]!.match(/\{\w+\}/g) ?? []).sort(); expect((v!.match(/\{\w+\}/g) ?? []).sort()).toEqual(ph); }
  });
  it('covers all common shell strings in my and th', () => {
    const shell = Object.keys(en).filter((k) => k.startsWith('nav.'));
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of shell) expect(d[k as keyof typeof en]).toBeTruthy();
  });
  it('covers all detail.* strings in my and th', () => {
    const detailKeys = Object.keys(en).filter((k) => k.startsWith('detail.'));
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of detailKeys) {
        expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
      }
    }
  });
  it('covers all kit.* strings in my and th', () => {
    const kitKeys = Object.keys(en).filter((k) => k.startsWith('kit.'));
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of kitKeys) expect(d[k as keyof typeof en]).toBeTruthy();
  });
  it('covers all settings search and section strings in my and th', () => {
    const keys = Object.keys(en).filter((k) => k.startsWith('settings.section') || k.startsWith('settings.search') || k.endsWith('Help'));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of keys) expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
  });
  it('covers all trash.* strings in my and th', () => {
    const trashKeys = Object.keys(en).filter((k) => k.startsWith('trash.'));
    expect(trashKeys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of trashKeys) expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
  });
  it("covers all watchtower.weakReason.* strings in my and th", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("watchtower.weakReason."));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) {
        expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
      }
    }
  });
  it("covers all items.empty.* and category.* strings in my and th", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("items.empty.") || k.startsWith("category."));
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) {
        expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
      }
    }
  });
  it("covers all unlock.* and sidebar.* strings in my and th", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("unlock.") || k.startsWith("sidebar."));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) {
        expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
      }
    }
  });
  it("covers all items.search.* strings in my and th", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("items.search."));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
    }
  });
  it('covers all import.* strings in my and th', () => {
    const importKeys = Object.keys(en).filter((k) => k.startsWith('import.'));
    expect(importKeys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of importKeys) expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
  });
  it('covers the new home vault, watchtower score and breach-check strings in en/my/th', () => {
    const keys = ['home.vault.all', 'home.vault.open', 'home.vault.itemOne', 'home.vault.items', 'home.vault.allDesc', 'home.vault.sharedDesc', 'home.vault.privateDesc', 'home.vault.settingsAria', 'home.vault.openAria', 'vault.confirmNameAria', 'watchtower.score.excellent', 'watchtower.score.good', 'watchtower.score.fair', 'watchtower.score.poor', 'watchtower.score.aria', 'watchtower.breachCheckAria', 'watchtower.breachCheckLabel'] as const;
    for (const k of keys) {
      expect(en[k]).toBeTruthy();
      expect(translate('my', k)).not.toBe(k); expect(translate('th', k)).not.toBe(k);
    }
    expect(translate('my', 'home.vault.items', { count: 3 })).toBe('item 3 ခု');
    expect(translate('th', 'watchtower.score.aria', { score: 71, label: 'ดี' })).toBe('คะแนนความปลอดภัย 71 จาก 100, ดี');
  });
  it("covers all strength.*, field.* and totp.* strings in my and th", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("strength.") || k.startsWith("field.") || k.startsWith("totp."));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) {
        expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
      }
    }
  });
  it("covers all vault.* strings in my and th", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("vault."));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
    }
  });
  it("covers all generator.* strings in my and th", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("generator."));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
    }
  });
  it("covers all wifi.* strings in my and th and category.wifi unification", () => {
    const keys = Object.keys(en).filter((k) => k.startsWith("wifi."));
    expect(keys.length).toBeGreaterThan(0);
    for (const d of [dictionaries.my, dictionaries.th]) {
      for (const k of keys) expect(d[k as keyof typeof en], `missing key ${k}`).toBeTruthy();
    }
    expect(en['category.wifi']).toBe('Wi-Fi');
    expect(dictionaries.my['category.wifi']).toBe('Wi-Fi');
    expect(dictionaries.th['category.wifi']).toBe('Wi-Fi');
  });
});
