import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { useItemsUI } from '../features/items';
import {
  useSavedSearches,
  SAVED_SEARCHES_KEY,
  type SavedSearch,
} from '../features/items/savedSearches';
import { Sidebar } from './Sidebar';
import { Toolbar } from './Toolbar';
import { CommandPalette } from '../features/palette/CommandPalette';
import { Toaster, TooltipProvider, useToasts } from '../ui';
import type { Filter, ItemSummary, VaultInfo } from '../api/types';

const VAULTS: VaultInfo[] = [
  { id: 'v1', name: 'Personal', description: null, itemCount: 2, shared: false, members: [], icon: 'vault', color: '#64748b' },
  { id: 'v2', name: 'Work', description: null, itemCount: 1, shared: false, members: [], icon: null, color: null },
];

const item = (id: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id,
  vaultId: 'v1',
  category: 'login',
  title: `Item ${id}`,
  subtitle: '',
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt: '2026-01-01T00:00:00Z',
  createdAt: '2026-01-01T00:00:00Z',
  ...over,
});

let testItems: ItemSummary[] = [];
const list = ({ filter }: { filter: Filter }) => {
  const live = testItems.filter((x) => !x.deletedAt && !x.archived);
  if (filter.kind === 'deleted') return Promise.resolve(testItems.filter((x) => x.deletedAt));
  if (filter.kind === 'archive') return Promise.resolve(testItems.filter((x) => x.archived && !x.deletedAt));
  if (filter.kind === 'vault' && filter.vaultId) return Promise.resolve(live.filter((x) => x.vaultId === filter.vaultId));
  if (filter.kind === 'tag' && filter.tag) return Promise.resolve(live.filter((x) => x.tags.includes(filter.tag!)));
  return Promise.resolve(live);
};

const renderApp = () =>
  render(
    <TooltipProvider>
      <Toaster />
      <div className="flex h-screen w-screen">
        <Sidebar />
        <div className="flex flex-1 flex-col">
          <Toolbar />
        </div>
      </div>
    </TooltipProvider>
  );

beforeEach(() => {
  localStorage.clear();
  useSavedSearches.getState().reload();
  useToasts.setState({ list: [] });
  testItems = [
    item('1', { title: 'GitHub', tags: ['dev'], vaultId: 'v1' }),
    item('2', { title: 'AWS Console', tags: ['dev', 'cloud'], vaultId: 'v2' }),
    item('3', { title: 'Personal Bank', category: 'credit_card', vaultId: 'v1' }),
  ];
  vi.spyOn(api, 'listVaults').mockResolvedValue(VAULTS);
  vi.spyOn(api, 'listTags').mockResolvedValue(['dev', 'cloud']);
  vi.spyOn(api, 'listItems').mockImplementation(list as typeof api.listItems);
  useApp.setState({
    route: { name: 'items', filter: { kind: 'all', sort: 'updated' } },
    itemsVersion: 0,
    locked: false,
    sidebarCollapsed: false,
  });
  useItemsUI.setState({
    query: '',
    category: null,
    sort: 'updated',
    items: testItems,
    loading: false,
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  useSavedSearches.getState().reload();
});

describe('saved searches', () => {
  it('hides the Saved Searches section when there are no saved searches (empty state)', async () => {
    renderApp();
    expect(screen.queryByText(/SAVED SEARCHES/i)).toBeNull();
    expect(screen.queryByTestId('sidebar-saved-search')).toBeNull();
  });

  it('disables the save search button when query and filters are empty', async () => {
    renderApp();
    const saveBtn = screen.getByTestId('save-search-button');
    expect(saveBtn).toBeDisabled();
  });

  it('saves current search with query and filters, displaying in sidebar and persisting in localStorage', async () => {
    renderApp();
    // Type query
    const input = screen.getByPlaceholderText('Search in gitpass');
    fireEvent.change(input, { target: { value: 'dev items' } });

    const saveBtn = screen.getByTestId('save-search-button');
    expect(saveBtn).not.toBeDisabled();

    // Click Save search button
    fireEvent.click(saveBtn);

    // Popover opens prefilled with query
    const popover = await screen.findByTestId('save-search-popover');
    const nameInput = within(popover).getByLabelText('Name') as HTMLInputElement;
    expect(nameInput.value).toBe('dev items');

    // Change name and submit
    fireEvent.change(nameInput, { target: { value: 'My Dev Work' } });
    fireEvent.click(within(popover).getByRole('button', { name: 'Save search' }));

    // Should appear in the sidebar
    expect(await screen.findByText('SAVED SEARCHES')).toBeTruthy();
    expect(await screen.findByRole('button', { name: 'My Dev Work' })).toBeTruthy();

    // Check localStorage persistence
    const stored = JSON.parse(localStorage.getItem(SAVED_SEARCHES_KEY) ?? '[]');
    expect(stored).toHaveLength(1);
    expect(stored[0].name).toBe('My Dev Work');
    expect(stored[0].query).toBe('dev items');
  });

  it('restores exact query, category, vault and tag filters on click', async () => {
    // Seed a saved search with query, category, vault, and tag
    const search: SavedSearch = {
      id: 's-1',
      name: 'Work Dev Logins',
      query: 'github',
      category: 'login',
      vaultId: 'v2',
      tag: 'dev',
    };
    localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify([search]));
    useSavedSearches.getState().reload();

    renderApp();
    const searchRow = await screen.findByRole('button', { name: 'Work Dev Logins' });
    expect(searchRow).toBeTruthy();

    // Click the saved search
    await userEvent.click(searchRow);

    // Verify useItemsUI query and category
    expect(useItemsUI.getState().query).toBe('github');
    expect(useItemsUI.getState().category).toBe('login');

    // Verify route
    const currentRoute = useApp.getState().route;
    expect(currentRoute.name).toBe('items');
    if (currentRoute.name === 'items') {
      expect(currentRoute.filter.kind).toBe('vault');
      expect(currentRoute.filter.vaultId).toBe('v2');
      expect(currentRoute.filter.tag).toBe('dev');
    }

    // Verify active styling on the row
    expect(searchRow.className).toContain('bg-active');
  });

  it('renames a saved search via F2 keyboard shortcut and updates localStorage', async () => {
    const search: SavedSearch = {
      id: 's-1',
      name: 'Old Search Name',
      query: 'test',
    };
    localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify([search]));
    useSavedSearches.getState().reload();

    renderApp();
    const searchRow = await screen.findByRole('button', { name: 'Old Search Name' });
    searchRow.focus();

    // Press F2 to start rename
    fireEvent.keyDown(searchRow, { key: 'F2' });

    const renameForm = await screen.findByTestId('sidebar-saved-search-rename');
    const input = within(renameForm).getByRole('textbox') as HTMLInputElement;
    expect(input.value).toBe('Old Search Name');

    fireEvent.change(input, { target: { value: 'New Renamed Search' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(await screen.findByRole('button', { name: 'New Renamed Search' })).toBeTruthy();
    const stored = JSON.parse(localStorage.getItem(SAVED_SEARCHES_KEY) ?? '[]');
    expect(stored[0].name).toBe('New Renamed Search');
  });

  it('deletes a saved search via Delete key and allows Undo via toast', async () => {
    const search: SavedSearch = {
      id: 's-1',
      name: 'To Be Deleted',
      query: 'something',
    };
    localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify([search]));
    useSavedSearches.getState().reload();

    renderApp();
    const searchRow = await screen.findByRole('button', { name: 'To Be Deleted' });
    searchRow.focus();

    // Press Delete
    fireEvent.keyDown(searchRow, { key: 'Delete' });

    // Should be removed from DOM
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: 'To Be Deleted' })).toBeNull();
    });

    // Check localStorage
    expect(localStorage.getItem(SAVED_SEARCHES_KEY)).toBeNull();

    // Undo toast should be visible
    const undoBtn = await screen.findByRole('button', { name: 'Undo' });
    expect(undoBtn).toBeTruthy();

    // Click Undo
    fireEvent.click(undoBtn);

    // Row should be restored
    expect(await screen.findByRole('button', { name: 'To Be Deleted' })).toBeTruthy();
    const stored = JSON.parse(localStorage.getItem(SAVED_SEARCHES_KEY) ?? '[]');
    expect(stored[0].name).toBe('To Be Deleted');
  });

  it('supports context menu actions: Rename and Delete', async () => {
    const search: SavedSearch = {
      id: 's-menu',
      name: 'Menu Search',
      query: 'menu',
    };
    localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify([search]));
    useSavedSearches.getState().reload();

    renderApp();
    const searchRow = await screen.findByRole('button', { name: 'Menu Search' });

    // Open context menu via right click
    fireEvent.contextMenu(searchRow);

    const renameItem = await screen.findByText('Rename');
    const deleteItem = await screen.findByText('Delete');
    expect(renameItem).toBeTruthy();
    expect(deleteItem).toBeTruthy();

    // Click Delete in context menu
    fireEvent.click(deleteItem);

    await waitFor(() => {
      expect(screen.queryByRole('button', { name: 'Menu Search' })).toBeNull();
    });
  });

  it('displays matching item count on the saved search row', async () => {
    // 2 items have tag 'dev' in testItems
    const search: SavedSearch = {
      id: 's-count',
      name: 'Dev Tag Search',
      tag: 'dev',
    };
    localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify([search]));
    useSavedSearches.getState().reload();

    renderApp();
    const searchRow = await screen.findByRole('button', { name: 'Dev Tag Search' });
    await waitFor(() => {
      const count = within(searchRow).getByTestId('sidebar-count');
      expect(count.textContent).toBe('2');
    });
  });

  it('shows a muted row and drops missing vault or tag without erroring when vault/tag was deleted', async () => {
    // Saved search references deleted vault 'v-deleted' and deleted tag 'tag-deleted'
    const search: SavedSearch = {
      id: 's-missing',
      name: 'Orphaned Filters Search',
      query: 'findme',
      vaultId: 'v-deleted',
      tag: 'tag-deleted',
    };
    localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify([search]));
    useSavedSearches.getState().reload();

    renderApp();
    const searchRow = await screen.findByRole('button', { name: 'Orphaned Filters Search' });

    // Shows muted row
    expect(searchRow.className).toContain('opacity-60');
    expect(searchRow.className).toContain('text-muted');

    // Clicking it drops the missing vault and tag filters instead of erroring
    await userEvent.click(searchRow);

    expect(useItemsUI.getState().query).toBe('findme');
    const r = useApp.getState().route;
    expect(r.name).toBe('items');
    if (r.name === 'items') {
      // Missing vault & tag were dropped, fell back to all
      expect(r.filter.kind).toBe('all');
      expect(r.filter.vaultId).toBeUndefined();
      expect(r.filter.tag).toBeUndefined();
    }
  });

  it('renders one search icon with tooltip listing saved searches in collapsed icon rail', async () => {
    const s1: SavedSearch = { id: 's1', name: 'Work Logins', query: 'work' };
    const s2: SavedSearch = { id: 's2', name: 'Bank Cards', query: 'bank' };
    localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify([s1, s2]));
    useSavedSearches.getState().reload();

    useApp.setState({ sidebarCollapsed: true });

    render(
      <TooltipProvider>
        <Sidebar />
      </TooltipProvider>
    );

    const railBtn = await screen.findByTestId('sidebar-rail-saved-searches');
    expect(railBtn).toBeTruthy();
    expect(railBtn.getAttribute('aria-label')).toContain('Work Logins, Bank Cards');
  });

  it('allows saving from Command Palette when query is active', async () => {
    act(() => {
      useItemsUI.setState({ query: 'prod keys' });
    });

    render(
      <TooltipProvider>
        <Toolbar />
        <CommandPalette open={true} onOpenChange={() => {}} />
      </TooltipProvider>
    );

    // Search for save command in palette
    const palInput = screen.getByPlaceholderText(/Type a command/i);
    fireEvent.change(palInput, { target: { value: 'Save' } });

    const saveOption = await screen.findByText('Save current search');
    expect(saveOption).toBeTruthy();
  });
});
