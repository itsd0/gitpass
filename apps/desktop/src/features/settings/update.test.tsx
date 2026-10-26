import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { UpdateRow } from './UpdateRow';
import { checkForUpdate, getAppVersion, isNoReleaseError } from '../../lib/updater';
import pkg from '../../../package.json';
import { translate } from '../../i18n';
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
  it('treats a missing release (404 / no latest.json) as up to date', () => {
    expect(isNoReleaseError(new Error('Could not fetch a valid release JSON from the remote'))).toBe(true);
    expect(isNoReleaseError('error sending request: status 404')).toBe(true);
    expect(isNoReleaseError(new Error('signature verification failed'))).toBe(false);
  });
  it('app version falls back to package.json outside Tauri', async () => { expect(await getAppVersion()).toBe(pkg.version); });
  it('has my/th strings for "up to date"', () => {
    for (const l of ['my', 'th'] as const) expect(translate(l, 'update.none')).not.toBe(translate('en', 'update.none'));
  });
  it('shows errors', async () => {
    const u = userEvent.setup(); render(<UpdateRow check={async () => { throw new Error('boom'); }} />);
    await u.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(await screen.findByText(/boom/)).toBeTruthy();
  });
});
