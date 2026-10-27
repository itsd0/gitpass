import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { onVaultChanged, useAgentEvents } from './agentEvents';
import { useApp } from './store';

describe('live sync: vault-changed', () => {
  beforeEach(() => { act(() => { useApp.setState({ locked: false, editing: false, itemsVersion: 0 }); }); });

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
});
