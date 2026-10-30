import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
// jsdom has no layout, so render every virtual row.
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0, scrollToIndex: () => {},
    getVirtualItems: () => Array.from({ length: o.count }, (_, i) => ({ index: i, start: 0, size: o.estimateSize(i), key: i })),
  }),
}));
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../../api/client';
import type { Filter, ItemDetail, ItemSummary } from '../../api/types';
import { useI18n } from '../../i18n';
import { useApp } from '../../lib/store';
import { Toaster, TooltipProvider, useToasts } from '../../ui';
import { BulkConfirm } from './BulkPane';
import { ItemContextMenu, menuFields } from './ItemContextMenu';
import { ItemList } from './ItemList';
import { useItemsUI } from './state';

afterEach(() => { cleanup(); useToasts.setState({ list: [] }); useI18n.setState({ lang: 'en' }); });

async function load(filter: Filter = { kind: 'all' }) {
  const items = await api.listItems({ filter });
  useItemsUI.setState({ items, loading: false });
  return items;
}
const field = (id: string, label: string, kind: string, value = 'x') => ({ id, label, kind, value }) as ItemDetail['sections'][number]['fields'][number];
const detailOf = (fields: ReturnType<typeof field>[], url: string | null = null) =>
  ({ url, sections: [{ id: 's', title: '', fields }] }) as unknown as ItemDetail;

describe('menuFields', () => {
  it('offers only the field actions an item has', () => {
    const login = menuFields(detailOf([field('username', 'username', 'text'), field('password', 'password', 'concealed'), field('otp', 'one-time password', 'totp')], 'https://example.test'));
    expect(login).toEqual({ username: true, password: true, totp: true, website: true });
    const note = menuFields(detailOf([]));
    expect(note).toEqual({ username: false, password: false, totp: false, website: false });
    expect(menuFields(detailOf([field('u', 'link', 'url', 'javascript:alert(1)')])).website).toBe(false);
  });
});

describe('item list context menu', () => {
  beforeEach(() => useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, multiIds: [], selectedItemId: null, bulkConfirm: null, editing: false }));
  const renderList = () => render(<TooltipProvider><ItemList /><BulkConfirm /><Toaster /></TooltipProvider>);

  it('opens on right-click with the login actions, selecting the clicked row', async () => {
    const items = await load();
    const login = items.find((i) => i.category === 'login' && !i.archived)!;
    renderList();
    const row = document.querySelector(`[data-item-id="${login.id}"]`)!;
    fireEvent.contextMenu(row, { clientX: 40, clientY: 60 });
    const menu = await screen.findByTestId('item-context-menu');
    expect(menu.getAttribute('role')).toBe('menu');
    expect(menu.getAttribute('aria-label')).toBe('Item actions');
    expect(useApp.getState().selectedItemId).toBe(login.id);
    expect(await screen.findByTestId('ctx-copy-password')).toBeTruthy();
    for (const id of ['ctx-favorite', 'ctx-edit', 'ctx-move', 'ctx-archive', 'ctx-trash']) expect(screen.getByTestId(id)).toBeTruthy();
    expect(screen.getByTestId('ctx-edit').textContent).toMatch(/Edit/);
    expect(screen.getByTestId('ctx-trash').textContent).toBe('Move to Recently Deleted');
  });

  it('opens from the keyboard (Shift+F10 and the context-menu key) on the selected row', async () => {
    const items = await load();
    useApp.setState({ selectedItemId: items[0].id });
    renderList();
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'F10', shiftKey: true });
    expect(await screen.findByTestId('item-context-menu')).toBeTruthy();
    fireEvent.keyDown(screen.getByTestId('item-context-menu'), { key: 'Escape' });
    await waitFor(() => expect(screen.queryByTestId('item-context-menu')).toBeNull());
    expect(document.activeElement).toBe(list);
    fireEvent.keyDown(list, { key: 'ContextMenu' });
    expect(await screen.findByTestId('item-context-menu')).toBeTruthy();
  });

  it('favorites the item from the menu', async () => {
    const items = await load();
    const target = items.find((i) => !i.favorite && !i.archived)!;
    renderList();
    fireEvent.contextMenu(document.querySelector(`[data-item-id="${target.id}"]`)!);
    fireEvent.click(await screen.findByTestId('ctx-favorite'));
    await waitFor(async () => expect((await api.getItem({ vaultId: target.vaultId, itemId: target.id })).favorite).toBe(true));
    expect(await screen.findByText('Added to Favorites')).toBeTruthy();
  });

  it('Edit switches the selected item to edit mode', async () => {
    const items = await load();
    renderList();
    fireEvent.contextMenu(document.querySelector(`[data-item-id="${items[1].id}"]`)!);
    fireEvent.click(await screen.findByTestId('ctx-edit'));
    await waitFor(() => expect(useApp.getState().editing).toBe(true));
    expect(useApp.getState().selectedItemId).toBe(items[1].id);
  });

  it('acts on the whole multi-selection with a count and only bulk-safe actions', async () => {
    const items = await load();
    const ids = items.filter((i) => !i.archived && !i.favorite).slice(0, 3).map((i) => i.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    renderList();
    fireEvent.contextMenu(document.querySelector(`[data-item-id="${ids[1]}"]`)!);
    expect(await screen.findByTestId('ctx-archive')).toBeTruthy();
    expect(screen.getByTestId('ctx-archive').textContent).toBe('Archive 3 items');
    expect(screen.getByTestId('ctx-favorite').textContent).toBe('Add 3 items to Favorites');
    expect(screen.getByTestId('ctx-trash').textContent).toBe('Move 3 items to Recently Deleted');
    for (const id of ['ctx-edit', 'ctx-copy-password', 'ctx-copy-username', 'ctx-open-website']) expect(screen.queryByTestId(id)).toBeNull();
    expect(useApp.getState().multiIds).toEqual(ids);
    fireEvent.click(screen.getByTestId('ctx-archive'));
    await waitFor(async () => {
      const archived = await api.listItems({ filter: { kind: 'archive' } });
      expect(ids.every((id) => archived.some((a) => a.id === id))).toBe(true);
    });
  });

  it('right-clicking an unselected row replaces the multi-selection', async () => {
    const items = await load();
    useApp.setState({ selectedItemId: items[0].id, multiIds: [items[0].id, items[1].id] });
    renderList();
    fireEvent.contextMenu(document.querySelector(`[data-item-id="${items[3].id}"]`)!);
    await screen.findByTestId('item-context-menu');
    expect(useApp.getState().multiIds).toEqual([]);
    expect(useApp.getState().selectedItemId).toBe(items[3].id);
  });

  it('offers Restore and Delete permanently (with the confirmation) in Recently Deleted', async () => {
    const live = await api.listItems({ filter: { kind: 'all' } });
    await api.deleteItem({ vaultId: live[0].vaultId, itemId: live[0].id });
    useApp.setState({ route: { name: 'items', filter: { kind: 'deleted' } } });
    const items = await load({ kind: 'deleted' });
    renderList();
    fireEvent.contextMenu(document.querySelector(`[data-item-id="${items[0].id}"]`)!);
    expect((await screen.findByTestId('ctx-restore')).textContent).toBe('Restore');
    expect(screen.queryByTestId('ctx-edit')).toBeNull();
    expect(screen.queryByTestId('ctx-trash')).toBeNull();
    fireEvent.click(screen.getByTestId('ctx-purge'));
    expect(await screen.findByText('Delete permanently?')).toBeTruthy();
  });

  it('localizes the menu in Thai and Burmese', async () => {
    const items = await load();
    const some = items.filter((i) => !i.archived).slice(0, 2);
    for (const [lang, label] of [['th', 'การดำเนินการกับรายการ'], ['my', 'အရာ လုပ်ဆောင်ချက်များ']] as const) {
      useI18n.setState({ lang });
      const { unmount } = render(<ItemContextMenu target={{ x: 1, y: 1, items: some as ItemSummary[] }} onClose={() => {}} />);
      expect((await screen.findByTestId('item-context-menu')).getAttribute('aria-label')).toBe(label);
      expect(screen.getByTestId('ctx-archive').textContent).toMatch(/2/);
      unmount();
    }
  });
});
