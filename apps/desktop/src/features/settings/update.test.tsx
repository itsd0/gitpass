import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { UpdateRow } from './UpdateRow';
import { checkForUpdate } from '../../lib/updater';
afterEach(cleanup);
describe('check for updates', () => {
  it('is unsupported outside Tauri', async () => { expect(await checkForUpdate()).toEqual({ kind: 'unsupported' }); });
  it('reports up to date', async () => {
    const u = userEvent.setup(); render(<UpdateRow check={async () => ({ kind: 'none' })} />);
    await u.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(await screen.findByText("You're on the latest version.")).toBeTruthy();
  });
  it('offers install when an update is available', async () => {
    const install = vi.fn(async () => {}); const u = userEvent.setup();
    render(<UpdateRow check={async () => ({ kind: 'available', version: '9.9.9', install })} />);
    await u.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(await screen.findByText('Version 9.9.9 is available.')).toBeTruthy();
    await u.click(screen.getByRole('button', { name: 'Install and restart' }));
    expect(install).toHaveBeenCalled();
  });
  it('shows errors', async () => {
    const u = userEvent.setup(); render(<UpdateRow check={async () => { throw new Error('boom'); }} />);
    await u.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(await screen.findByText(/boom/)).toBeTruthy();
  });
});
