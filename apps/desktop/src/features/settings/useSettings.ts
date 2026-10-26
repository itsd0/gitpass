import { create } from 'zustand';
import { api } from '../../api/client';
import type { Settings } from '../../api/types';
import { useI18n } from '../../i18n';
import { applyTheme } from './theme';
interface S { settings: Settings | null; error: string | null; load: () => Promise<void>; save: (patch: Partial<Settings>) => Promise<Settings> }
/** Settings are persisted by Rust only (api.setSettings); this store just mirrors the last saved value. */
export const useSettings = create<S>((set, get) => ({
  settings: null, error: null,
  load: async () => { try { const s = await api.getSettings(); set({ settings: s, error: null }); applyTheme(s.theme); useI18n.getState().setLang(s.language); } catch (e) { set({ error: String((e as Error)?.message ?? e) }); } },
  save: async (patch) => { const cur = get().settings; if (!cur) throw new Error('settings not loaded');
    const saved = await api.setSettings({ settings: { ...cur, ...patch } }); set({ settings: saved }); applyTheme(saved.theme); useI18n.getState().setLang(saved.language); return saved; },
}));
