import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
// jsdom has no layout, so render every virtual row.
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0, scrollToIndex: () => {},
    getVirtualItems: () => Array.from({ length: o.count }, (_, i) => ({ index: i, start: 0, size: o.estimateSize(i), key: i })),
  }),
}));
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from '../../api/client';
import type { Filter } from '../../api/types';
import { useI18n } from '../../i18n';
import { useApp } from '../../lib/store';
import { Toaster, TooltipProvider, useToasts } from '../../ui';
import { BulkConfirm } from './BulkPane';
import { ItemList } from './ItemList';
import { clearRowFields } from './rowFields';
import { useItemsUI } from './state';

afterEach(() => { cleanup(); useToasts.setState({ list: [] }); useI18n.setState({ lang: 'en' }); });

async function load(filter: Filter = { kind: 'all' }) {
  const items = await api.listItems({ filter });
  useItemsUI.setState({ items, loading: false });
  return items;
}
const rowOf = (id: string) => document.querySelector(`[data-item-id="${id}"]`) as HTMLElement;

describe('row quick actions', () => {
  beforeEach(() => {
    useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, multiIds: [], selectedItemId: null, bulkConfirm: null, editing: false });
    clearRowFields();
  });
  const renderList = () => render(<TooltipProvider><ItemList /><BulkConfirm /><Toaster /></TooltipProvider>);

  it('shows copy username/password for a login on hover, plus one-time code and website only when the item has them', async () => {
    const items = await load();
    const withTotp = items.find((i) => i.category === 'login' && i.title === 'Example Bank')!;
    renderList();
    fireEvent.mouseEnter(rowOf(withTotp.id));
    const cluster = within(rowOf(withTotp.id)).getByTestId('row-actions');
    // username/password render immediately; totp/website appear once the item's fields have been read
    expect(within(cluster).getByTestId('row-copy-username')).toBeTruthy();
    expect(within(cluster).getByTestId('row-copy-password')).toBeTruthy();
    await waitFor(() => expect(within(cluster).getByTestId('row-copy-totp')).toBeTruthy());
    expect(within(cluster).getByTestId('row-open-website')).toBeTruthy();
    expect(within(cluster).getByTestId('row-copy-username').getAttribute('aria-label')).toBe('Copy username');
    expect(within(cluster).getByTestId('row-copy-totp').getAttribute('aria-label')).toBe('Copy one-time code');
  });

  it('omits the one-time code button for a login without TOTP', async () => {
    const items = await load();
    const noTotp = items.find((i) => i.category === 'login' && i.title === 'GitHub (demo)')!;
    renderList();
    fireEvent.mouseEnter(rowOf(noTotp.id));
    const cluster = within(rowOf(noTotp.id)).getByTestId('row-actions');
    await waitFor(() => expect(within(cluster).getByTestId('row-open-website')).toBeTruthy());
    expect(within(cluster).queryByTestId('row-copy-totp')).toBeNull();
  });

  it('shows only copy number for a card and no cluster for other categories', async () => {
    const items = await load();
    const card = items.find((i) => i.category === 'credit_card')!;
    const note = items.find((i) => i.category === 'secure_note')!;
    renderList();
    const cluster = within(rowOf(card.id)).getByTestId('row-actions');
    expect(within(cluster).getByTestId('row-copy-number')).toBeTruthy();
    expect(within(cluster).getByTestId('row-copy-number').getAttribute('aria-label')).toBe('Copy number');
    expect(within(cluster).queryByTestId('row-copy-username')).toBeNull();
    expect(within(rowOf(note.id)).queryByTestId('row-actions')).toBeNull();
  });

  it('copies through the shared clipboard API without changing the selection', async () => {
    const items = await load();
    const login = items.find((i) => i.category === 'login' && i.title === 'GitHub (demo)')!;
    const other = items.find((i) => i.id !== login.id)!;
    useApp.setState({ selectedItemId: other.id });
    const spy = vi.spyOn(api, 'copyField');
    renderList();
    fireEvent.click(within(rowOf(login.id)).getByTestId('row-copy-password'));
    await waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ vaultId: login.vaultId, itemId: login.id })));
    expect(useApp.getState().selectedItemId).toBe(other.id);
    spy.mockRestore();
  });

  it('Tab-reachable only on the selected row; arrows move between its buttons and Escape returns focus to the row', async () => {
    const items = await load();
    const login = items.find((i) => i.category === 'login' && i.title === 'Example Bank')!;
    renderList();
    fireEvent.mouseEnter(rowOf(login.id));
    const cluster = within(rowOf(login.id)).getByTestId('row-actions');
    await waitFor(() => expect(within(cluster).getByTestId('row-copy-totp')).toBeTruthy());
    const btns = within(cluster).getAllByRole('button');
    // not selected: buttons are out of the tab order
    expect(btns.every((b) => b.tabIndex === -1)).toBe(true);
    useApp.setState({ selectedItemId: login.id });
    await waitFor(() => expect(within(cluster).getAllByRole('button').every((b) => b.tabIndex === 0)).toBe(true));
    btns[0].focus();
    fireEvent.keyDown(cluster, { key: 'ArrowRight' });
    expect(document.activeElement).toBe(within(cluster).getAllByRole('button')[1]);
    fireEvent.keyDown(cluster, { key: 'ArrowLeft' });
    expect(document.activeElement).toBe(within(cluster).getAllByRole('button')[0]);
    fireEvent.keyDown(cluster, { key: 'Escape' });
    expect(document.activeElement).toBe(rowOf(login.id));
    // arrows inside the cluster never move the list selection
    expect(useApp.getState().selectedItemId).toBe(login.id);
  });

  it('is hidden while multi-select is active', async () => {
    const items = await load();
    const login = items.find((i) => i.category === 'login')!;
    renderList();
    expect(screen.queryAllByTestId('row-actions').length).toBeGreaterThan(0);
    useApp.setState({ multiIds: [login.id, items.find((i) => i.id !== login.id)!.id] });
    await waitFor(() => expect(screen.queryAllByTestId('row-actions').length).toBe(0));
  });
});
