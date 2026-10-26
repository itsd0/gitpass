import type { Settings } from '../../api/types';
import { useApp } from '../../lib/store';
export function applyTheme(theme: Settings['theme']) {
  if (typeof document === 'undefined') return;
  const dark = theme === 'dark' || (theme === 'system' && window.matchMedia?.('(prefers-color-scheme: dark)').matches);
  document.documentElement.classList.toggle('dark', !!dark);
  if (useApp.getState().theme !== (dark ? 'dark' : 'light')) useApp.getState().setTheme(dark ? 'dark' : 'light'); // keep shell toggle in sync
}
export const isDark = () => typeof document !== 'undefined' && document.documentElement.classList.contains('dark');
