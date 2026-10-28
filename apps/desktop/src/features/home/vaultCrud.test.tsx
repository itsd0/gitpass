import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import type { VaultInfo } from '../../api/types';
import { DeleteVaultDialog, EditVaultDialog } from './VaultDialogs';
import { NewVaultDialog } from './NewVaultDialog';
import { Sidebar } from '../../layout/Sidebar';
import { Toaster, TooltipProvider, useToasts } from '../../ui';
import { useApp } from '../../lib/store';
import { groupRows } from '../items/grouping';
import { rowHeight, HEADER_H, ITEM_H } from '../items/ItemList';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useToasts.setState({ list: [] });
});

const V: VaultInfo = {
  id: 'v1',
  name: 'Work',
  description: null,
  itemCount: 3,
  shared: false,
  members: [],
  icon: 'vault',
  color: '#64748b',
};

describe('vault CRUD dialogs', () => {
  it('delete requires typing the exact vault name', async () => {
    const spy = vi.spyOn(api, 'deleteVault').mockResolvedValue(undefined);
    const onDeleted = vi.fn();
    const u = userEvent.setup();
    render(<DeleteVaultDialog vault={V} isLast={false} onOpenChange={() => {}} onDeleted={onDeleted} />);
    const btn = screen.getByRole('button', { name: 'Delete vault permanently' });
    expect(screen.getByText(/all 3 items/)).toBeTruthy();
    await u.type(screen.getByLabelText('Confirm vault name'), 'work');
    expect((btn as HTMLButtonElement).disabled).toBe(true);
    await u.clear(screen.getByLabelText('Confirm vault name'));
    await u.type(screen.getByLabelText('Confirm vault name'), 'Work');
    expect((btn as HTMLButtonElement).disabled).toBe(false);
    await u.click(btn);
    await waitFor(() => expect(onDeleted).toHaveBeenCalledWith('v1'));
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', confirmName: 'Work' });
  });

  it('blocks deleting the last vault', () => {
    render(<DeleteVaultDialog vault={V} isLast onOpenChange={() => {}} onDeleted={() => {}} />);
    expect(screen.getByRole('alert').textContent).toMatch(/only vault/);
    expect(screen.queryByRole('button', { name: 'Delete vault permanently' })).toBeNull();
  });

  it('edit renames and restyles', async () => {
    const spy = vi.spyOn(api, 'updateVault').mockResolvedValue({ ...V, name: 'Job' });
    const u = userEvent.setup();
    render(<EditVaultDialog vault={V} onOpenChange={() => {}} onSaved={() => {}} />);
    const name = screen.getByLabelText('Vault name');
    await u.clear(name);
    await u.type(name, 'Job');
    await u.click(screen.getByRole('button', { name: 'Icon star' }));
    await u.click(screen.getByRole('button', { name: 'Color #ef4444' }));
    await u.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({
        vaultId: 'v1',
        name: 'Job',
        description: null,
        icon: 'star',
        color: '#ef4444',
      })
    );
  });

  it('new vault dialog creates vault with name, description, icon and color', async () => {
    const createdVault: VaultInfo = {
      id: 'v2',
      name: 'Finance',
      description: 'Bank accounts',
      itemCount: 0,
      shared: false,
      members: [],
      icon: 'wallet',
      color: '#14b8a6',
    };
    const spy = vi.spyOn(api, 'createVault').mockResolvedValue(createdVault);
    const onCreated = vi.fn();
    const onOpenChange = vi.fn();
    const u = userEvent.setup();
    render(<NewVaultDialog open={true} onOpenChange={onOpenChange} onCreated={onCreated} />);

    expect(screen.getByRole('heading', { name: 'New vault' })).toBeTruthy();
    const nameInput = screen.getByLabelText('Vault name');
    expect(nameInput).toHaveFocus();

    const createBtn = screen.getByRole('button', { name: 'Create vault' });
    expect((createBtn as HTMLButtonElement).disabled).toBe(true);

    await u.type(nameInput, 'Finance');
    await u.type(screen.getByLabelText('Vault description'), 'Bank accounts');

    const walletBtn = screen.getByRole('button', { name: 'Icon wallet' });
    expect(walletBtn.getAttribute('aria-pressed')).toBe('false');
    await u.click(walletBtn);
    expect(walletBtn.getAttribute('aria-pressed')).toBe('true');

    const tealBtn = screen.getByRole('button', { name: 'Color #14b8a6' });
    expect(tealBtn.getAttribute('aria-pressed')).toBe('false');
    await u.click(tealBtn);
    expect(tealBtn.getAttribute('aria-pressed')).toBe('true');

    expect((createBtn as HTMLButtonElement).disabled).toBe(false);
    await u.click(createBtn);

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({
        name: 'Finance',
        description: 'Bank accounts',
        icon: 'wallet',
        color: '#14b8a6',
      })
    );
    expect(onCreated).toHaveBeenCalledWith(createdVault);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('submits on Enter and closes on Escape', async () => {
    const createdVault: VaultInfo = {
      id: 'v3',
      name: 'Quick',
      description: null,
      itemCount: 0,
      shared: false,
      members: [],
      icon: 'vault',
      color: '#a855f7',
    };
    const spy = vi.spyOn(api, 'createVault').mockResolvedValue(createdVault);
    const onCreated = vi.fn();
    const onOpenChange = vi.fn();
    const u = userEvent.setup();
    const { rerender } = render(
      <NewVaultDialog open={true} onOpenChange={onOpenChange} onCreated={onCreated} />
    );

    // Press Escape to close
    await u.keyboard('{Escape}');
    expect(onOpenChange).toHaveBeenCalledWith(false);

    // Re-render and type name then press Enter
    rerender(<NewVaultDialog open={true} onOpenChange={onOpenChange} onCreated={onCreated} />);
    const nameInput = screen.getByLabelText('Vault name');
    await u.type(nameInput, 'Quick{Enter}');

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({
        name: 'Quick',
        description: null,
        icon: 'vault',
        color: '#a855f7',
      })
    );
    expect(onCreated).toHaveBeenCalledWith(createdVault);
  });

  it('displays error alert when createVault fails', async () => {
    vi.spyOn(api, 'createVault').mockRejectedValue(new Error('Name already taken'));
    const u = userEvent.setup();
    render(<NewVaultDialog open={true} onOpenChange={() => {}} onCreated={() => {}} />);

    await u.type(screen.getByLabelText('Vault name'), 'Work');
    await u.click(screen.getByRole('button', { name: 'Create vault' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/Name already taken/);
  });

  it('creating a vault from sidebar shows description/icon/colour and navigates to the vault', async () => {
    const newV: VaultInfo = {
      id: 'v_new',
      name: 'Personal Vault',
      description: 'My stuff',
      itemCount: 0,
      shared: false,
      members: [],
      icon: 'star',
      color: '#ef4444',
    };
    vi.spyOn(api, 'listVaults').mockResolvedValue([V]);
    vi.spyOn(api, 'listTags').mockResolvedValue([]);
    const createSpy = vi.spyOn(api, 'createVault').mockResolvedValue(newV);

    useApp.setState({
      route: { name: 'items', filter: { kind: 'all', sort: 'updated' } },
      itemsVersion: 0,
      locked: false,
    });

    const u = userEvent.setup();
    render(
      <TooltipProvider>
        <Toaster />
        <Sidebar />
      </TooltipProvider>
    );

    // Click the "+" button in sidebar
    const addVaultBtn = screen.getByRole('button', { name: 'New vault' });
    await u.click(addVaultBtn);

    // Verify NewVaultDialog opened with description, icon and color fields
    expect(await screen.findByRole('heading', { name: 'New vault' })).toBeTruthy();
    expect(screen.getByLabelText('Vault name')).toBeTruthy();
    expect(screen.getByLabelText('Vault description')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Icon star' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Color #ef4444' })).toBeTruthy();

    // Fill in fields
    await u.type(screen.getByLabelText('Vault name'), 'Personal Vault');
    await u.type(screen.getByLabelText('Vault description'), 'My stuff');
    await u.click(screen.getByRole('button', { name: 'Icon star' }));
    await u.click(screen.getByRole('button', { name: 'Color #ef4444' }));

    // Submit
    await u.click(screen.getByRole('button', { name: 'Create vault' }));

    await waitFor(() => {
      expect(createSpy).toHaveBeenCalledWith({
        name: 'Personal Vault',
        description: 'My stuff',
        icon: 'star',
        color: '#ef4444',
      });
    });

    // Check that route navigated to the new vault
    await waitFor(() => {
      expect(useApp.getState().route).toEqual({
        name: 'items',
        filter: {
          kind: 'vault',
          vaultId: 'v_new',
          sort: 'updated',
        },
      });
    });
  });
});

describe('item list layout', () => {
  it('row offsets are exact sums of fixed heights (no overlap / gaps)', () => {
    const mk = (id: string, d: string, subtitle: string) => ({
      id,
      vaultId: 'v',
      category: 'login' as const,
      title: id,
      subtitle,
      favorite: false,
      archived: false,
      deletedAt: null,
      tags: [],
      updatedAt: d,
      createdAt: d,
    });
    const rows = groupRows(
      [mk('a', '2026-11-02T00:00:00Z', 'user@x'), mk('b', '2026-11-01T00:00:00Z', ''), mk('c', '2026-10-01T00:00:00Z', 'sub')],
      'updated'
    );
    expect(rows.map((r) => r.type)).toEqual(['header', 'item', 'item', 'header', 'item']);
    expect(rows.map(rowHeight)).toEqual([HEADER_H, ITEM_H, ITEM_H, HEADER_H, ITEM_H]);
  });
});
