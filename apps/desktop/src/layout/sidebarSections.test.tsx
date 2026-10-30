import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { Sidebar } from './Sidebar';
import { Toaster, TooltipProvider, useToasts } from '../ui';
import type { Filter, ItemSummary, VaultInfo } from '../api/types';

const KEY = 'gitpass.sidebar.collapsedSections';

const V: VaultInfo[] = [
  { id: 'v1', name: 'Personal', description: null, itemCount: 3, shared: false, members: [], icon: 'vault', color: '#64748b' },
];

const item = (id: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id, vaultId: 'v1', category: 'login', title: `Fake ${id}`, subtitle: '', favorite: false, archived: false, deletedAt: null,
  tags: [], updatedAt: '2026-01-01T00:00:00Z', createdAt: '2026-01-01T00:00:00Z', ...over,
});

let items: ItemSummary[] = [];
const list = ({ filter }: { filter: Filter }) => {
  const live = items.filter((x) => !x.deletedAt && !x.archived);
  if (filter.kind === 'deleted') return Promise.resolve(items.filter((x) => x.deletedAt));
  if (filter.kind === 'archive') return Promise.resolve(items.filter((x) => x.archived && !x.deletedAt));
  return Promise.resolve(live);
};

const renderSidebar = () => render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
const countOf = (row: HTMLElement) => within(row).getByTestId('sidebar-count').textContent;

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  useToasts.setState({ list: [] });
});

beforeEach(() => {
  localStorage.clear();
  items = [
    item('a', { favorite: true, tags: ['work'] }),
    item('b', { tags: ['work', 'home'] }),
    item('c', { archived: true }),
    item('d', { deletedAt: '2026-01-02T00:00:00Z' }),
  ];
  vi.spyOn(api, 'listVaults').mockResolvedValue(V);
  vi.spyOn(api, 'listTags').mockResolvedValue(['home', 'work']);
  vi.spyOn(api, 'listItems').mockImplementation(list as typeof api.listItems);
  useApp.setState({ route: { name: 'items', filter: { kind: 'all', sort: 'updated' } }, itemsVersion: 0, locked: false });
});

describe('sidebar collapsible sections', () => {
  it('toggles Tags with the mouse, with aria-expanded and a localized label', async () => {
    renderSidebar();
    const header = await screen.findByRole('button', { name: 'Collapse Tags' });
    expect(header.getAttribute('aria-expanded')).toBe('true');
    expect(await screen.findByRole('button', { name: /^work$/ })).toBeTruthy();

    await userEvent.click(header);
    const expand = screen.getByRole('button', { name: 'Expand Tags' });
    expect(expand.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('button', { name: /^work$/ })).toBeNull();
    // Vaults section is independent
    expect(screen.getByRole('button', { name: /^Personal/ })).toBeTruthy();

    await userEvent.click(expand);
    expect(await screen.findByRole('button', { name: /^work$/ })).toBeTruthy();
  });

  it('collapses with Left, expands with Right and Enter/Space, keeping focus on the header', async () => {
    renderSidebar();
    const header = await screen.findByRole('button', { name: 'Collapse Vaults' });
    header.focus();
    fireEvent.keyDown(header, { key: 'ArrowLeft' });
    const collapsed = screen.getByRole('button', { name: 'Expand Vaults' });
    expect(collapsed).toBe(header);
    expect(document.activeElement).toBe(collapsed);
    expect(screen.queryByTestId('sidebar-vault')).toBeNull();

    fireEvent.keyDown(collapsed, { key: 'ArrowLeft' }); // already collapsed: no-op
    expect(screen.getByRole('button', { name: 'Expand Vaults' }).getAttribute('aria-expanded')).toBe('false');

    fireEvent.keyDown(collapsed, { key: 'ArrowRight' });
    expect(screen.getByRole('button', { name: 'Collapse Vaults' }).getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByTestId('sidebar-vault')).toBeTruthy();

    await userEvent.keyboard('{Enter}');
    expect(screen.getByRole('button', { name: 'Expand Vaults' })).toBeTruthy();
    await userEvent.keyboard(' ');
    expect(screen.getByRole('button', { name: 'Collapse Vaults' })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Collapse Vaults' }));
  });

  it('remembers the collapsed state across remounts in local storage only', async () => {
    const first = renderSidebar();
    await userEvent.click(await screen.findByRole('button', { name: 'Collapse Tags' }));
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ vaults: false, tags: true, searches: false });
    first.unmount();

    renderSidebar();
    expect((await screen.findByRole('button', { name: 'Expand Tags' })).getAttribute('aria-expanded')).toBe('false');
    expect(screen.getByRole('button', { name: 'Collapse Vaults' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /^work$/ })).toBeNull();
  });

  it('ignores a corrupt stored value', async () => {
    localStorage.setItem(KEY, '{not json');
    renderSidebar();
    expect(await screen.findByRole('button', { name: 'Collapse Tags' })).toBeTruthy();
  });
});

describe('sidebar counts', () => {
  it('shows counts on All items, Favorites, Archive, Recently deleted and tags', async () => {
    renderSidebar();
    const all = await screen.findByRole('button', { name: 'All Items' });
    await waitFor(() => expect(countOf(all)).toBe('2'));
    expect(countOf(screen.getByRole('button', { name: 'Favorites' }))).toBe('1');
    expect(countOf(screen.getByRole('button', { name: 'Archive' }))).toBe('1');
    expect(countOf(screen.getByRole('button', { name: 'Recently Deleted' }))).toBe('1');
    expect(countOf(screen.getByRole('button', { name: /^work$/ }))).toBe('2');
    expect(countOf(screen.getByRole('button', { name: /^home$/ }))).toBe('1');
  });

  it('updates after a delete and a restore', async () => {
    renderSidebar();
    const all = await screen.findByRole('button', { name: 'All Items' });
    await waitFor(() => expect(countOf(all)).toBe('2'));

    items = items.map((x) => (x.id === 'a' ? { ...x, deletedAt: '2026-01-03T00:00:00Z' } : x));
    act(() => useApp.getState().refreshItems());
    await waitFor(() => expect(countOf(screen.getByRole('button', { name: 'All Items' }))).toBe('1'));
    expect(countOf(screen.getByRole('button', { name: 'Recently Deleted' }))).toBe('2');
    expect(countOf(screen.getByRole('button', { name: 'Favorites' }))).toBe('0');
    expect(countOf(screen.getByRole('button', { name: /^work$/ }))).toBe('1');

    items = items.map((x) => (x.id === 'a' ? { ...x, deletedAt: null } : x));
    act(() => useApp.getState().refreshItems());
    await waitFor(() => expect(countOf(screen.getByRole('button', { name: 'All Items' }))).toBe('2'));
    expect(countOf(screen.getByRole('button', { name: 'Recently Deleted' }))).toBe('1');
    expect(countOf(screen.getByRole('button', { name: 'Favorites' }))).toBe('1');
    expect(countOf(screen.getByRole('button', { name: /^work$/ }))).toBe('2');
  });
});
