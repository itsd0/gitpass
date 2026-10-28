import { describe, expect, it } from 'vitest';
import { builtinCommands } from './commands';
import { useApp } from '../../lib/store';

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
});
