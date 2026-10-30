import { useEffect } from 'react';
import { create } from 'zustand';

/** Text size steps, smallest to largest. The root font size is scaled, so rem-based text and spacing follow. */
export const TEXT_SIZES = ['smaller', 'default', 'larger', 'large', 'largest'] as const;
export type TextSize = (typeof TEXT_SIZES)[number];
/** Percent of the browser's default root font size for each step. */
export const TEXT_SIZE_PERCENT: Record<TextSize, number> = { smaller: 87.5, default: 100, larger: 112.5, large: 125, largest: 137.5 };
/** Per-device preference (localStorage, shared by the main and Quick Access windows); never written to the vault. */
export const TEXT_SIZE_KEY = 'gitpass.textSize';

export const isTextSize = (v: unknown): v is TextSize => typeof v === 'string' && (TEXT_SIZES as readonly string[]).includes(v);

export function loadTextSize(): TextSize {
  try { const raw = localStorage.getItem(TEXT_SIZE_KEY); return isTextSize(raw) ? raw : 'default'; } catch { return 'default'; }
}

export function applyTextSize(size: TextSize) {
  if (typeof document === 'undefined') return;
  document.documentElement.style.fontSize = size === 'default' ? '' : `${TEXT_SIZE_PERCENT[size]}%`;
}

interface S { size: TextSize; set: (size: TextSize) => void; step: (delta: 1 | -1) => void; reset: () => void }
export const useTextSize = create<S>((set, get) => ({
  size: loadTextSize(),
  set: (size) => {
    try { if (size === 'default') localStorage.removeItem(TEXT_SIZE_KEY); else localStorage.setItem(TEXT_SIZE_KEY, size); } catch { /* storage unavailable: keep in memory */ }
    applyTextSize(size); set({ size });
  },
  step: (delta) => { const i = TEXT_SIZES.indexOf(get().size) + delta; get().set(TEXT_SIZES[Math.min(TEXT_SIZES.length - 1, Math.max(0, i))]!); },
  reset: () => get().set('default'),
}));

/** Maps ⌘+ / ⌘= (larger), ⌘− (smaller) and ⌘0 (reset) to an action; Ctrl instead of ⌘ off macOS. */
export function textSizeKeyAction(e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey'>, mac: boolean): 'larger' | 'smaller' | 'reset' | null {
  if (e.altKey || (mac ? !e.metaKey || e.ctrlKey : !e.ctrlKey || e.metaKey)) return null;
  if (e.key === '=' || e.key === '+') return 'larger';
  if (e.key === '-' || e.key === '_') return 'smaller';
  if (e.key === '0') return 'reset';
  return null;
}

/** Mount once at the app root: applies the saved size, handles the shortcuts, and follows changes from other windows. */
export function useTextSizeShortcuts() {
  useEffect(() => {
    applyTextSize(useTextSize.getState().size);
    const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
    const onKey = (e: KeyboardEvent) => {
      const a = textSizeKeyAction(e, mac); if (!a) return;
      e.preventDefault();
      const s = useTextSize.getState();
      if (a === 'larger') s.step(1); else if (a === 'smaller') s.step(-1); else s.reset();
    };
    const onStorage = (e: StorageEvent) => {
      if (e.key !== null && e.key !== TEXT_SIZE_KEY) return;
      const size = loadTextSize(); applyTextSize(size); useTextSize.setState({ size });
    };
    window.addEventListener('keydown', onKey); window.addEventListener('storage', onStorage);
    return () => { window.removeEventListener('keydown', onKey); window.removeEventListener('storage', onStorage); };
  }, []);
}
