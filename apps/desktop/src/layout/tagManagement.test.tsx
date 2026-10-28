import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { Sidebar } from './Sidebar';
import { Toaster, TooltipProvider, useToasts } from '../ui';
import type { BulkResult, ItemSummary, VaultInfo } from '../api/types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useToasts.setState({ list: [] });
});

const V: VaultInfo[] = [
  { id: 'v1', name: 'Personal', description: null, itemCount: 5, shared: false, members: [], icon: 'vault', color: '#64748b' },
];

describe('sidebar tag management', () => {
  beforeEach(() => {
    vi.spyOn(api, 'listVaults').mockResolvedValue(V);
    useApp.setState({
      route: { name: 'items', filter: { kind: 'all', sort: 'updated' } },
      itemsVersion: 0,
      locked: false,
    });
  });

  it('opens context menu via right-click and keyboard (Shift+F10)', async () => {
    vi.spyOn(api, 'listTags').mockResolvedValue(['dev', 'work']);
    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);

    // Wait for tag to appear in sidebar
    const workBtn = await screen.findByRole('button', { name: /^work$/ });
    expect(workBtn).toBeTruthy();

    // 1. Right click on the tag row
    fireEvent.contextMenu(workBtn);

    // Menu items should be visible
    expect(await screen.findByRole('menuitem', { name: /Rename tag…/i })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: /Delete tag…/i })).toBeTruthy();

    // Dismiss menu by pressing Escape
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('menu')).toBeNull());

    // 2. Keyboard shortcut: Shift+F10 on focused tag
    workBtn.focus();
    fireEvent.keyDown(workBtn, { key: 'F10', shiftKey: true });

    expect(await screen.findByRole('menuitem', { name: /Rename tag…/i })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: /Delete tag…/i })).toBeTruthy();
  });

  it('inline rename: Enter saves new tag, updates route, and displays toast', async () => {
    const renameSpy = vi.spyOn(api, 'renameTag').mockResolvedValue({ count: 2, items: [] });
    vi.spyOn(api, 'listTags').mockResolvedValue(['work', 'personal']);
    useApp.setState({ route: { name: 'items', filter: { kind: 'tag', tag: 'work', sort: 'updated' } } });

    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
    const workBtn = await screen.findByRole('button', { name: /^work$/ });

    // Open context menu and choose Rename tag…
    fireEvent.contextMenu(workBtn);
    const renameItem = await screen.findByRole('menuitem', { name: /Rename tag…/i });
    fireEvent.click(renameItem);

    // Inline input field appears with current tag value
    const input = await screen.findByLabelText('Rename tag work') as HTMLInputElement;
    expect(input).toBeTruthy();
    expect(input.value).toBe('work');

    // Change value and submit via Enter
    fireEvent.change(input, { target: { value: 'career' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => {
      expect(renameSpy).toHaveBeenCalledWith({ old: 'work', new: 'career' });
    });

    // Toast shown
    expect(await screen.findByText('Renamed #work to #career')).toBeTruthy();

    // Active route updated to new tag name
    expect(useApp.getState().route).toEqual({
      name: 'items',
      filter: { kind: 'tag', tag: 'career', sort: 'updated' },
    });
  });

  it('inline rename: Esc cancels edit without saving', async () => {
    const renameSpy = vi.spyOn(api, 'renameTag');
    vi.spyOn(api, 'listTags').mockResolvedValue(['work']);

    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
    const workBtn = await screen.findByRole('button', { name: /^work$/ });

    fireEvent.contextMenu(workBtn);
    const renameItem = await screen.findByRole('menuitem', { name: /Rename tag…/i });
    fireEvent.click(renameItem);

    const input = await screen.findByLabelText('Rename tag work');
    fireEvent.change(input, { target: { value: 'canceled-name' } });
    fireEvent.keyDown(input, { key: 'Escape' });

    await waitFor(() => {
      expect(screen.queryByLabelText('Rename tag work')).toBeNull();
    });
    expect(renameSpy).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /^work$/ })).toBeTruthy();
  });

  it('inline rename: renaming to an existing tag prompts merge confirmation', async () => {
    const renameSpy = vi.spyOn(api, 'renameTag').mockResolvedValue({ count: 3, items: [] });
    vi.spyOn(api, 'listTags').mockResolvedValue(['work', 'job']);
    useApp.setState({ route: { name: 'items', filter: { kind: 'tag', tag: 'work', sort: 'updated' } } });

    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
    const workBtn = await screen.findByRole('button', { name: /^work$/ });

    fireEvent.contextMenu(workBtn);
    const renameItem = await screen.findByRole('menuitem', { name: /Rename tag…/i });
    fireEvent.click(renameItem);

    const input = await screen.findByLabelText('Rename tag work');
    fireEvent.change(input, { target: { value: 'job' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    // Merge confirmation dialog appears
    expect(await screen.findByText('Merge into #job?')).toBeTruthy();
    expect(screen.getByText('All items tagged #work will be retagged with #job.')).toBeTruthy();

    // Click Merge button (orange primary confirm button)
    const mergeBtn = screen.getByRole('button', { name: 'Merge' });
    expect(mergeBtn).toBeTruthy();
    fireEvent.click(mergeBtn);

    await waitFor(() => {
      expect(renameSpy).toHaveBeenCalledWith({ old: 'work', new: 'job' });
    });

    expect(await screen.findByText('Merged #work into #job')).toBeTruthy();
    expect(useApp.getState().route).toEqual({
      name: 'items',
      filter: { kind: 'tag', tag: 'job', sort: 'updated' },
    });
  });

  it('delete tag: shows confirm dialog with affected item count, deletes, and undo restores tag', async () => {
    const deleteResult: BulkResult = {
      count: 2,
      items: [
        { vaultId: 'v1', itemId: 'item-1' },
        { vaultId: 'v1', itemId: 'item-2' },
      ],
    };
    const deleteSpy = vi.spyOn(api, 'deleteTag').mockResolvedValue(deleteResult);
    const bulkSpy = vi.spyOn(api, 'bulkItems').mockResolvedValue({ count: 2, items: deleteResult.items });
    vi.spyOn(api, 'listTags').mockResolvedValue(['work', 'hobby']);
    vi.spyOn(api, 'listItems').mockResolvedValue([
      { id: 'item-1', vaultId: 'v1', category: 'login', title: 'A', subtitle: '', favorite: false, archived: false, deletedAt: null, tags: ['work'], updatedAt: '', createdAt: '' },
      { id: 'item-2', vaultId: 'v1', category: 'login', title: 'B', subtitle: '', favorite: false, archived: false, deletedAt: null, tags: ['work'], updatedAt: '', createdAt: '' },
    ] as ItemSummary[]);

    useApp.setState({ route: { name: 'items', filter: { kind: 'tag', tag: 'work', sort: 'updated' } } });

    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
    const workBtn = await screen.findByRole('button', { name: /^work$/ });

    // Open context menu and choose Delete tag…
    fireEvent.contextMenu(workBtn);
    const deleteItem = await screen.findByRole('menuitem', { name: /Delete tag…/i });
    fireEvent.click(deleteItem);

    // Delete confirmation dialog appears
    expect(await screen.findByText('Delete tag #work?')).toBeTruthy();
    expect(await screen.findByText('2 items will lose this tag. The items will not be deleted.')).toBeTruthy();

    // Confirm deletion
    const confirmDeleteBtn = screen.getByRole('button', { name: 'Delete tag' });
    fireEvent.click(confirmDeleteBtn);

    await waitFor(() => {
      expect(deleteSpy).toHaveBeenCalledWith({ name: 'work' });
    });

    // Route transitions to 'all' items
    expect(useApp.getState().route).toEqual({
      name: 'items',
      filter: { kind: 'all', sort: 'updated' },
    });

    // Undo toast is displayed with Undo button
    expect(await screen.findByText('Deleted tag #work')).toBeTruthy();
    const undoBtn = screen.getByRole('button', { name: /Undo/i });
    expect(undoBtn).toBeTruthy();

    // Click Undo
    fireEvent.click(undoBtn);

    await waitFor(() => {
      expect(bulkSpy).toHaveBeenCalledWith({
        req: {
          action: 'add_tag',
          tag: 'work',
          items: deleteResult.items,
        },
      });
    });

    expect(await screen.findByText('Action undone')).toBeTruthy();
  });
});

  it('inline rename: identical or empty name cancels without calling api', async () => {
    const renameSpy = vi.spyOn(api, 'renameTag');
    vi.spyOn(api, 'listTags').mockResolvedValue(['work']);

    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
    const workBtn = await screen.findByRole('button', { name: /^work$/ });

    fireEvent.contextMenu(workBtn);
    const renameItem = await screen.findByRole('menuitem', { name: /Rename tag…/i });
    fireEvent.click(renameItem);

    const input = await screen.findByLabelText('Rename tag work');
    // Same name
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => {
      expect(screen.queryByLabelText('Rename tag work')).toBeNull();
    });
    expect(renameSpy).not.toHaveBeenCalled();

    // Now try empty name
    const workBtn2 = screen.getByRole('button', { name: /^work$/ });
    fireEvent.contextMenu(workBtn2);
    const renameItem2 = await screen.findByRole('menuitem', { name: /Rename tag…/i });
    fireEvent.click(renameItem2);

    const input2 = await screen.findByLabelText('Rename tag work');
    fireEvent.change(input2, { target: { value: '   ' } });
    fireEvent.keyDown(input2, { key: 'Enter' });

    await waitFor(() => {
      expect(screen.queryByLabelText('Rename tag work')).toBeNull();
    });
    expect(renameSpy).not.toHaveBeenCalled();
  });

  it('merge dialog: cancel closes dialog without calling api', async () => {
    const renameSpy = vi.spyOn(api, 'renameTag');
    vi.spyOn(api, 'listTags').mockResolvedValue(['work', 'job']);

    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
    const workBtn = await screen.findByRole('button', { name: /^work$/ });

    fireEvent.contextMenu(workBtn);
    const renameItem = await screen.findByRole('menuitem', { name: /Rename tag…/i });
    fireEvent.click(renameItem);

    const input = await screen.findByLabelText('Rename tag work');
    fireEvent.change(input, { target: { value: 'job' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(await screen.findByText('Merge into #job?')).toBeTruthy();
    const cancelBtn = screen.getByRole('button', { name: 'Cancel' });
    fireEvent.click(cancelBtn);

    await waitFor(() => {
      expect(screen.queryByText('Merge into #job?')).toBeNull();
    });
    expect(renameSpy).not.toHaveBeenCalled();
  });

  it('delete dialog: cancel closes dialog without calling api', async () => {
    const deleteSpy = vi.spyOn(api, 'deleteTag');
    vi.spyOn(api, 'listTags').mockResolvedValue(['work']);
    vi.spyOn(api, 'listItems').mockResolvedValue([
      { id: 'item-1', vaultId: 'v1', category: 'login', title: 'A', subtitle: '', favorite: false, archived: false, deletedAt: null, tags: ['work'], updatedAt: '', createdAt: '' },
    ] as ItemSummary[]);

    render(<TooltipProvider><Toaster /><Sidebar /></TooltipProvider>);
    const workBtn = await screen.findByRole('button', { name: /^work$/ });

    fireEvent.contextMenu(workBtn);
    const deleteItem = await screen.findByRole('menuitem', { name: /Delete tag…/i });
    fireEvent.click(deleteItem);

    expect(await screen.findByText('Delete tag #work?')).toBeTruthy();
    expect(await screen.findByText('1 item will lose this tag. The item will not be deleted.')).toBeTruthy();

    const cancelBtn = screen.getByRole('button', { name: 'Cancel' });
    fireEvent.click(cancelBtn);

    await waitFor(() => {
      expect(screen.queryByText('Delete tag #work?')).toBeNull();
    });
    expect(deleteSpy).not.toHaveBeenCalled();
  });
