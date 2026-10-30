import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

import { api } from '../../api/client';
import { TooltipProvider } from '../../ui';
import { PasswordHistory } from './PasswordHistory';
import { FieldRow } from './FieldView';

describe('PasswordHistory UI', () => {
  const ctx = { vaultId: 'v1', itemId: 'item-1' };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it('renders nothing when history is undefined or empty', () => {
    const { container: c1 } = render(
      <TooltipProvider>
        <PasswordHistory ctx={ctx} history={undefined} />
      </TooltipProvider>
    );
    expect(c1.firstChild).toBeNull();

    const { container: c2 } = render(
      <TooltipProvider>
        <PasswordHistory ctx={ctx} history={[]} />
      </TooltipProvider>
    );
    expect(c2.firstChild).toBeNull();
  });

  it('renders collapsed disclosure trigger with count', () => {
    const history = [
      { replacedAt: '2026-10-30T10:00:00Z' },
      { replacedAt: '2026-10-25T08:00:00Z' },
    ];
    render(
      <TooltipProvider>
        <PasswordHistory ctx={ctx} history={history} />
      </TooltipProvider>
    );

    const toggle = screen.getByTestId('password-history-toggle');
    expect(toggle).toBeDefined();
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(toggle.textContent).toContain('Password history (2)');

    // Rows should not be in the document while collapsed
    expect(screen.queryByTestId('password-history-row-0')).toBeNull();
  });

  it('expands rows with masked values on click', () => {
    const history = [{ replacedAt: '2026-10-30T10:00:00Z' }];
    render(
      <TooltipProvider>
        <PasswordHistory ctx={ctx} history={history} />
      </TooltipProvider>
    );

    const toggle = screen.getByTestId('password-history-toggle');
    fireEvent.click(toggle);

    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByTestId('password-history-row-0')).toBeDefined();
    expect(screen.getByTestId('history-value-0').textContent).toBe('••••••••••');
  });

  it('reveals historical password on reveal click', async () => {
    const history = [{ replacedAt: '2026-10-30T10:00:00Z' }];
    vi.spyOn(api, 'revealField').mockResolvedValue('old-secret-pass');

    render(
      <TooltipProvider>
        <PasswordHistory ctx={ctx} history={history} />
      </TooltipProvider>
    );

    fireEvent.click(screen.getByTestId('password-history-toggle'));
    const revealBtn = screen.getByTestId('history-reveal-0');
    fireEvent.click(revealBtn);

    await waitFor(() => {
      expect(api.revealField).toHaveBeenCalledWith({
        vaultId: 'v1',
        itemId: 'item-1',
        fieldId: 'password_history:0',
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('history-value-0').textContent).toContain('old-secret-pass');
    });

    // Clicking again conceals
    fireEvent.click(revealBtn);
    expect(screen.getByTestId('history-value-0').textContent).toBe('••••••••••');
  });

  it('copies historical password on copy click', async () => {
    const history = [{ replacedAt: '2026-10-30T10:00:00Z' }];
    vi.spyOn(api, 'copyField').mockResolvedValue();

    render(
      <TooltipProvider>
        <PasswordHistory ctx={ctx} history={history} />
      </TooltipProvider>
    );

    fireEvent.click(screen.getByTestId('password-history-toggle'));
    const copyBtn = screen.getByTestId('history-copy-0');
    fireEvent.click(copyBtn);

    await waitFor(() => {
      expect(api.copyField).toHaveBeenCalledWith({
        vaultId: 'v1',
        itemId: 'item-1',
        fieldId: 'password_history:0',
      });
    });
  });

  it('confirms and clears password history', async () => {
    const history = [{ replacedAt: '2026-10-30T10:00:00Z' }];
    const onCleared = vi.fn();
    vi.spyOn(api, 'clearPasswordHistory').mockResolvedValue();

    render(
      <TooltipProvider>
        <PasswordHistory ctx={ctx} history={history} onCleared={onCleared} />
      </TooltipProvider>
    );

    fireEvent.click(screen.getByTestId('password-history-toggle'));

    // Click Clear history button
    const clearBtn = screen.getByTestId('clear-password-history-btn');
    fireEvent.click(clearBtn);

    // Dialog opens
    expect(screen.getByTestId('confirm-clear-history-btn')).toBeDefined();

    // Confirm clearing
    fireEvent.click(screen.getByTestId('confirm-clear-history-btn'));

    await waitFor(() => {
      expect(api.clearPasswordHistory).toHaveBeenCalledWith({
        vaultId: 'v1',
        itemId: 'item-1',
      });
      expect(onCleared).toHaveBeenCalledTimes(1);
    });
  });

  it('integrates with FieldRow when category is login and label is Password', () => {
    const field = { id: 'password', label: 'Password', kind: 'concealed' as const, value: 'current' };
    const history = [{ replacedAt: '2026-10-30T10:00:00Z' }];

    render(
      <TooltipProvider>
        <FieldRow
          field={field}
          ctx={ctx}
          category="login"
          passwordHistory={history}
        />
      </TooltipProvider>
    );

    expect(screen.getByTestId('password-history-toggle')).toBeDefined();
  });
});
