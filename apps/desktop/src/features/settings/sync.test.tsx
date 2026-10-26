import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { useI18n } from '../../i18n';
import { SyncPanel, isValidRemoteUrl, remoteErrorKey } from './SyncPanel';
import { Checklist } from '../onboarding/Checklist';
import { useApp } from '../../lib/store';
afterEach(() => { cleanup(); vi.restoreAllMocks(); useI18n.getState().setLang('en'); });
const none = { remote: null, lastSync: null, ahead: 0, behind: 0, dirty: false };
const connected = { remote: 'git@github.com:me/vault.git', lastSync: '2026-10-26T10:00:00Z', ahead: 0, behind: 0, dirty: false };
const onb = { extensionInstalled: false, emergencyKitSaved: false, repoConnected: false, imported: false, dismissed: false };

describe('remote URL validation', () => {
  it('accepts https and ssh forms on any host', () => {
    for (const u of ['https://github.com/me/v.git', 'git@github.com:me/v.git', 'git@gitlab.com:g/s/v.git', 'ssh://git@host.example:2222/me/v.git', 'https://codeberg.org/me/v'])
      expect(isValidRemoteUrl(u)).toBe(true);
  });
  it('rejects tokens in URLs, http and junk', () => {
    for (const u of ['', 'github.com/me/v', 'http://github.com/me/v', 'https://ghp_x@github.com/me/v.git', '-oProxyCommand=x', 'https://github.com/', 'ext::sh'])
      expect(isValidRemoteUrl(u)).toBe(false);
  });
  it('maps backend error markers', () => {
    expect(remoteErrorKey('AUTH_FAILED: push')).toBe('sync.err.auth');
    expect(remoteErrorKey('WRONG_ACCOUNT: x')).toBe('sync.err.account');
    expect(remoteErrorKey('REMOTE_NOT_EMPTY: x')).toBe('sync.err.notEmpty');
    expect(remoteErrorKey('GIT_NOT_INSTALLED: x')).toBe('sync.err.git');
    expect(remoteErrorKey('boom')).toBeNull();
  });
});

describe('Settings → Sync connect form', () => {
  it('shows the Connect GitHub form with hint + github.com/new link when there is no remote', async () => {
    vi.spyOn(api, 'syncStatus').mockResolvedValue(none);
    const open = vi.spyOn(api, 'openNewRepoPage').mockResolvedValue(undefined);
    const u = userEvent.setup(); render(<SyncPanel />);
    expect(await screen.findByRole('form', { name: 'Connect GitHub' })).toBeTruthy();
    expect(screen.getByText(/EMPTY, PRIVATE repository/)).toBeTruthy();
    await u.click(screen.getByRole('button', { name: /Open github.com\/new/ }));
    expect(open).toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Connect' })).toBeDisabled();
  });
  it('connects, then shows remote, status, Sync and Disconnect', async () => {
    vi.spyOn(api, 'syncStatus').mockResolvedValueOnce(none).mockResolvedValue(connected);
    const conn = vi.spyOn(api, 'remoteConnect').mockResolvedValue(connected);
    vi.spyOn(api, 'setOnboarding').mockResolvedValue({ ...onb, repoConnected: true });
    const sync = vi.spyOn(api, 'sync').mockResolvedValue({ message: 'pulled and pushed to origin' });
    const u = userEvent.setup(); render(<SyncPanel />);
    await u.type(await screen.findByLabelText('Repository URL'), '  git@github.com:me/vault.git ');
    await u.click(screen.getByRole('button', { name: 'Connect' }));
    await waitFor(() => expect(conn).toHaveBeenCalledWith({ url: 'git@github.com:me/vault.git' }));
    expect(await screen.findByText('git@github.com:me/vault.git')).toBeTruthy();
    expect(screen.getByText('0 ahead')).toBeTruthy();
    await u.click(screen.getByRole('button', { name: 'Sync' }));
    await waitFor(() => expect(sync).toHaveBeenCalledWith({ merge: false }));
    expect(await screen.findByText(/pulled and pushed/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Disconnect' })).toBeTruthy();
  });
  it('shows auth guidance (SSH URL / gh auth login) on auth failure', async () => {
    vi.spyOn(api, 'syncStatus').mockResolvedValue(none);
    vi.spyOn(api, 'remoteConnect').mockRejectedValue(new Error('AUTH_FAILED: git push failed: Permission denied (publickey)'));
    const u = userEvent.setup(); render(<SyncPanel />);
    await u.type(await screen.findByLabelText('Repository URL'), 'https://github.com/me/vault.git');
    await u.click(screen.getByRole('button', { name: 'Connect' }));
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toMatch(/Authentication failed/);
    expect(alert.textContent).toMatch(/gh auth login/);
    expect(alert.textContent).toMatch(/git@github.com/);
  });
  it('shows wrong-account and not-empty errors clearly', async () => {
    vi.spyOn(api, 'syncStatus').mockResolvedValue(none);
    const c = vi.spyOn(api, 'remoteConnect').mockRejectedValueOnce(new Error('WRONG_ACCOUNT: other')).mockRejectedValueOnce(new Error('REMOTE_NOT_EMPTY: x'));
    const u = userEvent.setup(); render(<SyncPanel />);
    await u.type(await screen.findByLabelText('Repository URL'), 'git@github.com:me/vault.git');
    await u.click(screen.getByRole('button', { name: 'Connect' }));
    expect((await screen.findByRole('alert')).textContent).toMatch(/different gitpass account/);
    await u.click(screen.getByRole('button', { name: 'Connect' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/not empty/));
    expect(c).toHaveBeenCalledTimes(2);
  });
  it('disconnect asks for confirmation and returns to the form', async () => {
    vi.spyOn(api, 'syncStatus').mockResolvedValue(connected);
    const d = vi.spyOn(api, 'remoteDisconnect').mockResolvedValue(none);
    vi.spyOn(api, 'setOnboarding').mockResolvedValue(onb);
    const u = userEvent.setup(); render(<SyncPanel />);
    await u.click(await screen.findByRole('button', { name: 'Disconnect' }));
    const btns = await screen.findAllByRole('button', { name: 'Disconnect' });
    await u.click(btns[btns.length - 1]!);
    await waitFor(() => expect(d).toHaveBeenCalled());
    expect(await screen.findByRole('form', { name: 'Connect GitHub' })).toBeTruthy();
  });
});

describe('Home onboarding repo card', () => {
  it('How? shows the real steps, links to Settings → Sync, and Re-check marks it done', async () => {
    vi.spyOn(api, 'onboarding').mockResolvedValue(onb);
    const st = vi.spyOn(api, 'syncStatus').mockResolvedValue(none);
    const patch = vi.spyOn(api, 'setOnboarding').mockImplementation(async (a) => ({ ...onb, ...a.patch }));
    const u = userEvent.setup(); render(<Checklist />);
    await u.click(await screen.findByRole('button', { name: 'How?' }));
    expect(screen.getByText(/Open Settings → Sync, paste the repository URL/)).toBeTruthy();
    expect(screen.queryByText(/remote add origin/)).toBeNull();
    await u.click(screen.getByRole('button', { name: 'Re-check' }));
    expect(await screen.findByText('Still not connected.')).toBeTruthy();
    st.mockResolvedValue(connected);
    await u.click(screen.getByRole('button', { name: 'Re-check' }));
    await waitFor(() => expect(patch).toHaveBeenCalledWith({ patch: { repoConnected: true } }));
    await u.click(screen.getByRole('button', { name: 'Open Settings → Sync' }));
    expect(useApp.getState().route).toEqual({ name: 'settings', tab: 'sync' });
  });
});
