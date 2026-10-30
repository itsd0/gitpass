import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { translate } from '../../i18n';
import { useApp } from '../../lib/store';
import { allCommands, modalFor } from '../registry';
import { SettingsPage } from '../settings/SettingsPage';
import { searchSettings } from '../settings/searchIndex';
import { useSettings } from '../settings/useSettings';
import type { EmergencyKitData } from '../../api/types';

const base: EmergencyKitData = { accountId: 'acc-1', vaultPath: '/tmp/v', remote: null, deviceName: null, createdAt: 'Mon 2026-10-26 10:00 AM (UTC+07:00)', secretKeyAvailable: true, secretKey: null };
const en = (k: any) => translate('en', k);

beforeEach(() => {
  useApp.setState({ modal: null });
  vi.spyOn(api, 'getSettings').mockResolvedValue({ autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'light', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); useApp.setState({ modal: null }); useSettings.setState({ settings: null }); });

describe('View / Save Emergency Kit entry points (no key rotation)', () => {
  it('is a palette command and findable in settings search', () => {
    expect(allCommands().some((c) => c.id === 'view-emergency-kit')).toBe(true);
    allCommands().find((c) => c.id === 'view-emergency-kit')!.run();
    expect(useApp.getState().modal?.id).toBe('emergency-kit');
    expect(searchSettings('emergency kit', en).some((r) => r.id === 'setting-view-kit')).toBe(true);
  });
  it('Settings → Security opens the kit; the key shows only after the master password and nothing rotates', async () => {
    const rotate = vi.spyOn(api, 'rotateSecretKey');
    vi.spyOn(api, 'emergencyKitData').mockImplementation(async (a: { password?: string } = {}) => (a.password === 'right' ? { ...base, secretKey: 'A3-SHOWN' } : base));
    const patchSpy = vi.spyOn(api, 'setOnboarding').mockResolvedValue({ extensionInstalled: false, emergencyKitSaved: true, repoConnected: true, imported: false, dismissed: false });
    vi.spyOn(api, 'onboarding').mockResolvedValue({ extensionInstalled: false, emergencyKitSaved: true, repoConnected: true, imported: false, dismissed: false });
    const u = userEvent.setup();
    function Host() { const M = useApp((s) => (s.modal ? modalFor(s.modal.id) : undefined)); return <><SettingsPage initialTab="security" />{M && <M />}</>; }
    render(<Host />);
    await u.click(await screen.findByRole('button', { name: en('settings.viewKit') }));
    await screen.findByTestId('emergency-kit-sheet');
    expect(screen.queryByText('A3-SHOWN')).toBeNull();
    await u.click(screen.getByText(en('kit.reveal')));
    await u.type(screen.getByLabelText(en('kit.masterPassword'), { selector: 'input' }), 'right');
    await u.click(screen.getAllByText(en('kit.reveal'))[0]);
    await screen.findByText('A3-SHOWN');
    fireEvent.click(screen.getByText(en('kit.markSaved')));
    await waitFor(() => expect(patchSpy).toHaveBeenCalledWith({ patch: { emergencyKitSaved: true } }));
    expect(rotate).not.toHaveBeenCalled();
    await waitFor(() => expect(useApp.getState().modal).toBeNull());
  });
});
