import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { cleanup, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { AcceptShare } from './PeoplePage';

const openMock = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: openMock }));

describe('AcceptShare pick', () => {
  beforeEach(() => {
    openMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    delete (window as any).__TAURI_INTERNALS__;
  });

  it('fills the path from the native picker, filtered to .gitpass-share files', async () => {
    (window as any).__TAURI_INTERNALS__ = {};
    openMock.mockResolvedValue('/home/user/Downloads/vault.gitpass-share');
    render(<AcceptShare />);

    fireEvent.click(screen.getByRole('button', { name: /Choose file/i }));

    await waitFor(() => expect(openMock).toHaveBeenCalledWith({
      multiple: false,
      filters: [{ name: 'gitpass share', extensions: ['gitpass-share'] }],
    }));
    expect((screen.getByLabelText(/Share file path/i) as HTMLInputElement).value)
      .toBe('/home/user/Downloads/vault.gitpass-share');
  });

  it('falls back to focusing the path input outside Tauri', async () => {
    render(<AcceptShare />);

    fireEvent.click(screen.getByRole('button', { name: /Choose file/i }));

    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText(/Share file path/i)));
    expect(openMock).not.toHaveBeenCalled();
  });
});
