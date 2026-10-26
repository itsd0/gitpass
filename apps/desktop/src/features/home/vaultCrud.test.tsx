import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import type { VaultInfo } from '../../api/types';
import { DeleteVaultDialog, EditVaultDialog } from './VaultDialogs';
import { groupRows } from '../items/grouping';
import { rowHeight, HEADER_H, ITEM_H } from '../items/ItemList';
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const V: VaultInfo = { id: 'v1', name: 'Work', description: null, itemCount: 3, shared: false, members: [], icon: 'vault', color: '#64748b' };

describe('vault CRUD dialogs', () => {
  it('delete requires typing the exact vault name', async () => {
    const spy = vi.spyOn(api, 'deleteVault').mockResolvedValue(undefined); const onDeleted = vi.fn(); const u = userEvent.setup();
    render(<DeleteVaultDialog vault={V} isLast={false} onOpenChange={() => {}} onDeleted={onDeleted} />);
    const btn = screen.getByRole('button', { name: 'Delete vault permanently' });
    expect(screen.getByText(/all 3 items/)).toBeTruthy();
    await u.type(screen.getByLabelText('Confirm vault name'), 'work');
    expect((btn as HTMLButtonElement).disabled).toBe(true);
    await u.clear(screen.getByLabelText('Confirm vault name')); await u.type(screen.getByLabelText('Confirm vault name'), 'Work');
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
    const spy = vi.spyOn(api, 'updateVault').mockResolvedValue({ ...V, name: 'Job' }); const u = userEvent.setup();
    render(<EditVaultDialog vault={V} onOpenChange={() => {}} onSaved={() => {}} />);
    const name = screen.getByLabelText('Vault name'); await u.clear(name); await u.type(name, 'Job');
    await u.click(screen.getByRole('button', { name: 'Icon star' })); await u.click(screen.getByRole('button', { name: 'Color #ef4444' }));
    await u.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', name: 'Job', description: null, icon: 'star', color: '#ef4444' }));
  });
});

describe('item list layout', () => {
  it('row offsets are exact sums of fixed heights (no overlap / gaps)', () => {
    const mk = (id: string, d: string, subtitle: string) => ({ id, vaultId: 'v', category: 'login' as const, title: id, subtitle, favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: d, createdAt: d });
    const rows = groupRows([mk('a', '2026-11-02T00:00:00Z', 'user@x'), mk('b', '2026-11-01T00:00:00Z', ''), mk('c', '2026-10-01T00:00:00Z', 'sub')], 'updated');
    expect(rows.map((r) => r.type)).toEqual(['header', 'item', 'item', 'header', 'item']);
    expect(rows.map(rowHeight)).toEqual([HEADER_H, ITEM_H, ITEM_H, HEADER_H, ITEM_H]);
  });
});
