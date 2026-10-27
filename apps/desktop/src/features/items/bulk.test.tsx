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
import { BulkConfirm, BulkPane } from './BulkPane';
import { ItemList } from './ItemList';
import type { Filter } from '../../api/types';
afterEach(cleanup);

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
});
