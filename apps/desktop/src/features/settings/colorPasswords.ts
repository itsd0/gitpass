import { create } from 'zustand';

/** Per-device preference (localStorage, like Text size); never written to the vault. On unless explicitly turned off. */
export const COLOR_PASSWORDS_KEY = 'gitpass.colorPasswords';

export function loadColorPasswords(): boolean {
  try { return localStorage.getItem(COLOR_PASSWORDS_KEY) !== 'off'; } catch { return true; }
}

interface S { enabled: boolean; set: (enabled: boolean) => void }
export const useColorPasswords = create<S>((set) => ({
  enabled: loadColorPasswords(),
  set: (enabled) => {
    try { if (enabled) localStorage.removeItem(COLOR_PASSWORDS_KEY); else localStorage.setItem(COLOR_PASSWORDS_KEY, 'off'); } catch { /* storage unavailable: keep in memory */ }
    set({ enabled });
  },
}));
