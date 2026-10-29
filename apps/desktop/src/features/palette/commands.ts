import { FolderInput, Home, KeyRound, Lock, Maximize2, Moon, Plus, RefreshCw, Settings, ShieldCheck, Tag, Users, Keyboard, Zap } from 'lucide-react';
import { api } from '../../api/client';
import { translate, useI18n, type Key } from '../../i18n';
import type { PaletteCommand } from '../registry';
import { useApp } from '../../lib/store';
import { useItemsUI } from '../items/state';
import { toast } from '../../ui';
import { isDark } from '../settings/theme';
import { useSettings } from '../settings/useSettings';
export type { PaletteCommand };
export const openShortcutsHelp = () => useApp.getState().openModal('shortcuts-help');
export function builtinCommands(): PaletteCommand[] {
  const app = useApp.getState(); const lang = useI18n.getState().lang;
  const cmds: (Omit<PaletteCommand, 'label'> & { labelKey: Key })[] = [
    { id: 'open-quick-access', labelKey: 'cmd.openQuickAccess', icon: Zap, keywords: ['quick', 'access', 'spotlight', 'floating', 'hotkey'], run: () => { if ((window as any).__TAURI_INTERNALS__) void api.showQuickAccess(); else useApp.getState().setRoute({ name: 'quick-access' }); } },
    { id: 'new-item', labelKey: 'cmd.newItem', icon: Plus, keywords: ['create', 'add'], shortcut: 'mod+n', run: () => app.openModal('new-item') },
    { id: 'generate-password', labelKey: 'cmd.generatePassword', icon: KeyRound, keywords: ['generator', 'pin', 'words', 'random'], shortcut: 'mod+shift+g', run: () => app.openModal('password-generator') },
    { id: 'lock', labelKey: 'cmd.lock', icon: Lock, shortcut: 'mod+shift+l', run: async () => { await api.lock().catch(() => {}); app.lock(); } },
    { id: 'go-home', labelKey: 'cmd.home', icon: Home, run: () => app.setRoute({ name: 'home' }) },
    { id: 'go-watchtower', labelKey: 'cmd.watchtower', icon: ShieldCheck, keywords: ['security', 'audit'], run: () => app.setRoute({ name: 'watchtower' }) },
    { id: 'go-settings', labelKey: 'cmd.settings', icon: Settings, keywords: ['preferences'], shortcut: 'mod+,', run: () => app.setRoute({ name: 'settings' }) },
    { id: 'go-people', labelKey: 'cmd.people', icon: Users, keywords: ['share', 'members'], run: () => app.setRoute({ name: 'people' }) },
    { id: 'toggle-theme', labelKey: 'cmd.toggleTheme', icon: Moon, keywords: ['dark', 'light'], run: async () => { const s = useSettings.getState(); if (!s.settings) await s.load(); await s.save({ theme: isDark() ? 'light' : 'dark' }).catch((e) => toast(String(e), 'danger')); } },
    { id: 'sync', labelKey: 'cmd.sync', icon: RefreshCw, keywords: ['git', 'push', 'pull'], run: async () => { try { toast((await api.sync({ merge: false })).message); } catch (e) { toast(String((e as Error).message ?? e), 'danger'); } } },
    { id: 'shortcuts', labelKey: 'cmd.shortcuts', icon: Keyboard, keywords: ['help', 'keys'], shortcut: '?', run: openShortcutsHelp },
  ];
  const hasSelection = Boolean(app.selectedItemId || app.multiIds.length > 0);
  // No move/tag for items in Recently Deleted or Archive: restore them first.
  const kind = app.route.name === 'items' ? app.route.filter.kind : null;
  const movable = kind !== 'deleted' && kind !== 'archive';
  if (hasSelection && movable) {
    cmds.push({
      id: 'move-to-vault',
      labelKey: 'cmd.moveToVault',
      icon: FolderInput,
      keywords: ['move', 'vault', 'transfer'],
      run: () => app.openModal('move-to-vault'),
    });
    cmds.push({
      id: 'add-tag',
      labelKey: 'cmd.addTag',
      icon: Tag,
      keywords: ['tag', 'label'],
      run: () => app.openModal('add-tag'),
    });
  }
  if (app.selectedItemId) {
    cmds.push({
      id: 'show-large-type',
      labelKey: 'cmd.showLargeType',
      icon: Maximize2,
      keywords: ['large', 'type', 'password', 'secret', 'zoom'],
      run: async () => {
        try {
          const selId = useApp.getState().selectedItemId;
          if (!selId) return;
          const items = useItemsUI.getState().items;
          let vaultId = items.find((x) => x.id === selId)?.vaultId;
          if (!vaultId) {
            const all = await api.listItems({ filter: { kind: 'all' } }).catch(() => []);
            vaultId = all.find((x) => x.id === selId)?.vaultId;
          }
          if (!vaultId) return;
          const detail = await api.getItem({ vaultId, itemId: selId });
          const allFields = detail.sections.flatMap((s) => s.fields);
          const field = allFields.find((f) => f.kind === 'concealed' && /password/i.test(f.label))
            ?? allFields.find((f) => f.kind === 'concealed');
          if (!field) {
            toast('No concealed field on this item');
            return;
          }
          const val = await api.revealField({ vaultId: detail.vaultId, itemId: detail.id, fieldId: field.id });
          useApp.getState().openModal('large-type', { title: field.label, value: val });
        } catch (e) {
          toast(String((e as Error)?.message ?? e), 'danger');
        }
      },
    });
  }
  return cmds.map((c) => ({ ...c, label: translate(lang, c.labelKey) }));
}
