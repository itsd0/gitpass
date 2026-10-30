import { describe, expect, it, vi } from 'vitest';
import { builtinCommands } from './commands';
import { useApp } from '../../lib/store';
import { useSettings } from '../settings/useSettings';
import { api } from '../../api/client';

describe('palette builtin commands with selection', () => {
  it('does not include move-to-vault and add-tag when nothing is selected', () => {
    useApp.setState({ selectedItemId: null, multiIds: [] });
    const cmds = builtinCommands();
    expect(cmds.some((c) => c.id === 'move-to-vault')).toBe(false);
    expect(cmds.some((c) => c.id === 'add-tag')).toBe(false);
  });

  it('includes move-to-vault and add-tag when single item is selected', () => {
    useApp.setState({ selectedItemId: 'item-1', multiIds: [] });
    const cmds = builtinCommands();
    expect(cmds.some((c) => c.id === 'move-to-vault')).toBe(true);
    expect(cmds.some((c) => c.id === 'add-tag')).toBe(true);
    const moveCmd = cmds.find((c) => c.id === 'move-to-vault');
    expect(moveCmd?.label).toBe('Move to vault…');
    const tagCmd = cmds.find((c) => c.id === 'add-tag');
    expect(tagCmd?.label).toBe('Add tag…');
  });

  it('includes move-to-vault and add-tag when multi-selection is active', () => {
    useApp.setState({ selectedItemId: 'item-1', multiIds: ['item-1', 'item-2'] });
    const cmds = builtinCommands();
    expect(cmds.some((c) => c.id === 'move-to-vault')).toBe(true);
    expect(cmds.some((c) => c.id === 'add-tag')).toBe(true);
  });

  it('toggle-list-density flips the saved density', async () => {
    const base = { autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark' as const, language: 'en' as const, biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false, listDensity: 'comfortable' as const };
    useSettings.setState({ settings: { ...base }, error: null });
    vi.spyOn(api, 'setSettings').mockImplementation(async ({ settings }) => { useSettings.setState({ settings, error: null }); return settings; });
    const cmd = builtinCommands().find((c) => c.id === 'toggle-list-density');
    expect(cmd?.label).toBe('Toggle compact list');
    await cmd?.run();
    expect(useSettings.getState().settings?.listDensity).toBe('compact');
    await builtinCommands().find((c) => c.id === 'toggle-list-density')?.run();
    expect(useSettings.getState().settings?.listDensity).toBe('comfortable');
    vi.restoreAllMocks();
  });

  it.each(['deleted', 'archive'] as const)('hides move-to-vault and add-tag in %s', (kind) => {
    useApp.setState({ route: { name: 'items', filter: { kind } } as any, selectedItemId: 'item-1', multiIds: ['item-1', 'item-2'] });
    const cmds = builtinCommands();
    expect(cmds.some((c) => c.id === 'move-to-vault')).toBe(false);
    expect(cmds.some((c) => c.id === 'add-tag')).toBe(false);
    useApp.setState({ route: { name: 'items', filter: { kind: 'all' } } as any });
  });
});
