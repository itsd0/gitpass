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
import type { ItemSummary } from '../../api/types';
import { translate, useI18n, type Key } from '../../i18n';
import { useApp } from '../../lib/store';
import { Toaster, useToasts } from '../../ui';
import { ItemList } from './ItemList';
import { useItemsUI } from './state';
import { daysUntilPurge, trashHint } from './trash';

const DAY_MS = 86_400_000;
const NOW = Date.UTC(2026, 9, 28, 12);
const t = (k: Key, vars?: Record<string, string | number>) => translate('en', k, vars);

function summary(id: string, title: string, deletedAt: string | null): ItemSummary {
  return { id, vaultId: 'v_test', category: 'login', title, subtitle: 'fake-user', favorite: false, archived: false,
    deletedAt, tags: [], updatedAt: deletedAt ?? new Date(NOW).toISOString(), createdAt: new Date(NOW).toISOString(), url: null };
}

let trash: ItemSummary[] = [];
let emptyTrash: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  useApp.setState({ route: { name: 'items', filter: { kind: 'deleted' } }, selectedItemId: null, multiIds: [], bulkConfirm: null, itemsVersion: 0 });
  useItemsUI.setState({ query: '', category: null, sort: 'updated', items: [], loading: true });
  useI18n.setState({ lang: 'en' });
  useToasts.setState({ list: [] });
  // Relative to the real clock: the rows compute days against Date.now().
  const now = Date.now();
  trash = [summary('i1', 'One day left', new Date(now - 29 * DAY_MS).toISOString()), summary('i2', 'Plenty of time', new Date(now - 26 * DAY_MS).toISOString())];
  vi.spyOn(api, 'listItems').mockImplementation(async ({ filter }) => (filter.kind === 'deleted' ? [...trash] : []));
  emptyTrash = vi.spyOn(api, 'emptyTrash').mockImplementation(async () => { const n = trash.length; trash = []; return n; });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('purge countdown', () => {
  it('rounds up remaining days and clamps past the deadline to today', () => {
    expect(daysUntilPurge(new Date(NOW).toISOString(), NOW)).toBe(30);
    expect(daysUntilPurge(new Date(NOW - 27 * DAY_MS).toISOString(), NOW)).toBe(3);
    expect(daysUntilPurge(new Date(NOW - 29.5 * DAY_MS).toISOString(), NOW)).toBe(1);
    expect(daysUntilPurge(new Date(NOW - 30 * DAY_MS).toISOString(), NOW)).toBe(0);
    expect(daysUntilPurge(new Date(NOW - 45 * DAY_MS).toISOString(), NOW)).toBe(0);
  });
  it('ignores missing or invalid dates', () => {
    expect(daysUntilPurge(null, NOW)).toBeNull();
    expect(daysUntilPurge(undefined, NOW)).toBeNull();
    expect(daysUntilPurge('not a date', NOW)).toBeNull();
    expect(trashHint(null, NOW, t)).toBeNull();
  });
  it('formats the row line and flags the last three days', () => {
    expect(trashHint(new Date(NOW - 1 * DAY_MS).toISOString(), NOW, t)).toEqual({ text: 'Deletes in 29 days', danger: false, days: 29 });
    expect(trashHint(new Date(NOW - 26 * DAY_MS).toISOString(), NOW, t)).toEqual({ text: 'Deletes in 4 days', danger: false, days: 4 });
    expect(trashHint(new Date(NOW - 27 * DAY_MS).toISOString(), NOW, t)).toEqual({ text: 'Deletes in 3 days', danger: true, days: 3 });
    expect(trashHint(new Date(NOW - 29 * DAY_MS).toISOString(), NOW, t)).toEqual({ text: 'Deletes in 1 day', danger: true, days: 1 });
    expect(trashHint(new Date(NOW - 30 * DAY_MS).toISOString(), NOW, t)).toEqual({ text: 'Deletes today', danger: true, days: 0 });
  });
});

describe('recently deleted view', () => {
  it('shows days-left per row and uses the danger colour within three days', async () => {
    render(<ItemList />);
    expect(await screen.findByText('One day left')).toBeTruthy();
    expect(screen.getByText('Deletes in 1 day')).toHaveClass('text-danger');
    expect(screen.getByText('Deletes in 4 days')).toHaveClass('text-muted');
    expect(screen.getAllByTestId('trash-days')).toHaveLength(2);
  });

  it('shows the count, asks for confirmation with Cancel focused, and cancel does nothing', async () => {
    render(<><ItemList /><Toaster /></>);
    expect(await screen.findByText('2 items')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /Empty Recently Deleted/ }));
    expect(await screen.findByText('Permanently delete 2 items?')).toBeTruthy();
    expect(screen.getByText(/This can’t be undone/)).toBeTruthy();

    const cancel = screen.getByRole('button', { name: 'Cancel' });
    await waitFor(() => expect(cancel).toHaveFocus());
    fireEvent.click(cancel);
    await waitFor(() => expect(screen.queryByText('Permanently delete 2 items?')).toBeNull());
    expect(emptyTrash).not.toHaveBeenCalled();
    expect(trash).toHaveLength(2);
    expect(screen.getByText('One day left')).toBeTruthy();
  });

  it('Escape closes the dialog without purging', async () => {
    render(<ItemList />);
    fireEvent.click(await screen.findByRole('button', { name: /Empty Recently Deleted/ }));
    expect(await screen.findByText('Permanently delete 2 items?')).toBeTruthy();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByText('Permanently delete 2 items?')).toBeNull());
    expect(emptyTrash).not.toHaveBeenCalled();
  });

  it('confirm purges via the API, toasts, and shows the empty state', async () => {
    render(<><ItemList /><Toaster /></>);
    fireEvent.click(await screen.findByRole('button', { name: /Empty Recently Deleted/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete permanently' }));
    await waitFor(() => expect(emptyTrash).toHaveBeenCalledTimes(1));
    expect(await screen.findByTestId('trash-empty')).toBeTruthy();
    expect(screen.getByText('Recently Deleted is empty')).toBeTruthy();
    expect(screen.getByText('Items you delete stay here for 30 days')).toBeTruthy();
    expect(await screen.findByText('Permanently deleted 2 items')).toBeTruthy();
    expect(screen.queryByTestId('trash-bar')).toBeNull();
  });

  it('uses singular strings for a single trashed item', async () => {
    trash = [summary('i1', 'Only one', new Date(Date.now() - 29 * DAY_MS).toISOString())];
    render(<ItemList />);
    expect(await screen.findByText('1 item')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /Empty Recently Deleted/ }));
    expect(await screen.findByText('Permanently delete 1 item?')).toBeTruthy();
  });

  it('renders my and th strings after a locale switch', async () => {
    useI18n.setState({ lang: 'my' });
    render(<ItemList />);
    expect(await screen.findByText('မကြာသေးမီက ဖျက်ထားသည်များကို ရှင်းလင်းရန်')).toBeTruthy();
    expect(screen.getAllByTestId('trash-days').some((el) => el.textContent?.startsWith('နောက်'))).toBe(true);

    useI18n.setState({ lang: 'th' });
    expect(await screen.findByText('ล้างรายการที่เพิ่งลบ')).toBeTruthy();
    expect(screen.getAllByTestId('trash-days').some((el) => el.textContent?.startsWith('จะลบใน'))).toBe(true);
    expect(await screen.findByText('2 รายการ')).toBeTruthy(); // lets both list fetches settle
  });
});
