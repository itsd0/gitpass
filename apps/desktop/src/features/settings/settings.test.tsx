import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { useI18n } from '../../i18n';
import { useApp } from '../../lib/store';
import { SettingsPage } from './SettingsPage';
import { useSettings } from './useSettings';
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useSettings.setState({ settings: null });
  useI18n.getState().setLang('en');
  useApp.setState({ route: { name: 'settings', tab: 'general' } });
});
const defaults = { autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'dark' as const, language: 'en' as const, biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false };
beforeEach(() => { vi.spyOn(api, 'getSettings').mockResolvedValue({ ...defaults }); });
describe('settings', () => {
  it('saves theme via api.setSettings with full settings object and applies it', async () => {
    const spy = vi.spyOn(api, 'setSettings'); const u = userEvent.setup();
    render(<SettingsPage initialTab="general" />);
    await u.click(await screen.findByRole('radio', { name: 'Light' }));
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0]![0].settings).toMatchObject({ theme: 'light', autoLockMinutes: 10, language: 'en' });
    await waitFor(() => expect(document.documentElement.classList.contains('dark')).toBe(false));
  });
  it('switching language re-renders UI in Thai', async () => {
    const u = userEvent.setup(); render(<SettingsPage initialTab="general" />);
    await u.click(await screen.findByRole('radio', { name: 'ไทย' }));
    expect(await screen.findByRole('heading', { name: 'การตั้งค่า' })).toBeTruthy();
  });
  it('shows "not supported yet" when backend rejects biometric unlock', async () => {
    vi.spyOn(api, 'setSettings').mockRejectedValue(new Error('unsupported')); const u = userEvent.setup();
    render(<SettingsPage initialTab="security" />);
    const sw = await screen.findByRole('switch', { name: /biometric/i });
    expect(sw).toBeEnabled();
    await u.click(sw);
    expect(await screen.findByText(/not supported yet/)).toBeTruthy();
  });
  it('website icons toggle is off by default and saved when enabled', async () => {
    const spy = vi.spyOn(api, 'setSettings').mockResolvedValue(undefined as any);
    const u = userEvent.setup();
    render(<SettingsPage initialTab="general" />);
    const sw = await screen.findByTestId('show-website-icons-switch');
    expect(sw).toHaveAttribute('aria-checked', 'false');
    expect(sw).toBeEnabled();
    await u.click(sw);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.showWebsiteIcons).toBe(true);
  });
  it('HIBP check is off by default and saved when enabled', async () => {
    const spy = vi.spyOn(api, 'setSettings').mockResolvedValue(undefined as any); const u = userEvent.setup();
    render(<SettingsPage initialTab="security" />);
    const sw = await screen.findByTestId('hibp-switch');
    expect(sw).toHaveAttribute('aria-checked', 'false');
    expect(sw).toBeEnabled();
    await u.click(sw);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.hibpCheck).toBe(true);
  });
  it('saves auto-lock minutes on Enter', async () => {
    const spy = vi.spyOn(api, 'setSettings'); const u = userEvent.setup(); render(<SettingsPage initialTab="security" />);
    const input = await screen.findByLabelText('Auto-lock after (minutes)');
    expect(input).toBeEnabled();
    await u.clear(input);
    await u.type(input, '25{Enter}');
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.autoLockMinutes).toBe(25);
  });
  it('SSH Agent tab shows IdentityAgent line and toggles approve-each-use', async () => {
    vi.spyOn(api, 'sshAgentInfo').mockResolvedValue({ socketPath: '/x/ssh-agent.sock', configLine: 'Host *\n  IdentityAgent "/x/ssh-agent.sock"', running: true, approveEachUse: false });
    const spy = vi.spyOn(api, 'setSettings').mockImplementation(async (a) => a.settings);
    render(<SettingsPage initialTab="ssh" />);
    expect((await screen.findByTestId('ssh-config-line')).textContent).toContain('IdentityAgent "/x/ssh-agent.sock"');
    expect(screen.getByText('Running')).toBeTruthy();
    const toggle = await screen.findByRole('switch', { name: 'Approve each use' });
    expect(toggle).toBeEnabled();
    fireEvent.click(toggle);
    await waitFor(() => expect(spy).toHaveBeenCalled(), { timeout: 3000 });
    expect(spy.mock.calls[0]![0].settings).toMatchObject({ sshApproveEachUse: true, hibpCheck: false });
  });
  it('re-rendering the settings page keeps the active section mounted', async () => {
    const view = render(<SettingsPage initialTab="ssh" />);
    const sw = await screen.findByRole('switch', { name: 'Approve each use' });
    view.rerender(<SettingsPage initialTab="ssh" />);
    expect(screen.getByRole('switch', { name: 'Approve each use' })).toBe(sw);
  });
  it('toggles lock on Mac sleep, screen lock, and user switch', async () => {
    const spy = vi.spyOn(api, 'setSettings').mockResolvedValue(undefined as any);
    const u = userEvent.setup();
    render(<SettingsPage initialTab="security" />);

    const sleepSwitch = await screen.findByTestId('lock-sleep-switch');
    expect(sleepSwitch).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Lock when Mac sleeps')).toBeTruthy();
    expect(screen.getByText('Lock the vault immediately when your Mac goes to sleep')).toBeTruthy();
    await u.click(sleepSwitch);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.lockOnSleep).toBe(false);

    const screenSwitch = await screen.findByTestId('lock-screen-switch');
    expect(screenSwitch).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Lock when screen locks')).toBeTruthy();
    await u.click(screenSwitch);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.lockOnScreenLock).toBe(false);

    const switchUserSwitch = await screen.findByTestId('lock-user-switch-switch');
    expect(switchUserSwitch).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Lock when switching users')).toBeTruthy();
    await u.click(switchUserSwitch);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.lockOnUserSwitch).toBe(false);
  });
  it('toggles hide from screen recording', async () => {
    const spy = vi.spyOn(api, 'setSettings').mockResolvedValue(undefined as any);
    const u = userEvent.setup();
    render(<SettingsPage initialTab="security" />);

    const hideSwitch = await screen.findByTestId('hide-screen-recording-switch');
    expect(hideSwitch).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Hide gitpass from screen recording and sharing')).toBeTruthy();
    expect(screen.getByText('Screenshots and screen shares show a blank window while this is on; turn it off for demos')).toBeTruthy();
    await u.click(hideSwitch);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.hideFromScreenRecording).toBe(false);
  });
  it('toggles show setup guide switch in General tab', async () => {
    const onb = { extensionInstalled: false, emergencyKitSaved: false, repoConnected: true, imported: false, dismissed: false };
    vi.spyOn(api, 'onboarding').mockResolvedValue(onb);
    const patchSpy = vi.spyOn(api, 'setOnboarding').mockImplementation(async ({ patch }) => ({ ...onb, ...patch }));
    const u = userEvent.setup();
    render(<SettingsPage initialTab="general" />);

    const setupSwitch = await screen.findByTestId('setup-guide-switch');
    expect(setupSwitch).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Show setup guide')).toBeTruthy();
    expect(screen.getByText('Display the setup checklist on the Home page')).toBeTruthy();

    await u.click(setupSwitch);
    await waitFor(() => expect(patchSpy).toHaveBeenCalledWith({ patch: { dismissed: true } }));
  });
});
