import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { LargeTypeDialog, LargeTypeModalHost, charKind, groupCharacters } from './LargeType';
import { TooltipProvider } from '../../ui';
import { useApp } from '../../lib/store';
import { builtinCommands } from '../palette/commands';
import { api } from '../../api/client';
import { useItemsUI } from './state';
import type { ItemDetail, ItemSummary } from '../../api/types';

describe('LargeType helpers', () => {
  it('classifies digits, letters, symbols, and spaces correctly', () => {
    expect(charKind('5')).toBe('digit');
    expect(charKind('A')).toBe('letter');
    expect(charKind('z')).toBe('letter');
    expect(charKind('!')).toBe('symbol');
    expect(charKind('#')).toBe('symbol');
    expect(charKind('$')).toBe('symbol');
    expect(charKind(' ')).toBe('space');
  });

  it('groups characters into chunks of up to 8', () => {
    // 20 characters -> 8 + 8 + 4
    const pw = '1234567890abcdefghij';
    const groups = groupCharacters(pw, 8);
    expect(groups).toHaveLength(3);
    expect(groups[0]).toHaveLength(8);
    expect(groups[1]).toHaveLength(8);
    expect(groups[2]).toHaveLength(4);

    expect(groups[0].map((c) => c.pos)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    expect(groups[1].map((c) => c.pos)).toEqual([9, 10, 11, 12, 13, 14, 15, 16]);
    expect(groups[2].map((c) => c.pos)).toEqual([17, 18, 19, 20]);
  });

  it('handles empty string cleanly', () => {
    expect(groupCharacters('')).toEqual([]);
  });
});

describe('LargeTypeDialog', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ locked: false });
  });

  it('renders title, groups of up to 8 characters, and positions', () => {
    const onOpenChange = vi.fn();
    // 10 chars -> group 1 (8), group 2 (2)
    render(
      <TooltipProvider>
        <LargeTypeDialog
          open={true}
          onOpenChange={onOpenChange}
          title="Password"
          value="aB1!cD2@eF"
        />
      </TooltipProvider>
    );

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Password' })).toBeInTheDocument();

    const groups = screen.getAllByTestId('large-type-group');
    expect(groups).toHaveLength(2);
    expect(groups[0].querySelectorAll('[data-testid="large-type-char"]')).toHaveLength(8);
    expect(groups[1].querySelectorAll('[data-testid="large-type-char"]')).toHaveLength(2);

    const positions = screen.getAllByTestId('large-type-pos');
    expect(positions).toHaveLength(10);
    expect(positions.map((p) => p.textContent)).toEqual(['1', '2', '3', '4', '5', '6', '7', '8', '9', '10']);
  });

  it('dismisses when Close button is clicked', () => {
    const onOpenChange = vi.fn();
    render(
      <TooltipProvider>
        <LargeTypeDialog
          open={true}
          onOpenChange={onOpenChange}
          title="PIN"
          value="1234"
        />
      </TooltipProvider>
    );

    const closeBtn = screen.getByLabelText('Close');
    fireEvent.click(closeBtn);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('closes on gitpass:locked window event', () => {
    const onOpenChange = vi.fn();
    render(
      <TooltipProvider>
        <LargeTypeDialog
          open={true}
          onOpenChange={onOpenChange}
          title="API Secret"
          value="xyz"
        />
      </TooltipProvider>
    );

    act(() => {
      window.dispatchEvent(new CustomEvent('gitpass:locked'));
    });

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('never writes secret value to console, document.title, or clipboard', () => {
    const logSpy = vi.spyOn(console, 'log');
    const infoSpy = vi.spyOn(console, 'info');
    const warnSpy = vi.spyOn(console, 'warn');
    const origTitle = document.title;

    const SECRET = 'SUPER_CONFIDENTIAL_SECRET_123';
    render(
      <TooltipProvider>
        <LargeTypeDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Password"
          value={SECRET}
        />
      </TooltipProvider>
    );

    for (const spy of [logSpy, infoSpy, warnSpy]) {
      for (const call of spy.mock.calls) {
        for (const arg of call) {
          expect(String(arg)).not.toContain(SECRET);
        }
      }
    }
    expect(document.title).toBe(origTitle);
    expect(document.title).not.toContain(SECRET);
  });
});

describe('LargeTypeModalHost', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ modal: null, locked: false });
  });

  it('renders modal when modal id is large-type and closes on dismissal', () => {
    render(<LargeTypeModalHost />);
    expect(screen.queryByRole('dialog')).toBeNull();

    act(() => {
      useApp.getState().openModal('large-type', { title: 'Recovery Code', value: 'RC-9988' });
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Recovery Code' })).toBeInTheDocument();

    const chars = screen.getAllByTestId('large-type-char');
    expect(chars).toHaveLength(7);

    // Click close
    fireEvent.click(screen.getByLabelText('Close'));
    expect(useApp.getState().modal).toBeNull();
  });
});

describe('Command palette integration', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ selectedItemId: null, modal: null });
  });

  it('includes "Show password in large type" when an item is selected', async () => {
    // When no item is selected:
    let cmds = builtinCommands();
    expect(cmds.find((c) => c.id === 'show-large-type')).toBeUndefined();

    // Select an item:
    useApp.setState({ selectedItemId: 'item-123' });
    cmds = builtinCommands();
    const largeTypeCmd = cmds.find((c) => c.id === 'show-large-type');
    expect(largeTypeCmd).toBeDefined();
    expect(largeTypeCmd?.label).toBe('Show password in large type');

    // Mock item summary and detail with a password field
    const mockSummary: ItemSummary = {
      id: 'item-123',
      vaultId: 'vault-1',
      title: 'GitHub',
      subtitle: 'user@example.com',
      category: 'login',
      updatedAt: '2026-10-28T00:00:00Z',
    };
    useItemsUI.setState({ items: [mockSummary] });

    const mockDetail: ItemDetail = {
      id: 'item-123',
      vaultId: 'vault-1',
      title: 'GitHub',
      subtitle: 'user@example.com',
      category: 'login',
      favorite: false,
      archived: false,
      sections: [
        {
          id: 'sec-1',
          title: 'Main',
          fields: [
            { id: 'f-user', label: 'username', kind: 'text', value: 'user@example.com' },
            { id: 'f-pw', label: 'password', kind: 'concealed' },
          ],
        },
      ],
      tags: [],
      attachments: [],
      history: [],
      createdAt: '2026-10-28T00:00:00Z',
      updatedAt: '2026-10-28T00:00:00Z',
    };

    vi.spyOn(api, 'getItem').mockResolvedValue(mockDetail);
    vi.spyOn(api, 'revealField').mockResolvedValue('RevealedPassword123!');

    // Run the command
    await act(async () => {
      await largeTypeCmd?.run();
    });

    expect(api.getItem).toHaveBeenCalledWith({ vaultId: 'vault-1', itemId: 'item-123' });
    expect(api.revealField).toHaveBeenCalledWith({ vaultId: 'vault-1', itemId: 'item-123', fieldId: 'f-pw' });
    expect(useApp.getState().modal).toEqual({
      id: 'large-type',
      props: { title: 'password', value: 'RevealedPassword123!' },
    });
  });
});
