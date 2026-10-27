import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act } from '@testing-library/react';
import { defaultRoute, useApp } from './store';
import { api, isLockedError, VaultLockedError } from '../api/client';
import { mock } from '../api/mock';
import { toast } from '../ui/toast';

const settings = { name: 'settings', tab: 'vaults' } as const;

describe('vault lock handling', () => {
  beforeEach(() => { act(() => { useApp.setState({ locked: false, route: settings, resumeRoute: null, selectedItemId: 'i1', modal: { id: 'x' }, editing: true }); }); });

  it('lock clears UI state and unlock returns to the previous page', () => {
    act(() => useApp.getState().lock());
    let s = useApp.getState();
    expect(s.locked).toBe(true);
    expect(s.selectedItemId).toBeNull();
    expect(s.modal).toBeNull();
    expect(s.editing).toBe(false);
    expect(s.route).toEqual(defaultRoute);
    act(() => useApp.getState().lock()); // second lock keeps the remembered page
    act(() => useApp.getState().unlock());
    s = useApp.getState();
    expect(s.locked).toBe(false);
    expect(s.route).toEqual(settings);
    expect(s.resumeRoute).toBeNull();
  });

  it('a "vault is locked" command error switches to the unlock screen without the raw message', async () => {
    const spy = vi.spyOn(mock as any, 'list_vaults').mockImplementation(() => Promise.reject('vault is locked'));
    await expect(api.listVaults()).rejects.toBeInstanceOf(VaultLockedError);
    expect(useApp.getState().locked).toBe(true);
    expect(useApp.getState().resumeRoute).toEqual(settings);
    spy.mockRestore();
  });

  it('other errors pass through unchanged', async () => {
    const spy = vi.spyOn(mock as any, 'list_vaults').mockImplementation(() => Promise.reject(new Error('disk full')));
    await expect(api.listVaults()).rejects.toThrow('disk full');
    expect(useApp.getState().locked).toBe(false);
    spy.mockRestore();
  });

  it('detects the backend error shapes and suppresses lock toasts', async () => {
    expect(isLockedError('vault is locked')).toBe(true);
    expect(isLockedError(new Error('list_vaults: vault is locked'))).toBe(true);
    expect(isLockedError('nope')).toBe(false);
    const { useToasts } = await import('../ui/toast') as any;
    expect(useToasts).toBeTruthy(); { const n = useToasts.getState().list.length; toast('vault is locked', 'danger'); expect(useToasts.getState().list.length).toBe(n); }
  });
});
