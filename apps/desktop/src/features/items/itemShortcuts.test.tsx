import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../../api/client';
import type { ItemDetail, ItemSummary } from '../../api/types';
import { translate, useI18n } from '../../i18n';
import { useApp } from '../../lib/store';
import { useToasts } from '../../ui';
import { PaletteHost, ShortcutsHelp } from '../palette/CommandPalette';
import { builtinCommands } from '../palette/commands';
import { formatCombo, matches } from '../palette/shortcuts';
import { useItemsUI } from './state';

const FAKE = 'FAKE-never-in-state';
const item: ItemSummary = { id: 'i1', vaultId: 'v1', category: 'login', title: 'Fake Mail', subtitle: 'fake@example.invalid', favorite: false, archived: false,
  deletedAt: null, tags: [], updatedAt: '2026-10-01T00:00:00Z', createdAt: '2026-10-01T00:00:00Z', url: 'https://example.invalid' };
const detail = (over: Partial<ItemDetail> = {}): ItemDetail => ({ ...item, hasTotp: true, notes: null, attachments: [], history: [], sections: [{ id: 's', title: null, fields: [
  { id: 'fu', label: 'username', kind: 'text', value: 'fake@example.invalid' },
  { id: 'fp', label: 'password', kind: 'concealed', value: null },
  { id: 'ft', label: 'one-time password', kind: 'totp', value: null },
  { id: 'fw', label: 'website', kind: 'url', value: 'https://example.invalid/login' } ] }], ...over });
const toasts = () => useToasts.getState().list.map((x) => x.title);
const key = (target: Element | Window, init: KeyboardEventInit) => fireEvent.keyDown(target, init);

let copyField: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  useI18n.setState({ lang: 'en' });
  useToasts.setState({ list: [] });
  useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, selectedItemId: 'i1', multiIds: [], modal: null, editing: false });
  useItemsUI.setState({ items: [item] });
  vi.spyOn(api, 'getItem').mockImplementation(async () => detail());
  copyField = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
  render(<PaletteHost />);
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); window.getSelection()?.removeAllRanges(); });

describe('selected-item shortcuts', () => {
  it('⌘C copies the password through the pasteboard API and toasts, keeping no secret in state', async () => {
    key(window, { key: 'c', ctrlKey: true });
    await waitFor(() => expect(copyField).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'i1', fieldId: 'fp' }));
    await waitFor(() => expect(toasts().join()).toContain('Password copied'));
    expect(JSON.stringify(useApp.getState())).not.toContain(FAKE);
  });

  it('⌘C is not intercepted inside an input or with a text selection', async () => {
    const input = document.body.appendChild(document.createElement('input'));
    expect(fireEvent.keyDown(input, { key: 'c', ctrlKey: true })).toBe(true); // not default-prevented
    input.remove();
    const p = document.body.appendChild(document.createElement('p')); p.textContent = 'fake selected text';
    const range = document.createRange(); range.selectNodeContents(p); window.getSelection()?.addRange(range);
    expect(fireEvent.keyDown(document.body, { key: 'c', ctrlKey: true })).toBe(true);
    p.remove();
    await Promise.resolve();
    expect(copyField).not.toHaveBeenCalled();
  });

  it('⌘⇧C copies the username, or says there is none', async () => {
    key(window, { key: 'C', ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(copyField).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'i1', fieldId: 'fu' }));
    await waitFor(() => expect(toasts().join()).toContain('Username copied'));
    vi.spyOn(api, 'getItem').mockImplementation(async () => detail({ sections: [{ id: 's', title: null, fields: [{ id: 'fp', label: 'password', kind: 'concealed', value: null }] }] }));
    key(window, { key: 'C', ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(toasts().join()).toContain('No username'));
    expect(copyField).toHaveBeenCalledTimes(1);
  });

  it('⌥⌘C copies the one-time code', async () => {
    key(window, { key: 'ç', code: 'KeyC', ctrlKey: true, altKey: true });
    await waitFor(() => expect(copyField).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'i1', fieldId: 'ft' }));
    await waitFor(() => expect(toasts().join()).toContain('One-time code copied'));
  });

  it('⌘E opens the editor, except for items in Recently Deleted', () => {
    key(window, { key: 'e', ctrlKey: true });
    expect(useApp.getState().editing).toBe(true);
    useApp.setState({ editing: false });
    useItemsUI.setState({ items: [{ ...item, deletedAt: '2026-10-20T00:00:00Z' }] });
    key(window, { key: 'e', ctrlKey: true });
    expect(useApp.getState().editing).toBe(false);
  });

  it('copy shortcuts still work for items in Recently Deleted', async () => {
    useItemsUI.setState({ items: [{ ...item, deletedAt: '2026-10-20T00:00:00Z' }] });
    key(window, { key: 'c', ctrlKey: true });
    await waitFor(() => expect(copyField).toHaveBeenCalledTimes(1));
  });

  it('⌘⇧O opens the first website link', async () => {
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    key(window, { key: 'O', ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(click).toHaveBeenCalled());
    const a = click.mock.contexts[0] as HTMLAnchorElement;
    expect(a.href).toBe('https://example.invalid/');
    expect(a.rel).toContain('noopener');
  });

  it('does nothing with no selection, in multi-select, or while a dialog or the editor is open', async () => {
    for (const s of [{ selectedItemId: null }, { multiIds: ['i1', 'i2'] }, { modal: { id: 'new-item' } }, { editing: true }] as Partial<ReturnType<typeof useApp.getState>>[]) {
      useApp.setState({ selectedItemId: 'i1', multiIds: [], modal: null, editing: false, ...s });
      key(window, { key: 'c', ctrlKey: true }); key(window, { key: 'C', ctrlKey: true, shiftKey: true });
      key(window, { key: 'e', ctrlKey: true }); key(window, { key: 'ç', code: 'KeyC', ctrlKey: true, altKey: true });
    }
    await Promise.resolve();
    expect(copyField).not.toHaveBeenCalled();
    expect(useApp.getState().editing).toBe(true); // only the pre-set `editing: true` case
  });
});

describe('shortcut sheet and palette', () => {
  it('lists the new shortcuts in the ? sheet with ⌥ and ⇧ hints', () => {
    render(<ShortcutsHelp open onOpenChange={() => {}} />);
    for (const k of ['copyPassword', 'copyUsername', 'copyTotp', 'edit', 'openWebsite'] as const) expect(screen.getByText(translate('en', `itemKeys.${k}`))).toBeInTheDocument();
    expect(formatCombo('mod+alt+c')).toContain(navigator.platform.includes('Mac') ? '⌥' : 'Alt');
  });

  it('palette item commands exist only while one item is selected, with shortcut hints', () => {
    const ids = () => builtinCommands().filter((c) => c.id.startsWith('item-')).map((c) => c.id);
    expect(ids()).toEqual(['item-copy-password', 'item-copy-username', 'item-copy-totp', 'item-open-website', 'item-edit']);
    expect(builtinCommands().find((c) => c.id === 'item-copy-totp')?.shortcut).toBe('mod+alt+c');
    useItemsUI.setState({ items: [{ ...item, deletedAt: '2026-10-20T00:00:00Z' }] });
    expect(ids()).not.toContain('item-edit');
    useApp.setState({ multiIds: ['i1', 'i2'] });
    expect(ids()).toEqual([]);
    useApp.setState({ multiIds: [], selectedItemId: null });
    expect(ids()).toEqual([]);
  });

  it('⌥ shortcuts match the physical key when macOS reports a different character', () => {
    expect(matches('mod+alt+c', { key: 'ç', code: 'KeyC', metaKey: true, ctrlKey: false, altKey: true, shiftKey: false }, true)).toBe(true);
    expect(matches('mod+c', { key: 'ç', code: 'KeyC', metaKey: true, ctrlKey: false, altKey: true, shiftKey: false }, true)).toBe(false);
  });
});

describe('item shortcut strings', () => {
  it('exist in en, my and th', async () => {
    const { dictionaries } = await import('../../i18n');
    const keys = Object.keys(dictionaries.en).filter((k) => k.startsWith('itemKeys.'));
    expect(keys.length).toBeGreaterThanOrEqual(13);
    for (const d of [dictionaries.my, dictionaries.th]) for (const k of keys) expect(d[k as keyof typeof d], k).toBeTruthy();
  });
});
