import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { useI18n } from '../../i18n';
import { SettingsPage } from './SettingsPage';
import { useSettings } from './useSettings';
afterEach(() => { cleanup(); vi.restoreAllMocks(); useSettings.setState({ settings: null }); useI18n.getState().setLang('en'); });
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
    await u.click(await screen.findByRole('switch', { name: /biometric/i }));
    expect(await screen.findByText(/not supported yet/)).toBeTruthy();
  });
  it('HIBP check is off by default and saved when enabled', async () => {
    const spy = vi.spyOn(api, 'setSettings').mockResolvedValue(undefined as any); const u = userEvent.setup();
    render(<SettingsPage initialTab="security" />);
    const sw = await screen.findByTestId('hibp-switch'); expect(sw).toHaveAttribute('aria-checked', 'false');
    await u.click(sw);
    await waitFor(() => expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.hibpCheck).toBe(true));
  });
  it('saves auto-lock minutes on Enter', async () => {
    const spy = vi.spyOn(api, 'setSettings'); const u = userEvent.setup(); render(<SettingsPage initialTab="security" />);
    const input = await screen.findByLabelText('Auto-lock after (minutes)'); await u.clear(input); await u.type(input, '25{Enter}');
    await waitFor(() => expect(spy.mock.calls[spy.mock.calls.length - 1]![0].settings.autoLockMinutes).toBe(25));
  });
});
