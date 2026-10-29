import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { cleanup, render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QuickAccessUnlock } from './QuickAccessUnlock';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import { TooltipProvider } from '../../ui';

describe('QuickAccessUnlock', () => {
  beforeEach(() => {
    useApp.setState({ locked: true, account: null });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders unlock state and unlocks with master password', async () => {
    vi.spyOn(api, 'status').mockResolvedValue({
      unlocked: false,
      vaultPath: '~/.gitpass-test',
      account: null,
      biometricAvailable: false,
    });
    vi.spyOn(api, 'getSettings').mockResolvedValue({
      autoLockMinutes: 5,
      clipboardClearSecs: 90,
      theme: 'dark',
      language: 'en',
      biometricUnlock: false,
      quickAccessHotkey: 'CommandOrControl+Shift+Space',
      hibpCheck: false,
      sshApproveEachUse: false,
      lockOnSleep: true,
      lockOnScreenLock: true,
      lockOnUserSwitch: true,
    });
    const unlockSpy = vi.spyOn(api, 'unlock').mockResolvedValue({
      unlocked: true,
      vaultPath: '~/.gitpass-test',
      account: 'Test User',
    });

    const onDone = vi.fn();
    render(
      <TooltipProvider>
        <QuickAccessUnlock onDone={onDone} />
      </TooltipProvider>
    );

    expect(await screen.findByTestId('quick-access-unlock')).toBeTruthy();
    expect(screen.getByText('Unlock gitpass')).toBeTruthy();

    const input = screen.getByLabelText(/Master password/i);
    fireEvent.change(input, { target: { value: 'master-pass-123' } });

    const submitBtn = screen.getByRole('button', { name: /Unlock/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(unlockSpy).toHaveBeenCalledWith({
        vaultPath: '~/.gitpass-test',
        password: 'master-pass-123',
        secretKey: null,
      });
      expect(useApp.getState().locked).toBe(false);
      expect(useApp.getState().account).toBe('Test User');
      expect(onDone).toHaveBeenCalled();
    });
  });

  it('prompts Touch ID automatically on mount when biometric unlock is enabled', async () => {
    vi.spyOn(api, 'status').mockResolvedValue({
      unlocked: false,
      vaultPath: '~/.gitpass-test',
      account: null,
      biometricAvailable: true,
    });
    vi.spyOn(api, 'getSettings').mockResolvedValue({
      autoLockMinutes: 5,
      clipboardClearSecs: 90,
      theme: 'dark',
      language: 'en',
      biometricUnlock: true,
      quickAccessHotkey: 'CommandOrControl+Shift+Space',
      hibpCheck: false,
      sshApproveEachUse: false,
      lockOnSleep: true,
      lockOnScreenLock: true,
      lockOnUserSwitch: true,
    });
    const bioSpy = vi.spyOn(api, 'unlockBiometric').mockResolvedValue({
      unlocked: true,
      vaultPath: '~/.gitpass-test',
      account: 'TouchID User',
    });

    const onDone = vi.fn();
    render(
      <TooltipProvider>
        <QuickAccessUnlock onDone={onDone} />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(bioSpy).toHaveBeenCalledWith({ vaultPath: '~/.gitpass-test' });
      expect(useApp.getState().locked).toBe(false);
      expect(useApp.getState().account).toBe('TouchID User');
      expect(onDone).toHaveBeenCalled();
    });
  });

  it('shows error message when unlock fails', async () => {
    vi.spyOn(api, 'status').mockResolvedValue({
      unlocked: false,
      vaultPath: '~/.gitpass-test',
      account: null,
      biometricAvailable: false,
    });
    vi.spyOn(api, 'getSettings').mockResolvedValue({
      autoLockMinutes: 5,
      clipboardClearSecs: 90,
      theme: 'dark',
      language: 'en',
      biometricUnlock: false,
      quickAccessHotkey: 'CommandOrControl+Shift+Space',
      hibpCheck: false,
      sshApproveEachUse: false,
      lockOnSleep: true,
      lockOnScreenLock: true,
      lockOnUserSwitch: true,
    });
    vi.spyOn(api, 'unlock').mockRejectedValue(new Error('Wrong master password'));

    render(
      <TooltipProvider>
        <QuickAccessUnlock />
      </TooltipProvider>
    );

    const input = await screen.findByLabelText(/Master password/i);
    fireEvent.change(input, { target: { value: 'bad-pass' } });

    const submitBtn = screen.getByRole('button', { name: /Unlock/i });
    fireEvent.click(submitBtn);

    const alert = await screen.findByRole('alert');
    expect(alert).toBeTruthy();
    expect(alert.textContent).toBe('Wrong master password');
    expect(useApp.getState().locked).toBe(true);
  });
});
