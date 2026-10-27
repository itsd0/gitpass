import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// jsdom has no layout engine, so render every virtual row for ItemList.
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0,
    scrollToIndex: () => {},
    getVirtualItems: () =>
      Array.from({ length: o.count }, (_, i) => ({ index: i, start: 0, size: o.estimateSize(i), key: i })),
  }),
}));

import { act, cleanup, render, renderHook, screen, waitFor } from '@testing-library/react';
import { onVaultChanged, useAgentEvents } from './agentEvents';
import { defaultRoute, useApp } from './store';
import { api } from '../api/client';
import App from '../App';
import { useItemsUI } from '../features/items/state';
import type { ItemDetail, ItemSummary, Settings, VaultInfo } from '../api/types';

describe('live sync: vault-changed', () => {
  beforeEach(() => {
    act(() => {
      useApp.setState({ locked: false, editing: false, itemsVersion: 0, route: defaultRoute, selectedItemId: null });
      useItemsUI.setState({ items: [], query: '', category: null, loading: false });
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('refetches items immediately when not editing', () => {
    onVaultChanged();
    expect(useApp.getState().itemsVersion).toBe(1);
  });

  it('ignores changes while locked', () => {
    act(() => { useApp.setState({ locked: true }); });
    onVaultChanged();
    expect(useApp.getState().itemsVersion).toBe(0);
  });

  it('defers the refetch until the editor closes (never clobbers an edit)', () => {
    renderHook(() => useAgentEvents());
    act(() => { useApp.setState({ editing: true }); });
    onVaultChanged();
    expect(useApp.getState().itemsVersion).toBe(0);
    act(() => { useApp.setState({ editing: false }); });
    expect(useApp.getState().itemsVersion).toBe(1);
  });

  it('refreshes the item list and shows new items, updates edited title in place, and switches to locked on lock event', async () => {
    const v1: VaultInfo = {
      id: 'v1',
      name: 'Personal',
      description: null,
      itemCount: 1,
      shared: false,
      members: [],
      icon: null,
      color: null,
    };
    const settings: Settings = {
      autoLockMinutes: 5,
      clipboardClearSecs: 90,
      theme: 'dark',
      language: 'en',
      biometricUnlock: false,
      quickAccessHotkey: 'CommandOrControl+Shift+Space',
      hibpCheck: false,
      sshApproveEachUse: false,
    };

    const initialItem: ItemSummary = {
      id: 'i1',
      vaultId: 'v1',
      category: 'login',
      title: 'GitHub',
      subtitle: 'octocat',
      favorite: false,
      archived: false,
      deletedAt: null,
      tags: [],
      updatedAt: '2026-10-28T01:00:00Z',
      createdAt: '2026-10-28T01:00:00Z',
    };

    const initialDetail: ItemDetail = {
      ...initialItem,
      sections: [
        {
          id: 'login',
          title: null,
          fields: [
            { id: 'username', label: 'Username', kind: 'text', value: 'octocat', masked: null },
            { id: 'password', label: 'Password', kind: 'concealed', value: null, masked: null },
          ],
        },
      ],
      notes: null,
      attachments: [],
      history: [{ version: 1, updatedAt: '2026-10-28T01:00:00Z' }],
      hasTotp: false,
      passwordStrength: null,
    };

    let currentItems: ItemSummary[] = [initialItem];
    const detailsMap: Record<string, ItemDetail> = { i1: initialDetail };

    vi.spyOn(api, 'status').mockResolvedValue({
      unlocked: true,
      vaultPath: '~/.gitpass',
      account: 'demo-account',
      vaultExists: true,
      biometricAvailable: false,
    });
    vi.spyOn(api, 'listVaults').mockResolvedValue([v1]);
    vi.spyOn(api, 'listTags').mockResolvedValue([]);
    vi.spyOn(api, 'getSettings').mockResolvedValue(settings);
    vi.spyOn(api, 'listItems').mockImplementation(async () => currentItems);
    vi.spyOn(api, 'getItem').mockImplementation(async ({ itemId }) => {
      const d = detailsMap[itemId];
      if (!d) throw new Error('not found');
      return d;
    });

    render(<App />);

    // 1. Initial render shows the initial item in list and detail pane
    expect(await screen.findByRole('heading', { level: 1, name: 'GitHub' })).toBeTruthy();
    expect(screen.getByTestId('crumb-item').textContent).toBe('GitHub');

    // 2. An item is added externally via extension/host: receiving vault-changed refreshes list & shows new item
    const newItem: ItemSummary = {
      id: 'i2',
      vaultId: 'v1',
      category: 'login',
      title: 'AWS Console',
      subtitle: 'admin',
      favorite: false,
      archived: false,
      deletedAt: null,
      tags: [],
      updatedAt: '2026-10-28T02:00:00Z',
      createdAt: '2026-10-28T02:00:00Z',
    };
    currentItems = [initialItem, newItem];

    // Trigger the change event (as dispatched on receipt of host/agent socket write)
    act(() => {
      window.dispatchEvent(new CustomEvent('gitpass:vault-changed'));
    });

    // The item list refreshes and shows the new item
    expect(await screen.findByText('AWS Console')).toBeTruthy();

    // 3. An edited title updates in place in both the list and the active detail pane
    const updatedItem: ItemSummary = {
      ...initialItem,
      title: 'GitHub Enterprise',
      updatedAt: '2026-10-28T03:00:00Z',
    };
    const updatedDetail: ItemDetail = {
      ...initialDetail,
      title: 'GitHub Enterprise',
      updatedAt: '2026-10-28T03:00:00Z',
    };
    currentItems = [updatedItem, newItem];
    detailsMap.i1 = updatedDetail;

    act(() => {
      window.dispatchEvent(new CustomEvent('gitpass:vault-changed'));
    });

    await waitFor(() => {
      expect(screen.getByRole('heading', { level: 1, name: 'GitHub Enterprise' })).toBeTruthy();
      expect(screen.getByTestId('crumb-item').textContent).toBe('GitHub Enterprise');
      expect(screen.getAllByText('GitHub Enterprise').length).toBeGreaterThanOrEqual(2);
    });

    // 4. A lock event switches the UI to locked
    act(() => {
      window.dispatchEvent(new CustomEvent('gitpass:locked'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('unlock')).toBeTruthy();
      expect(screen.getByText('Unlock gitpass')).toBeTruthy();
      expect(screen.queryByTestId('sidebar')).toBeNull();
    });
  });

  it('handles Tauri IPC events for vault-changed and locked', async () => {
    // Also verify the Tauri event listener wire
    const tauriListeners: Record<string, ((ev: any) => void)[]> = {};
    const unlistenSpy = vi.fn();

    vi.doMock('@tauri-apps/api/event', () => ({
      listen: vi.fn(async (name: string, cb: (ev: any) => void) => {
        tauriListeners[name] = tauriListeners[name] || [];
        tauriListeners[name].push(cb);
        return unlistenSpy;
      }),
    }));

    (window as any).__TAURI_INTERNALS__ = {};

    const h = renderHook(() => useAgentEvents());

    // Give import('@tauri-apps/api/event') a tick to resolve
    await waitFor(() => {
      expect(tauriListeners['vault-changed']).toBeDefined();
      expect(tauriListeners['locked']).toBeDefined();
    });

    // Emitting vault-changed via Tauri IPC refreshes items
    expect(useApp.getState().itemsVersion).toBe(0);
    act(() => {
      tauriListeners['vault-changed'].forEach((cb) => cb({ event: 'vault-changed', id: 1, payload: () => {} }));
    });
    expect(useApp.getState().itemsVersion).toBe(1);

    // Emitting locked via Tauri IPC switches state to locked
    expect(useApp.getState().locked).toBe(false);
    act(() => {
      tauriListeners['locked'].forEach((cb) => cb({ event: 'locked', id: 2, payload: 'idle' }));
    });
    expect(useApp.getState().locked).toBe(true);

    h.unmount();
    delete (window as any).__TAURI_INTERNALS__;
    vi.resetModules();
  });
});
