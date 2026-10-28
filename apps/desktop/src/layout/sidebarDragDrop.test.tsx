import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// jsdom has no layout, so render every virtual row.
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0,
    scrollToIndex: () => {},
    getVirtualItems: () => Array.from({ length: o.count }, (_, i) => ({ index: i, start: 0, size: o.estimateSize(i), key: i })),
  }),
}));

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { useItemsUI } from '../features/items/state';
import { useDragStore, canDropOnVault, getDragPreviewText } from '../features/items/dragState';
import { Sidebar } from './Sidebar';
import { ItemList } from '../features/items/ItemList';
import { MoveToVaultModal, AddTagModal } from '../features/items/ItemModals';
import { PaletteHost } from '../features/palette';
import { Toaster, TooltipProvider, useToasts } from '../ui';
import type { ItemSummary, VaultInfo } from '../api/types';
import { translate } from '../i18n';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useToasts.setState({ list: [] });
  useDragStore.getState().endDrag();
});

const VAULTS: VaultInfo[] = [
  { id: 'v1', name: 'Personal', description: null, itemCount: 2, shared: false, members: [], icon: 'vault', color: '#64748b' },
  { id: 'v2', name: 'Work', description: null, itemCount: 1, shared: false, members: [], icon: 'briefcase', color: '#f97316' },
  { id: 'v-ro', name: 'Archive Vault', description: null, itemCount: 0, shared: true, members: [], icon: 'archive', color: '#888', ...( { readOnly: true } as any) },
];

const ITEMS: ItemSummary[] = [
  {
    id: 'item-1',
    vaultId: 'v1',
    category: 'login',
    title: 'Alpha Login',
    subtitle: 'user@alpha.com',
    favorite: false,
    archived: false,
    deletedAt: null,
    tags: ['work'],
    updatedAt: new Date().toISOString(),
    createdAt: new Date().toISOString(),
  },
  {
    id: 'item-2',
    vaultId: 'v1',
    category: 'login',
    title: 'Beta Login',
    subtitle: 'user@beta.com',
    favorite: false,
    archived: false,
    deletedAt: null,
    tags: [],
    updatedAt: new Date().toISOString(),
    createdAt: new Date().toISOString(),
  },
];

describe('drag and drop on sidebar', () => {
  beforeEach(() => {
    vi.spyOn(api, 'listVaults').mockResolvedValue(VAULTS);
    vi.spyOn(api, 'listTags').mockResolvedValue(['work', 'projects']);
    vi.spyOn(api, 'listItems').mockResolvedValue(ITEMS);

    useItemsUI.setState({ items: ITEMS, loading: false });
    useApp.setState({
      route: { name: 'items', filter: { kind: 'all', sort: 'updated' } },
      itemsVersion: 0,
      locked: false,
      selectedItemId: 'item-1',
      multiIds: [],
      modal: null,
    });
  });

  it('canDropOnVault correctly allows or refuses vaults', () => {
    // Single item in v1 cannot drop on v1 (same vault)
    expect(canDropOnVault(VAULTS[0], [ITEMS[0]])).toBe(false);
    // Single item in v1 can drop on v2
    expect(canDropOnVault(VAULTS[1], [ITEMS[0]])).toBe(true);
    // Cannot drop on read-only vault
    expect(canDropOnVault(VAULTS[2], [ITEMS[0]])).toBe(false);
  });

  it('getDragPreviewText formats single and multi items', () => {
    const t = (k: any, v?: any) => translate('en', k, v);
    expect(getDragPreviewText([ITEMS[0]], t)).toBe('Alpha Login');
    expect(getDragPreviewText([ITEMS[0], ITEMS[1]], t)).toBe('2 items');
  });

  it('dragging single item shows title in preview and dropping on vault moves item with undo', async () => {
    const bulkSpy = vi.spyOn(api, 'bulkItems').mockResolvedValue({
      count: 1,
      items: [{ vaultId: 'v2', itemId: 'item-1' }],
    });

    render(
      <TooltipProvider>
        <Toaster />
        <Sidebar />
        <ItemList />
      </TooltipProvider>
    );

    // Find the item row in ItemList
    const rows = await screen.findAllByRole('option');
    expect(rows.length).toBeGreaterThan(0);

    const dataTransfer = {
      data: {} as Record<string, string>,
      setData(k: string, v: string) { this.data[k] = v; },
      getData(k: string) { return this.data[k] ?? ''; },
      dropEffect: 'none',
      effectAllowed: 'none',
      setDragImage: vi.fn(),
    };

    // 1. Drag start on first item
    fireEvent.dragStart(rows[0], { dataTransfer });
    expect(useDragStore.getState().dragged?.items).toHaveLength(1);
    expect(useDragStore.getState().dragged?.items[0].id).toBe('item-1');

    const preview = screen.getByTestId('drag-preview');
    expect(preview.textContent).toBe('Alpha Login');

    // 2. Drag over destination vault (v2 - Work)
    const vaultRows = screen.getAllByTestId('sidebar-vault');
    const workVault = vaultRows.find((r) => r.getAttribute('data-vault-id') === 'v2')!;
    expect(workVault).toBeTruthy();

    fireEvent.dragOver(workVault, { dataTransfer });
    expect(dataTransfer.dropEffect).toBe('move');
    // Hover highlight applied
    const workButton = workVault.querySelector('button');
    expect(workButton?.className).toContain('ring-accent');

    // 3. Drop on destination vault
    fireEvent.drop(workVault, { dataTransfer });

    expect(bulkSpy).toHaveBeenCalledWith({
      req: {
        action: 'move',
        toVaultId: 'v2',
        items: [{ vaultId: 'v1', itemId: 'item-1' }],
      },
    });

    // 4. Undo toast appears
    const undoBtn = await screen.findByRole('button', { name: /Undo/i });
    expect(undoBtn).toBeTruthy();

    // 5. Click undo to reverse move
    fireEvent.click(undoBtn);
    await waitFor(() => {
      expect(bulkSpy).toHaveBeenCalledWith({
        req: {
          action: 'move',
          toVaultId: 'v1',
          items: [{ vaultId: 'v2', itemId: 'item-1' }],
        },
      });
    });
  });

  it('dragging multi-selection shows "N items" preview and moves all selected items', async () => {
    useApp.setState({ multiIds: ['item-1', 'item-2'], selectedItemId: 'item-1' });

    const bulkSpy = vi.spyOn(api, 'bulkItems').mockResolvedValue({
      count: 2,
      items: [
        { vaultId: 'v2', itemId: 'item-1' },
        { vaultId: 'v2', itemId: 'item-2' },
      ],
    });

    render(
      <TooltipProvider>
        <Toaster />
        <Sidebar />
        <ItemList />
      </TooltipProvider>
    );

    const rows = await screen.findAllByRole('option');
    const dataTransfer = {
      data: {} as Record<string, string>,
      setData(k: string, v: string) { this.data[k] = v; },
      getData(k: string) { return this.data[k] ?? ''; },
      dropEffect: 'none',
      effectAllowed: 'none',
      setDragImage: vi.fn(),
    };

    fireEvent.dragStart(rows[0], { dataTransfer });
    expect(useDragStore.getState().dragged?.items).toHaveLength(2);

    const preview = screen.getByTestId('drag-preview');
    expect(preview.textContent).toBe('2 items');

    const vaultRows = screen.getAllByTestId('sidebar-vault');
    const workVault = vaultRows.find((r) => r.getAttribute('data-vault-id') === 'v2')!;

    fireEvent.dragOver(workVault, { dataTransfer });
    fireEvent.drop(workVault, { dataTransfer });

    expect(bulkSpy).toHaveBeenCalledWith({
      req: {
        action: 'move',
        toVaultId: 'v2',
        items: [
          { vaultId: 'v1', itemId: 'item-1' },
          { vaultId: 'v1', itemId: 'item-2' },
        ],
      },
    });
  });

  it('refuses drop on same vault and read-only vault without highlight or API call', async () => {
    const bulkSpy = vi.spyOn(api, 'bulkItems');

    render(
      <TooltipProvider>
        <Toaster />
        <Sidebar />
        <ItemList />
      </TooltipProvider>
    );

    const rows = await screen.findAllByRole('option');
    const dataTransfer = {
      data: {} as Record<string, string>,
      setData(k: string, v: string) { this.data[k] = v; },
      getData(k: string) { return this.data[k] ?? ''; },
      dropEffect: 'none',
      effectAllowed: 'none',
      setDragImage: vi.fn(),
    };

    // Start dragging item-1 (which belongs to v1)
    fireEvent.dragStart(rows[0], { dataTransfer });

    const vaultRows = screen.getAllByTestId('sidebar-vault');
    const sameVault = vaultRows.find((r) => r.getAttribute('data-vault-id') === 'v1')!;
    const roVault = vaultRows.find((r) => r.getAttribute('data-vault-id') === 'v-ro')!;

    // 1. Drag over same vault (v1)
    fireEvent.dragOver(sameVault, { dataTransfer });
    expect(dataTransfer.dropEffect).toBe('none');
    expect(sameVault.className).toContain('cursor-not-allowed');
    expect(sameVault.querySelector('button')?.className).not.toContain('ring-accent');

    fireEvent.drop(sameVault, { dataTransfer });
    expect(bulkSpy).not.toHaveBeenCalled();

    // 2. Drag over read-only vault (v-ro)
    fireEvent.dragOver(roVault, { dataTransfer });
    expect(dataTransfer.dropEffect).toBe('none');
    expect(roVault.className).toContain('cursor-not-allowed');
    expect(roVault.querySelector('button')?.className).not.toContain('ring-accent');

    fireEvent.drop(roVault, { dataTransfer });
    expect(bulkSpy).not.toHaveBeenCalled();
  });

  it('dropping on a tag adds the tag and shows undo toast', async () => {
    const bulkSpy = vi.spyOn(api, 'bulkItems').mockResolvedValue({
      count: 1,
      items: [{ vaultId: 'v1', itemId: 'item-1' }],
    });

    render(
      <TooltipProvider>
        <Toaster />
        <Sidebar />
        <ItemList />
      </TooltipProvider>
    );

    const rows = await screen.findAllByRole('option');
    const dataTransfer = {
      data: {} as Record<string, string>,
      setData(k: string, v: string) { this.data[k] = v; },
      getData(k: string) { return this.data[k] ?? ''; },
      dropEffect: 'none',
      effectAllowed: 'none',
      setDragImage: vi.fn(),
    };

    fireEvent.dragStart(rows[0], { dataTransfer });

    const tagRows = await screen.findAllByTestId('sidebar-tag');
    const projectsTag = tagRows.find((r) => r.getAttribute('data-tag-name') === 'projects')!;
    expect(projectsTag).toBeTruthy();

    fireEvent.dragOver(projectsTag, { dataTransfer });
    expect(dataTransfer.dropEffect).toBe('copy');
    expect(projectsTag.querySelector('button')?.className).toContain('ring-accent');

    fireEvent.drop(projectsTag, { dataTransfer });
    expect(bulkSpy).toHaveBeenCalledWith({
      req: {
        action: 'add_tag',
        tag: 'projects',
        items: [{ vaultId: 'v1', itemId: 'item-1' }],
      },
    });

    const undoBtn = await screen.findByRole('button', { name: /Undo/i });
    expect(undoBtn).toBeTruthy();

    fireEvent.click(undoBtn);
    await waitFor(() => {
      expect(bulkSpy).toHaveBeenCalledWith({
        req: {
          action: 'remove_tag',
          tag: 'projects',
          items: [{ vaultId: 'v1', itemId: 'item-1' }],
        },
      });
    });
  });

  it('Esc cancels an active drag and clears highlights', async () => {
    render(
      <TooltipProvider>
        <Sidebar />
        <ItemList />
      </TooltipProvider>
    );

    const rows = await screen.findAllByRole('option');
    const dataTransfer = {
      data: {} as Record<string, string>,
      setData(k: string, v: string) { this.data[k] = v; },
      getData(k: string) { return this.data[k] ?? ''; },
      dropEffect: 'none',
      effectAllowed: 'none',
      setDragImage: vi.fn(),
    };

    fireEvent.dragStart(rows[0], { dataTransfer });
    expect(useDragStore.getState().dragged).not.toBeNull();

    const vaultRows = screen.getAllByTestId('sidebar-vault');
    const workVault = vaultRows.find((r) => r.getAttribute('data-vault-id') === 'v2')!;
    fireEvent.dragOver(workVault, { dataTransfer });
    expect(workVault.querySelector('button')?.className).toContain('ring-accent');

    // Press Escape
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(useDragStore.getState().dragged).toBeNull();

    await waitFor(() => {
      expect(workVault.querySelector('button')?.className).not.toContain('ring-accent');
    });
  });

  it('keyboard command palette alternative: Move to vault', async () => {
    const bulkSpy = vi.spyOn(api, 'bulkItems').mockResolvedValue({
      count: 1,
      items: [{ vaultId: 'v2', itemId: 'item-1' }],
    });

    render(
      <TooltipProvider>
        <Toaster />
        <Sidebar />
        <MoveToVaultModal />
      </TooltipProvider>
    );

    act(() => {
      useApp.getState().openModal('move-to-vault');
    });
    expect(await screen.findByRole('dialog', { name: /Move to vault/i })).toBeTruthy();

    const workOption = screen.getByTestId('move-vault-option-v2');
    fireEvent.click(workOption);

    await waitFor(() => {
      expect(bulkSpy).toHaveBeenCalledWith({
        req: {
          action: 'move',
          toVaultId: 'v2',
          items: [{ vaultId: 'v1', itemId: 'item-1' }],
        },
      });
    });
  });

  it('keyboard command palette alternative: Add tag', async () => {
    const bulkSpy = vi.spyOn(api, 'bulkItems').mockResolvedValue({
      count: 1,
      items: [{ vaultId: 'v1', itemId: 'item-1' }],
    });

    render(
      <TooltipProvider>
        <Toaster />
        <Sidebar />
        <AddTagModal />
      </TooltipProvider>
    );

    act(() => {
      useApp.getState().openModal('add-tag');
    });
    expect(await screen.findByRole('dialog', { name: /Add tag/i })).toBeTruthy();

    const input = screen.getByTestId('add-tag-modal-input');
    fireEvent.change(input, { target: { value: 'mobile' } });
    fireEvent.submit(screen.getByRole('dialog', { name: /Add tag/i }).querySelector('form')!);

    await waitFor(() => {
      expect(bulkSpy).toHaveBeenCalledWith({
        req: {
          action: 'add_tag',
          tag: 'mobile',
          items: [{ vaultId: 'v1', itemId: 'item-1' }],
        },
      });
    });
  });
});
