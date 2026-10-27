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
import { useApp } from '../../lib/store';
import { useItemsUI } from './state';
import { BulkConfirm, BulkPane, runBulk } from './BulkPane';
import { ItemList } from './ItemList';
import type { Filter } from '../../api/types';
import { Toaster, useToasts } from '../../ui';
import { translate } from '../../i18n';
afterEach(() => { cleanup(); useToasts.setState({ list: [] }); });

async function load(filter: Filter = { kind: 'all' }) {
  const items = await api.listItems({ filter });
  useItemsUI.setState({ items, loading: false });
  return items;
}
describe('bulk pane', () => {
  beforeEach(() => useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, multiIds: [], selectedItemId: null, bulkConfirm: null }));

  it('shows N items selected and trashes after confirm', async () => {
    const items = await load();
    const ids = items.slice(0, 3).map((x) => x.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<><BulkPane /><BulkConfirm /></>);
    expect(screen.getByTestId('bulk-count').textContent).toBe('3 items selected');
    fireEvent.click(screen.getByRole('button', { name: /Move to Trash/ }));
    expect(await screen.findByText('Move to Trash?')).toBeTruthy();
    expect(screen.getByText(/3 items will move to Recently Deleted/)).toBeTruthy();
    fireEvent.click(screen.getAllByRole('button', { name: 'Move to Trash' }).at(-1)!);
    await waitFor(() => expect(useApp.getState().multiIds).toEqual([]));
    const deleted = await api.listItems({ filter: { kind: 'deleted' } });
    expect(ids.every((id) => deleted.some((d) => d.id === id))).toBe(true);
    // restore them from the Recently Deleted view
    useApp.setState({ route: { name: 'items', filter: { kind: 'deleted' } } });
    await load({ kind: 'deleted' });
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    cleanup(); render(<BulkPane />);
    expect(screen.getByRole('button', { name: /Delete permanently/ })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /Restore 3 items/ }));
    await waitFor(async () => expect((await api.listItems({ filter: { kind: 'deleted' } })).length).toBe(deleted.length - 3));
  });

  it('opens the export dialog for the selection', async () => {
    const items = await load();
    const ids = items.slice(0, 2).map((x) => x.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<BulkPane />);
    fireEvent.click(screen.getByRole('button', { name: /Export selected/ }));
    expect(await screen.findByText('Export selected items')).toBeTruthy();
    expect(screen.getByText('2 selected item(s)')).toBeTruthy();
  });

  it('adds a tag and favorites in one action', async () => {
    const items = await load();
    const ids = items.slice(0, 2).map((x) => x.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<BulkPane />);
    fireEvent.change(screen.getByLabelText('Tag to add'), { target: { value: 'bulk-test' } });
    fireEvent.click(screen.getByRole('button', { name: /^Add$/ }));
    await waitFor(async () => {
      for (const id of ids) expect((await api.getItem({ vaultId: '', itemId: id })).tags).toContain('bulk-test');
    });
    expect(useApp.getState().multiIds).toEqual(ids); // non-destructive actions keep the selection
  });

  it('bulk delete shows undo toast (~5s) that restores items on click', async () => {
    const items = await load();
    const ids = items.slice(0, 3).map((x) => x.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<><Toaster /><BulkPane /><BulkConfirm /></>);
    fireEvent.click(screen.getByRole('button', { name: /Move to Trash/ }));
    fireEvent.click(screen.getAllByRole('button', { name: 'Move to Trash' }).at(-1)!);

    await waitFor(async () => {
      const deleted = await api.listItems({ filter: { kind: 'deleted' } });
      expect(ids.every((id) => deleted.some((d) => d.id === id))).toBe(true);
    });

    const undoBtn = await screen.findByRole('button', { name: /Undo/i });
    expect(undoBtn).toBeTruthy();
    fireEvent.click(undoBtn);

    await waitFor(async () => {
      const deleted = await api.listItems({ filter: { kind: 'deleted' } });
      expect(ids.some((id) => deleted.some((d) => d.id === id))).toBe(false);
    });
  });

  it('bulk archive shows undo toast that unarchives items on click', async () => {
    const items = await load();
    const ids = items.slice(0, 2).map((x) => x.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<><Toaster /><BulkPane /></>);
    fireEvent.click(screen.getByRole('button', { name: /^Archive$/ }));

    await waitFor(async () => {
      const archived = await api.listItems({ filter: { kind: 'archive' } });
      expect(ids.every((id) => archived.some((d) => d.id === id))).toBe(true);
    });

    const undoBtn = await screen.findByRole('button', { name: /Undo/i });
    fireEvent.click(undoBtn);

    await waitFor(async () => {
      const archived = await api.listItems({ filter: { kind: 'archive' } });
      expect(ids.some((id) => archived.some((d) => d.id === id))).toBe(false);
    });
  });

  it('bulk favorite shows undo toast that unfavorites items on click', async () => {
    const items = await load();
    const ids = items.slice(0, 2).map((x) => x.id);
    await api.bulkItems({ req: { action: 'unfavorite', items: ids.map((id) => ({ vaultId: '', itemId: id })) } });
    await load();
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<><Toaster /><BulkPane /></>);
    fireEvent.click(screen.getByRole('button', { name: /Add to Favorites/ }));

    await waitFor(async () => {
      for (const id of ids) expect((await api.getItem({ vaultId: '', itemId: id })).favorite).toBe(true);
    });

    const undoBtn = await screen.findByRole('button', { name: /Undo/i });
    fireEvent.click(undoBtn);

    await waitFor(async () => {
      for (const id of ids) expect((await api.getItem({ vaultId: '', itemId: id })).favorite).toBe(false);
    });
  });

  it('bulk tag shows undo toast that removes tag on click', async () => {
    const items = await load();
    const ids = items.slice(0, 2).map((x) => x.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<><Toaster /><BulkPane /></>);
    fireEvent.change(screen.getByLabelText('Tag to add'), { target: { value: 'undo-tag-test' } });
    fireEvent.click(screen.getByRole('button', { name: /^Add$/ }));

    await waitFor(async () => {
      for (const id of ids) expect((await api.getItem({ vaultId: '', itemId: id })).tags).toContain('undo-tag-test');
    });

    const undoBtn = await screen.findByRole('button', { name: /Undo/i });
    fireEvent.click(undoBtn);

    await waitFor(async () => {
      for (const id of ids) expect((await api.getItem({ vaultId: '', itemId: id })).tags).not.toContain('undo-tag-test');
    });
  });

  it('bulk move shows undo toast that moves items back to original vault', async () => {
    const items = await load();
    const vaults = await api.listVaults();
    const origVault = items[0].vaultId;
    const destVault = vaults.find((v) => v.id !== origVault)!.id;
    const ids = items.filter((x) => x.vaultId === origVault).slice(0, 2).map((x) => x.id);
    useApp.setState({ selectedItemId: ids[0], multiIds: ids });
    render(<Toaster />);
    await runBulk('move', { toVaultId: destVault });

    await waitFor(async () => {
      for (const id of ids) expect((await api.getItem({ vaultId: '', itemId: id })).vaultId).toBe(destVault);
    });

    const undoBtn = await screen.findByRole('button', { name: /Undo/i });
    expect(undoBtn).toBeTruthy();
    fireEvent.click(undoBtn);

    await waitFor(async () => {
      for (const id of ids) expect((await api.getItem({ vaultId: '', itemId: id })).vaultId).toBe(origVault);
    });
  });
});

describe('item list multi-select', () => {
  beforeEach(() => useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, multiIds: [], selectedItemId: null, bulkConfirm: null }));
  it('cmd-click, shift-click, cmd-A, Esc and Delete', async () => {
    const items = await load();
    useApp.setState({ selectedItemId: items[0].id });
    render(<ItemList />);
    const rows = await screen.findAllByRole('option');
    fireEvent.click(rows[2], { metaKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[2].id]);
    fireEvent.click(rows[1]);
    fireEvent.click(rows[3], { shiftKey: true });
    expect(useApp.getState().multiIds).toEqual([items[1].id, items[2].id, items[3].id]);
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'Escape' });
    expect(useApp.getState().multiIds).toEqual([]);
    fireEvent.keyDown(list, { key: 'a', metaKey: true });
    expect(useApp.getState().multiIds.length).toBe(useItemsUI.getState().items.length);
    fireEvent.keyDown(list, { key: 'Delete' });
    expect(useApp.getState().bulkConfirm).toBe('trash');
  });

  it('Cmd-click toggles and deselects down to single', async () => {
    const items = await load();
    useApp.setState({ selectedItemId: items[0].id });
    render(<ItemList />);
    const rows = await screen.findAllByRole('option');
    // Cmd-click adds row 2
    fireEvent.click(rows[2], { metaKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[2].id]);
    // Cmd-click adds row 1
    fireEvent.click(rows[1], { metaKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[1].id, items[2].id]);
    // Cmd-click row 2 deselects it
    fireEvent.click(rows[2], { metaKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[1].id]);
    // Cmd-click row 1 deselects it, collapsing to single selection on row 0
    fireEvent.click(rows[1], { metaKey: true });
    expect(useApp.getState().multiIds).toEqual([]);
    expect(useApp.getState().selectedItemId).toBe(items[0].id);
    // Ctrl-click acts as toggle as well
    fireEvent.click(rows[3], { ctrlKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[3].id]);
  });

  it('Shift-click selects range in both directions from anchor', async () => {
    const items = await load();
    useApp.setState({ selectedItemId: items[2].id });
    render(<ItemList />);
    const rows = await screen.findAllByRole('option');
    // Shift-click down to row 4 selects 2..4
    fireEvent.click(rows[4], { shiftKey: true });
    expect(useApp.getState().multiIds).toEqual([items[2].id, items[3].id, items[4].id]);
    expect(useApp.getState().selectedItemId).toBe(items[2].id);
    // Shift-click up to row 0 selects 0..2
    fireEvent.click(rows[0], { shiftKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[1].id, items[2].id]);
    expect(useApp.getState().selectedItemId).toBe(items[2].id);
    // Plain click collapses to single selection
    fireEvent.click(rows[1]);
    expect(useApp.getState().multiIds).toEqual([]);
    expect(useApp.getState().selectedItemId).toBe(items[1].id);
  });

  it('shift+arrow extends and shrinks the selection; plain arrow returns to single', async () => {
    const items = await load();
    useApp.setState({ selectedItemId: items[0].id });
    render(<ItemList />);
    await screen.findAllByRole('option');
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'ArrowDown', shiftKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[1].id]);
    fireEvent.keyDown(list, { key: 'ArrowDown', shiftKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[1].id, items[2].id]);
    fireEvent.keyDown(list, { key: 'ArrowUp', shiftKey: true });
    expect(useApp.getState().multiIds).toEqual([items[0].id, items[1].id]);
    // Plain arrow moves one step from the focus row and collapses to single.
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    expect(useApp.getState().multiIds).toEqual([]);
    expect(useApp.getState().selectedItemId).toBe(items[2].id);
  });

  it('cmd-A then plain arrow moves from the anchor, not the opposite end', async () => {
    const items = await load();
    useApp.setState({ selectedItemId: items[2].id });
    render(<ItemList />);
    await screen.findAllByRole('option');
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'a', metaKey: true });
    expect(useApp.getState().multiIds.length).toBe(items.length);
    expect(useApp.getState().selectedItemId).toBe(items[2].id);
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    expect(useApp.getState().multiIds).toEqual([]);
    expect(useApp.getState().selectedItemId).toBe(items[3].id);
  });

  it('has Burmese and Thai strings for all bulk keys', () => {
    const keys = [
      'bulk.undo', 'bulk.undone', 'bulk.nSelected', 'bulk.oneSelected', 'bulk.clear',
      'bulk.restore', 'bulk.deletePermanently', 'bulk.unarchive', 'bulk.moveToTrash',
      'bulk.favorite', 'bulk.unfavorite', 'bulk.addTag', 'bulk.add', 'bulk.moveToVault',
      'bulk.copyToVault', 'bulk.archive', 'bulk.hint', 'bulk.toast.trash', 'bulk.toast.restore',
      'bulk.toast.archive', 'bulk.toast.unarchive', 'bulk.toast.favorite',
      'bulk.toast.unfavorite', 'bulk.toast.add_tag', 'bulk.toast.remove_tag',
      'bulk.toast.move', 'bulk.toast.copy',
    ] as const;
    for (const k of keys) {
      expect(translate('my', k)).not.toBe(k);
      expect(translate('my', k)).not.toBe(translate('en', k));
      expect(translate('th', k)).not.toBe(k);
      expect(translate('th', k)).not.toBe(translate('en', k));
    }
  });
});
