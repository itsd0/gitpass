import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

import { api } from '../../api/client';
import type { ItemDetail, ItemSummary, RelatedItem, VaultInfo } from '../../api/types';
import { useI18n } from '../../i18n';
import { defaultRoute, useApp } from '../../lib/store';
import { TooltipProvider, useToasts } from '../../ui';
import { ItemDetailPane } from './ItemDetail';
import { ItemEditor } from './ItemEditor';
import { RelatedItemPickerModal } from './RelatedItemPickerModal';
import { useItemsUI } from './state';

const mockVault: VaultInfo = {
  id: 'v1',
  name: 'Personal',
  description: 'Personal items',
  itemCount: 2,
  shared: false,
  members: [],
  icon: 'vault',
  color: '#f97316',
};

const mockSummaryA: ItemSummary = {
  id: 'item-a',
  vaultId: 'v1',
  category: 'login',
  title: 'GitHub Login',
  subtitle: 'octocat',
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt: '2026-10-30T00:00:00Z',
  createdAt: '2026-10-20T00:00:00Z',
};

const mockSummaryB: ItemSummary = {
  id: 'item-b',
  vaultId: 'v1',
  category: 'api_credential',
  title: 'GitHub API Key',
  subtitle: 'Personal access token',
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt: '2026-10-30T00:00:00Z',
  createdAt: '2026-10-20T00:00:00Z',
};

const mockSummaryC: ItemSummary = {
  id: 'item-c',
  vaultId: 'v1',
  category: 'server',
  title: 'Deploy Server',
  subtitle: 'ubuntu@10.0.0.1',
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt: '2026-10-30T00:00:00Z',
  createdAt: '2026-10-20T00:00:00Z',
};

const mockDetailWithRelated: ItemDetail = {
  ...mockSummaryA,
  sections: [
    {
      id: 'sec-1',
      title: null,
      fields: [
        { id: 'f1', kind: 'text', label: 'username', value: 'octocat' },
        { id: 'f2', kind: 'concealed', label: 'password', value: 'secret' },
      ],
    },
  ],
  notes: null,
  attachments: [],
  history: [],
  hasTotp: false,
  relatedItemIds: ['item-b', 'item-missing'],
  relatedItems: [
    {
      id: 'item-b',
      vaultId: 'v1',
      category: 'api_credential',
      title: 'GitHub API Key',
      subtitle: 'Personal access token',
      available: true,
    },
    {
      id: 'item-missing',
      title: 'Item not available',
      available: false,
    },
  ],
};

describe('Related items: ItemDetail view mode', () => {
  beforeEach(() => {
    act(() => {
      useApp.setState({
        locked: false,
        route: defaultRoute,
        selectedItemId: 'item-a',
        editing: false,
      });
      useItemsUI.setState({
        items: [mockSummaryA, mockSummaryB, mockSummaryC],
        loading: false,
      });
      useI18n.setState({ lang: 'en' });
      useToasts.setState({ list: [] });
    });
    vi.spyOn(api, 'listVaults').mockResolvedValue([mockVault]);
    vi.spyOn(api, 'listTags').mockResolvedValue([]);
    vi.spyOn(api, 'getItem').mockResolvedValue(mockDetailWithRelated);
    vi.spyOn(api, 'removeRelatedItem').mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('hides the Related items section when empty', async () => {
    vi.spyOn(api, 'getItem').mockResolvedValue({
      ...mockDetailWithRelated,
      relatedItems: [],
      relatedItemIds: [],
    });
    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );
    await waitFor(() => expect(screen.getByTestId('crumb-item')).toBeInTheDocument());
    expect(screen.queryByTestId('related-items-card')).toBeNull();
  });

  it('renders related items card with title, subtitle, and icon', async () => {
    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );
    await waitFor(() => expect(screen.getByTestId('related-items-card')).toBeInTheDocument());
    expect(screen.getByText('Related items')).toBeInTheDocument();
    expect(screen.getByText('GitHub API Key')).toBeInTheDocument();
    expect(screen.getByText('Personal access token')).toBeInTheDocument();
  });

  it('opens available item on click and Enter key', async () => {
    const selectItemSpy = vi.spyOn(useApp.getState(), 'selectItem');
    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );
    const row = await screen.findByTestId('related-item-item-b');

    // Click opens item
    fireEvent.click(row);
    expect(selectItemSpy).toHaveBeenCalledWith('item-b');

    // Enter key opens item
    fireEvent.keyDown(row, { key: 'Enter', code: 'Enter' });
    expect(selectItemSpy).toHaveBeenCalledWith('item-b');
  });

  it('renders "Item not available" for missing item with remove link button', async () => {
    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );
    await screen.findByTestId('related-items-card');
    expect(screen.getByText('Item not available')).toBeInTheDocument();

    const removeBtn = screen.getByTestId('remove-unavailable-related-item-missing');
    fireEvent.click(removeBtn);

    expect(api.removeRelatedItem).toHaveBeenCalledWith({
      vaultId: 'v1',
      itemId: 'item-a',
      relatedId: 'item-missing',
    });
  });
});

describe('Related items: ItemEditor and Picker', () => {
  beforeEach(() => {
    act(() => {
      useApp.setState({
        locked: false,
        route: defaultRoute,
        selectedItemId: 'item-a',
        editing: true,
      });
      useI18n.setState({ lang: 'en' });
      useToasts.setState({ list: [] });
    });
    vi.spyOn(api, 'passwordStrength').mockResolvedValue(90);
    vi.spyOn(api, 'listItems').mockResolvedValue([mockSummaryA, mockSummaryB, mockSummaryC]);
    vi.spyOn(api, 'updateItem').mockResolvedValue(mockSummaryA);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders editor related items card and lists currently related items', () => {
    render(
      <TooltipProvider>
        <ItemEditor detail={mockDetailWithRelated} vaults={[mockVault]} />
      </TooltipProvider>
    );
    expect(screen.getByTestId('editor-related-card')).toBeInTheDocument();
    expect(screen.getByText('GitHub API Key')).toBeInTheDocument();
  });

  it('removes related item when clicking remove button in editor', () => {
    render(
      <TooltipProvider>
        <ItemEditor detail={mockDetailWithRelated} vaults={[mockVault]} />
      </TooltipProvider>
    );
    expect(screen.getByTestId('editor-related-item-item-b')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('remove-related-item-b'));
    expect(screen.queryByTestId('editor-related-item-item-b')).toBeNull();
  });

  it('opens picker, excludes self and already related, and picks candidate on Enter', async () => {
    render(
      <TooltipProvider>
        <ItemEditor detail={mockDetailWithRelated} vaults={[mockVault]} />
      </TooltipProvider>
    );

    // Open picker
    fireEvent.click(screen.getByTestId('add-related-item-btn'));
    await waitFor(() => expect(screen.getByTestId('related-picker-search')).toBeInTheDocument());

    // Candidate C (Deploy Server) is listed, while A (self) and B (already linked) are excluded
    await waitFor(() => {
      expect(screen.getByTestId('related-picker-item-item-c')).toBeInTheDocument();
      expect(screen.queryByTestId('related-picker-item-item-a')).toBeNull();
      expect(screen.queryByTestId('related-picker-item-item-b')).toBeNull();
    });

    // Select candidate C by clicking
    fireEvent.click(screen.getByTestId('related-picker-item-item-c'));

    // Candidate C is now in editor
    await waitFor(() => {
      expect(screen.getByTestId('editor-related-item-item-c')).toBeInTheDocument();
      expect(screen.getByText('Deploy Server')).toBeInTheDocument();
    });
  });

  it('submits relatedItemIds when saving in editor', async () => {
    render(
      <TooltipProvider>
        <ItemEditor detail={mockDetailWithRelated} vaults={[mockVault]} />
      </TooltipProvider>
    );

    // Click Save
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(api.updateItem).toHaveBeenCalledWith(
        expect.objectContaining({
          itemId: 'item-a',
          input: expect.objectContaining({
            relatedItemIds: ['item-b', 'item-missing'],
          }),
        })
      );
    });
  });
});
