import { Home, Lock, Moon, Plus, RefreshCw, Settings, ShieldCheck, Users, Keyboard } from 'lucide-react';
import { api } from '../../api/client';
import { translate, useI18n, type Key } from '../../i18n';
import type { PaletteCommand } from '../registry';
import { useApp } from '../../lib/store';
import { toast } from '../../ui';
import { isDark } from '../settings/theme';
import { useSettings } from '../settings/useSettings';
export type { PaletteCommand };
export const openShortcutsHelp = () => useApp.getState().openModal('shortcuts-help');
export function builtinCommands(): PaletteCommand[] {
  const app = useApp.getState(); const lang = useI18n.getState().lang;
  const cmds: (Omit<PaletteCommand, 'label'> & { labelKey: Key })[] = [
    { id: 'new-item', labelKey: 'cmd.newItem', icon: Plus, keywords: ['create', 'add'], shortcut: 'mod+n', run: () => app.openModal('new-item') },
    { id: 'lock', labelKey: 'cmd.lock', icon: Lock, shortcut: 'mod+shift+l', run: async () => { await api.lock().catch(() => {}); app.lock(); } },
    { id: 'go-home', labelKey: 'cmd.home', icon: Home, run: () => app.setRoute({ name: 'home' }) },
    { id: 'go-watchtower', labelKey: 'cmd.watchtower', icon: ShieldCheck, keywords: ['security', 'audit'], run: () => app.setRoute({ name: 'watchtower' }) },
    { id: 'go-settings', labelKey: 'cmd.settings', icon: Settings, keywords: ['preferences'], shortcut: 'mod+,', run: () => app.setRoute({ name: 'settings' }) },
    { id: 'go-people', labelKey: 'cmd.people', icon: Users, keywords: ['share', 'members'], run: () => app.setRoute({ name: 'people' }) },
    { id: 'toggle-theme', labelKey: 'cmd.toggleTheme', icon: Moon, keywords: ['dark', 'light'], run: async () => { const s = useSettings.getState(); if (!s.settings) await s.load(); await s.save({ theme: isDark() ? 'light' : 'dark' }).catch((e) => toast(String(e), 'danger')); } },
    { id: 'sync', labelKey: 'cmd.sync', icon: RefreshCw, keywords: ['git', 'push', 'pull'], run: async () => { try { toast((await api.sync({ merge: false })).message); } catch (e) { toast(String((e as Error).message ?? e), 'danger'); } } },
    { id: 'shortcuts', labelKey: 'cmd.shortcuts', icon: Keyboard, keywords: ['help', 'keys'], shortcut: '?', run: openShortcutsHelp },
  ];
  return cmds.map((c) => ({ ...c, label: translate(lang, c.labelKey) }));
}
