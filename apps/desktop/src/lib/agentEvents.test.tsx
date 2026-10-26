import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { openItemFromAgent, useAgentEvents } from './agentEvents';
import { defaultRoute, useApp } from './store';

const other = { name: 'items', filter: { kind: 'favorites', sort: 'updated' } } as any;

describe('agentEvents (extension open_in_app)', () => {
  beforeEach(() => { act(() => { useApp.setState({ locked: false, selectedItemId: null, route: other, editing: true }); }); });

  it('selects the item and resets the route when unlocked', () => {
    openItemFromAgent({ vaultId: 'v1', itemId: 'i1' });
    const s = useApp.getState();
    expect(s.selectedItemId).toBe('i1');
    expect(s.route).toEqual(defaultRoute);
    expect(s.editing).toBe(false);
  });

  it('ignores empty or malformed payloads', () => {
    openItemFromAgent(null); openItemFromAgent({}); openItemFromAgent({ itemId: '' }); openItemFromAgent({ itemId: 5 as any });
    expect(useApp.getState().selectedItemId).toBeNull();
    expect(useApp.getState().route).toBe(other);
  });

  it('reacts to the gitpass:open-item DOM event dispatched by agent_ui.rs', () => {
    const h = renderHook(() => useAgentEvents());
    act(() => { window.dispatchEvent(new CustomEvent('gitpass:open-item', { detail: { vaultId: 'v1', itemId: 'i2' } })); });
    expect(useApp.getState().selectedItemId).toBe('i2');
    h.unmount();
    act(() => { useApp.setState({ selectedItemId: null }); });
    window.dispatchEvent(new CustomEvent('gitpass:open-item', { detail: { itemId: 'i3' } }));
    expect(useApp.getState().selectedItemId).toBeNull(); // listener removed on unmount
  });

  it('defers the open while locked and shows the item after unlock', () => {
    act(() => { useApp.setState({ locked: true }); });
    const h = renderHook(() => useAgentEvents());
    act(() => { openItemFromAgent({ vaultId: 'v1', itemId: 'i4' }); });
    expect(useApp.getState().selectedItemId).toBeNull();
    act(() => { useApp.setState({ locked: false }); });
    expect(useApp.getState().selectedItemId).toBe('i4');
    h.unmount();
  });
});
