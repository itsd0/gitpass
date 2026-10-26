import { create } from 'zustand';
import { en, type Key } from './en';
import { my } from './my';
import { th } from './th';
export type Lang = 'en' | 'my' | 'th';
export type { Key };
export const dictionaries: Record<Lang, Partial<Record<Key, string>>> = { en, my, th };
export const languages: { id: Lang; native: string }[] = [{ id: 'en', native: 'English' }, { id: 'my', native: 'မြန်မာ' }, { id: 'th', native: 'ไทย' }];
/** Pure translate: lang → English → key itself. `{name}` placeholders interpolated. */
export function translate(lang: Lang, key: Key | string, vars?: Record<string, string | number>): string {
  const s = dictionaries[lang]?.[key as Key] ?? (en as Record<string, string>)[key] ?? key;
  return vars ? s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : s;
}
export const useI18n = create<{ lang: Lang; setLang: (l: Lang) => void }>((set) => ({
  lang: 'en', setLang: (lang) => { set({ lang }); if (typeof document !== 'undefined') document.documentElement.lang = lang; },
}));
export function useT() { const lang = useI18n((s) => s.lang); return (key: Key, vars?: Record<string, string | number>) => translate(lang, key, vars); }
