import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { useApp } from '../../lib/store';
import { useItemsUI } from './state';
import { ItemDetailPane } from './ItemDetail';
afterEach(cleanup);
describe('detail empty states', () => {
  it('empty vault shows "This vault is empty" + New Item, not a skeleton (even with a stale selection)', () => {
    useApp.setState({ route: { name: 'items', filter: { kind: 'vault', vaultId: 'v9' } }, selectedItemId: 'stale-id' });
    useItemsUI.setState({ items: [], loading: false });
    render(<ItemDetailPane />);
    expect(screen.getByTestId('empty-vault')).toBeTruthy();
    expect(screen.getByRole('button', { name: /New Item/ })).toBeTruthy();
  });
  it('non-empty list with nothing selected shows "Select an item"', () => {
    useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, selectedItemId: null });
    render(<ItemDetailPane />);
    expect(screen.getByText('Select an item')).toBeTruthy();
  });
});
