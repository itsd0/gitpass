import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { cleanup, render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QuickAccessUnlock } from './QuickAccessUnlock';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import { TooltipProvider } from '../../ui';

/** Mutable stand-in for the quick-access window's visibility and focus. */
function mockWindowShown(state: { visibility: DocumentVisibilityState; focused: boolean }) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state.visibility,
  });
  vi.spyOn(document, 'hasFocus').mockImplementation(() => state.focused);
}

const bioSettings = {
  autoLockMinutes: 5,
  clipboardClearSecs: 90,
  theme: 'dark' as const,
  language: 'en' as const,
  biometricUnlock: true,
  quickAccessHotkey: 'CommandOrControl+Shift+Space',
  hibpCheck: false,
  sshApproveEachUse: false,
  lockOnSleep: true,
  lockOnScreenLock: true,
  lockOnUserSwitch: true,
};

function mockLockedWithBiometric() {
  vi.spyOn(api, 'status').mockResolvedValue({
    unlocked: false,
    vaultPath: '~/.gitpass-test',
    account: null,
    biometricAvailable: true,
  });
  vi.spyOn(api, 'getSettings').mockResolvedValue(bioSettings);
  return vi.spyOn(api, 'unlockBiometric').mockResolvedValue({
    unlocked: true,
    vaultPath: '~/.gitpass-test',
    account: 'TouchID User',
  });
}

describe('QuickAccessUnlock', () => {
  beforeEach(() => {
    useApp.setState({ locked: true, account: null });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.useRealTimers();
    Reflect.deleteProperty(document, 'visibilityState');
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

  it('prompts Touch ID when the quick access window is already shown', async () => {
    const shown = { visibility: 'visible' as const, focused: true };
    mockWindowShown(shown);
    const bioSpy = mockLockedWithBiometric();
    const onDone = vi.fn();
    render(
      <TooltipProvider>
        <QuickAccessUnlock onDone={onDone} />
      </TooltipProvider>
    );

    await waitFor(() => {
      expect(bioSpy).toHaveBeenCalledTimes(1);
      expect(bioSpy).toHaveBeenCalledWith({ vaultPath: '~/.gitpass-test' });
      expect(useApp.getState().locked).toBe(false);
      expect(useApp.getState().account).toBe('TouchID User');
      expect(onDone).toHaveBeenCalled();
    });
    const input = screen.getByLabelText(/Master password/i);
    expect(document.activeElement).toBe(input);
  });

  it('does not call unlockBiometric when the window is hidden or unfocused', async () => {
    const shown = { visibility: 'hidden' as DocumentVisibilityState, focused: true };
    mockWindowShown(shown);
    const bioSpy = mockLockedWithBiometric();
    render(
      <TooltipProvider>
        <QuickAccessUnlock />
      </TooltipProvider>
    );

    expect(await screen.findByText('~/.gitpass-test')).toBeTruthy();
    await act(async () => { window.dispatchEvent(new Event('focus')); });
    expect(bioSpy).not.toHaveBeenCalled();
    expect(document.activeElement).not.toBe(screen.getByLabelText(/Master password/i));

    shown.visibility = 'visible';
    shown.focused = false;
    await act(async () => {
      document.dispatchEvent(new Event('visibilitychange'));
      window.dispatchEvent(new Event('focus'));
    });
    expect(bioSpy).not.toHaveBeenCalled();
    expect(document.activeElement).not.toBe(screen.getByLabelText(/Master password/i));
  });

  it('calls unlockBiometric exactly once when the window gains focus', async () => {
    const shown = { visibility: 'visible' as const, focused: false };
    mockWindowShown(shown);
    const bioSpy = mockLockedWithBiometric();
    render(
      <TooltipProvider>
        <QuickAccessUnlock />
      </TooltipProvider>
    );

    expect(await screen.findByText('~/.gitpass-test')).toBeTruthy();
    expect(bioSpy).not.toHaveBeenCalled();

    shown.focused = true;
    await act(async () => { window.dispatchEvent(new Event('focus')); });
    await waitFor(() => {
      expect(bioSpy).toHaveBeenCalledTimes(1);
      expect(bioSpy).toHaveBeenCalledWith({ vaultPath: '~/.gitpass-test' });
    });

    await act(async () => { window.dispatchEvent(new Event('focus')); });
    expect(bioSpy).toHaveBeenCalledTimes(1);
  });

  it('does not prompt again when the system sheet returns focus, but does on the next show', async () => {
    const shown = { visibility: 'visible' as const, focused: false };
    mockWindowShown(shown);
    let rejectBio: (err: Error) => void = () => {};
    vi.spyOn(api, 'status').mockResolvedValue({
      unlocked: false,
      vaultPath: '~/.gitpass-test',
      account: null,
      biometricAvailable: true,
    });
    vi.spyOn(api, 'getSettings').mockResolvedValue(bioSettings);
    const bioSpy = vi.spyOn(api, 'unlockBiometric').mockImplementation(
      () => new Promise((_, reject) => { rejectBio = reject; }),
    );

    render(
      <TooltipProvider>
        <QuickAccessUnlock />
      </TooltipProvider>
    );
    expect(await screen.findByText('~/.gitpass-test')).toBeTruthy();

    shown.focused = true;
    await act(async () => { window.dispatchEvent(new Event('focus')); });
    await waitFor(() => expect(bioSpy).toHaveBeenCalledTimes(1));

    // Sheet shows: the window blurs while the prompt is still running.
    shown.focused = false;
    await act(async () => { window.dispatchEvent(new Event('blur')); });
    await act(async () => { rejectBio(new Error('cancelled')); });

    // Sheet dismissed: focus returns. Must not start a second prompt.
    shown.focused = true;
    await act(async () => { window.dispatchEvent(new Event('focus')); });
    expect(bioSpy).toHaveBeenCalledTimes(1);

    // User hides Quick Access, then opens it again.
    shown.focused = false;
    await act(async () => { window.dispatchEvent(new Event('blur')); });
    shown.focused = true;
    await act(async () => { window.dispatchEvent(new Event('focus')); });
    await waitFor(() => expect(bioSpy).toHaveBeenCalledTimes(2));
  });

  it('still starts Touch ID from the button when the window has not been shown', async () => {
    const shown = { visibility: 'hidden' as const, focused: false };
    mockWindowShown(shown);
    const bioSpy = mockLockedWithBiometric();
    render(
      <TooltipProvider>
        <QuickAccessUnlock />
      </TooltipProvider>
    );
    expect(await screen.findByText('~/.gitpass-test')).toBeTruthy();
    expect(bioSpy).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: /Unlock with Touch ID/i }));
    await waitFor(() => expect(bioSpy).toHaveBeenCalledTimes(1));
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
  it("disables unlock button and displays live countdown when throttled", async () => {
    vi.useFakeTimers();
    vi.spyOn(api, "status").mockResolvedValue({
      unlocked: false,
      vaultPath: "~/.gitpass",
      account: null,
      vaultExists: true,
      throttledSecs: 3,
    });
    vi.spyOn(api, "getSettings").mockResolvedValue({
      autoLockMinutes: 5,
      clipboardClearSecs: 90,
      theme: "dark",
      language: "en",
      biometricUnlock: false,
      quickAccessHotkey: "CommandOrControl+Shift+Space",
      hibpCheck: false,
      sshApproveEachUse: false,
      lockOnSleep: true,
      lockOnScreenLock: true,
      lockOnUserSwitch: true,
    });

    await act(async () => {
      render(
        <TooltipProvider>
          <QuickAccessUnlock />
        </TooltipProvider>
      );
    });

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe("Too many attempts. Try again in 3 s");
    const submitBtn = screen.getByRole("button", { name: /Unlock/i });
    expect((submitBtn as HTMLButtonElement).disabled).toBe(true);

    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByRole("alert").textContent).toBe("Too many attempts. Try again in 2 s");

    vi.useRealTimers();
  });
});
