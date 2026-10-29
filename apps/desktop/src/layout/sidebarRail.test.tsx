import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { Sidebar } from './Sidebar';
import { TooltipProvider } from '../ui';
import { translate } from '../i18n';
import type { VaultInfo } from '../api/types';

const en = (k: Parameters<typeof translate>[1]) => translate('en', k);

const VAULTS: VaultInfo[] = [
  { id: 'v1', name: 'Personal', description: null, itemCount: 2, shared: false, members: [], icon: 'vault', color: '#64748b' },
  { id: 'v2', name: 'Work', description: null, itemCount: 1, shared: false, members: [], icon: null, color: null },
];

const renderSidebar = () => render(<TooltipProvider><Sidebar /></TooltipProvider>);

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('sidebar icon rail', () => {
  beforeEach(() => {
    vi.spyOn(api, 'listVaults').mockResolvedValue(VAULTS);
    vi.spyOn(api, 'listTags').mockResolvedValue(['work']);
    vi.spyOn(api, 'listItems').mockResolvedValue([]);
    useApp.setState({ route: { name: 'items', filter: { kind: 'all', sort: 'updated' } }, modal: null, sidebarCollapsed: true, itemsVersion: 0 });
  });

  it('renders an icon button with aria-label for every entry and hides text labels', async () => {
    renderSidebar();
    const sidebar = screen.getByTestId('sidebar');
    expect(sidebar.dataset.collapsed).toBe('true');
    await screen.findByRole('button', { name: 'Work' });
    for (const name of [en('nav.allItems'), en('nav.favorites'), en('nav.watchtower'), en('nav.archive'), en('nav.deleted'), 'Personal', 'Work', 'work']) {
      const btn = screen.getByRole('button', { name });
      expect(btn.querySelector('svg')).not.toBeNull(); // icon (vault without icon falls back to the default)
      expect(btn.textContent).toBe(''); // no visible label
    }
    expect(within(sidebar).queryByText(en('nav.allItems'))).toBeNull();
    expect(within(sidebar).queryByText('Work')).toBeNull();
  });

  it('marks the active entry and keeps click behaviour', async () => {
    renderSidebar();
    const all = screen.getByRole('button', { name: en('nav.allItems') });
    expect(all.getAttribute('aria-current')).toBe('page');
    expect(all.className).toContain('text-accent');
    fireEvent.click(await screen.findByRole('button', { name: 'Work' }));
    const r = useApp.getState().route;
    expect(r.name === 'items' && r.filter.vaultId).toBe('v2');
    expect(screen.getByRole('button', { name: 'Work' }).getAttribute('aria-current')).toBe('page');
  });

  it('shows a tooltip with the entry name on keyboard focus', async () => {
    renderSidebar();
    const fav = screen.getByRole('button', { name: en('nav.favorites') });
    fav.focus();
    expect(document.activeElement).toBe(fav);
    await waitFor(() => expect(screen.getByRole('tooltip').textContent).toContain(en('nav.favorites')));
  });

  it('toggle switches between rail and expanded sidebar', async () => {
    renderSidebar();
    fireEvent.click(screen.getByRole('button', { name: en('sidebar.expand') }));
    expect(useApp.getState().sidebarCollapsed).toBe(false);
    expect(screen.getByTestId('sidebar').dataset.collapsed).toBe('false');
    expect(within(screen.getByTestId('sidebar')).getByText(en('nav.allItems'))).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: en('sidebar.collapse') }));
    expect(useApp.getState().sidebarCollapsed).toBe(true);
    expect(within(screen.getByTestId('sidebar')).queryByText(en('nav.allItems'))).toBeNull();
  });
});
