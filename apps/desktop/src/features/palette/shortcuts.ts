import { useEffect } from 'react';
/** Global shortcut registry. combo syntax: "mod+k", "shift+?", "?" ("mod" = Cmd on macOS, Ctrl elsewhere). */
export interface Shortcut { id: string; combo: string; labelKey: string; run: () => void; allowInInput?: boolean }
const registry = new Map<string, Shortcut>(); const subs = new Set<() => void>();
export const isMac = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
export function registerShortcut(s: Shortcut) { registry.set(s.id, s); subs.forEach((f) => f()); return () => { registry.delete(s.id); subs.forEach((f) => f()); }; }
export const listShortcuts = () => [...registry.values()];
export function matches(combo: string, e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>, mac = isMac()) {
  const parts = combo.toLowerCase().split('+'); const key = parts.pop()!;
  const wantMod = parts.includes('mod'); const mod = mac ? e.metaKey : e.ctrlKey;
  if (wantMod !== mod) return false; if (parts.includes('alt') !== e.altKey) return false;
  if (key !== '?' && parts.includes('shift') !== e.shiftKey) return false;
  return e.key.toLowerCase() === key;
}
export function formatCombo(combo: string) { return combo.split('+').map((p) => p === 'mod' ? (isMac() ? '⌘' : 'Ctrl') : p === 'shift' ? (isMac() ? '⇧' : 'Shift') : p === 'enter' ? '↵' : p.toUpperCase()); }
function isTyping(t: EventTarget | null) { const el = t as HTMLElement | null; return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable); }
export function useGlobalShortcuts() {
  useEffect(() => { const h = (e: KeyboardEvent) => { for (const s of registry.values()) if (matches(s.combo, e) && (s.allowInInput || !isTyping(e.target))) { e.preventDefault(); s.run(); return; } };
    window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h); }, []);
}
export function useShortcut(s: Shortcut, deps: unknown[] = []) { useEffect(() => registerShortcut(s), deps); } // eslint-disable-line react-hooks/exhaustive-deps
