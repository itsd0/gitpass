import { describe, expect, it } from 'vitest';
import { formatRelativePasswordChanged, parseDate, type TranslateFn } from './passwordAge';
import { en } from '../../i18n/en';
import { my } from '../../i18n/my';
import { th } from '../../i18n/th';
import type { Key } from '../../i18n';

const createTranslate = (dict: Record<string, string>): TranslateFn => (key: Key, vars?: Record<string, string | number>) => {
  let s = dict[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      s = s.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return s;
};

const tEn = createTranslate(en as Record<string, string>);
const tMy = createTranslate(my as Record<string, string>);
const tTh = createTranslate(th as Record<string, string>);

describe('passwordAge', () => {
  const now = new Date('2026-10-28T12:00:00Z');

  it('parses iso string and unix timestamp string', () => {
    expect(parseDate('2026-10-28T12:00:00Z')?.toISOString()).toBe('2026-10-28T12:00:00.000Z');
    expect(parseDate('1600000000')?.getTime()).toBe(1600000000000);
    expect(parseDate('')).toBeNull();
  });

  it('formats relative password changed time in English', () => {
    // Today
    expect(formatRelativePasswordChanged('2026-10-28T10:00:00Z', now, tEn)).toBe('Password changed today');
    // 1 day ago
    expect(formatRelativePasswordChanged('2026-10-27T12:00:00Z', now, tEn)).toBe('Password changed 1 day ago');
    // 5 days ago
    expect(formatRelativePasswordChanged('2026-10-23T12:00:00Z', now, tEn)).toBe('Password changed 5 days ago');
    // 1 month ago
    expect(formatRelativePasswordChanged('2026-09-28T12:00:00Z', now, tEn)).toBe('Password changed 1 month ago');
    // 3 months ago
    expect(formatRelativePasswordChanged('2026-07-28T12:00:00Z', now, tEn)).toBe('Password changed 3 months ago');
    // 1 year ago
    expect(formatRelativePasswordChanged('2025-10-28T12:00:00Z', now, tEn)).toBe('Password changed 1 year ago');
    // 2 years ago
    expect(formatRelativePasswordChanged('2024-10-28T12:00:00Z', now, tEn)).toBe('Password changed 2 years ago');
  });

  it('formats relative password changed time in Burmese', () => {
    expect(formatRelativePasswordChanged('2026-10-28T10:00:00Z', now, tMy)).toBe('စကားဝှက်ကို ယနေ့ ပြောင်းလဲခဲ့သည်');
    expect(formatRelativePasswordChanged('2026-10-27T12:00:00Z', now, tMy)).toBe('စကားဝှက်ကို လွန်ခဲ့သော ၁ ရက်က ပြောင်းလဲခဲ့သည်');
    expect(formatRelativePasswordChanged('2026-10-23T12:00:00Z', now, tMy)).toBe('စကားဝှက်ကို လွန်ခဲ့သော 5 ရက်က ပြောင်းလဲခဲ့သည်');
    expect(formatRelativePasswordChanged('2025-10-28T12:00:00Z', now, tMy)).toBe('စကားဝှက်ကို လွန်ခဲ့သော ၁ နှစ်က ပြောင်းလဲခဲ့သည်');
  });

  it('formats relative password changed time in Thai', () => {
    expect(formatRelativePasswordChanged('2026-10-28T10:00:00Z', now, tTh)).toBe('เปลี่ยนรหัสผ่านวันนี้');
    expect(formatRelativePasswordChanged('2026-10-27T12:00:00Z', now, tTh)).toBe('เปลี่ยนรหัสผ่านเมื่อ 1 วันที่แล้ว');
    expect(formatRelativePasswordChanged('2026-10-23T12:00:00Z', now, tTh)).toBe('เปลี่ยนรหัสผ่านเมื่อ 5 วันที่แล้ว');
    expect(formatRelativePasswordChanged('2025-10-28T12:00:00Z', now, tTh)).toBe('เปลี่ยนรหัสผ่านเมื่อ 1 ปีที่แล้ว');
  });

  it('handles invalid or empty inputs safely', () => {
    expect(formatRelativePasswordChanged('', now, tEn)).toBe('');
    expect(formatRelativePasswordChanged('invalid-date', now, tEn)).toBe('');
  });
});
