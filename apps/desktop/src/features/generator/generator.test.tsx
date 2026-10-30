import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';

// jsdom has no layout engine, so mock virtualizer if ItemList is loaded.
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (o: { count: number; estimateSize: (i: number) => number }) => ({
    getTotalSize: () => 0,
    scrollToIndex: () => {},
    getVirtualItems: () =>
      Array.from({ length: o.count }, (_, i) => ({ index: i, start: 0, size: o.estimateSize(i), key: i })),
  }),
}));

import App from '../../App';
import { defaultRoute, useApp } from '../../lib/store';
import { api } from '../../api/client';
import { useSettings } from '../settings/useSettings';
import type { GeneratorHistoryEntry, Settings, VaultInfo } from '../../api/types';
import { PasswordGenerator } from '../new-item/PasswordGenerator';
import { TooltipProvider } from '../../ui';

const mockVault: VaultInfo = {
  id: 'vault-1',
  name: 'Personal',
  description: 'Personal vault',
  itemCount: 1,
  shared: false,
  members: [],
};

const initialSettings: Settings = {
  autoLockMinutes: 5,
  clipboardClearSecs: 30,
  theme: 'dark',
  language: 'en',
  biometricUnlock: false,
  quickAccessHotkey: 'CmdOrCtrl+Shift+Space',
  hibpCheck: false,
  generatorMode: 'random',
  generatorLength: 20,
  generatorDigits: true,
  generatorSymbols: true,
};

function renderWithTooltip(ui: React.ReactElement) {
  return render(<TooltipProvider>{ui}</TooltipProvider>);
}

describe('Standalone Password Generator', () => {
  let curSettings: Settings;

  beforeEach(() => {
    Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true });
    curSettings = { ...initialSettings };
    vi.spyOn(api, 'status').mockResolvedValue({ unlocked: true, vaultPath: '~/.gitpass (test)', account: 'Test Account' });
    vi.spyOn(api, 'listVaults').mockResolvedValue([mockVault]);
    vi.spyOn(api, 'listItems').mockResolvedValue([]);
    vi.spyOn(api, 'getSettings').mockImplementation(async () => ({ ...curSettings }));
    vi.spyOn(api, 'setSettings').mockImplementation(async ({ settings }) => {
      curSettings = { ...settings };
      return curSettings;
    });
    vi.spyOn(api, 'passwordStrength').mockResolvedValue(85);
    vi.spyOn(api, 'getGeneratorHistory').mockResolvedValue([]);

    act(() => {
      useApp.setState({
        locked: false,
        route: defaultRoute,
        modal: null,
        editing: false,
      });
      useSettings.setState({ settings: curSettings, error: null });
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('opens dialog via ⌘⇧G shortcut (mod+shift+g)', async () => {
    render(<App />);
    expect(screen.queryByRole('dialog', { name: /Password generator/i })).toBeNull();

    // Fire Cmd+Shift+G
    fireEvent.keyDown(window, { key: 'g', metaKey: true, shiftKey: true });

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Password generator/i })).toBeTruthy();
    });
  });

  it('opens dialog via command palette ("Generate password")', async () => {
    render(<App />);

    // Open palette with Cmd+K
    fireEvent.keyDown(window, { key: 'k', metaKey: true });
    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Command palette/i })).toBeTruthy();
    });

    // Search for generate
    const input = screen.getByRole('combobox', { name: /Search items or type a command/i });
    fireEvent.change(input, { target: { value: 'Generate' } });

    const option = await screen.findByText('Generate password');
    fireEvent.click(option);

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Password generator/i })).toBeTruthy();
    });
  });

  it('opens dialog from toolbar button with key icon', async () => {
    render(<App />);

    const toolbarBtns = screen.getAllByRole('button', { name: /Password Generator/i });
    const toolbarBtn = toolbarBtns.find((b) => b.classList.contains('h-6')) ?? toolbarBtns[0];
    fireEvent.click(toolbarBtn);

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Password generator/i })).toBeTruthy();
    });
  });

  it('opens dialog from sidebar item with key icon', async () => {
    render(<App />);

    const sidebar = screen.getByTestId('sidebar');
    const sidebarBtn = within(sidebar).getByRole('button', { name: /Password Generator/i });
    fireEvent.click(sidebarBtn);

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Password generator/i })).toBeTruthy();
    });
  });

  it('renders large monospace field with character colouring and strength meter', async () => {
    vi.spyOn(api, 'generatePassword').mockResolvedValue('Abc1!Xyz2@');
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getByTestId('colored-password')).toBeTruthy();
    });

    const colored = screen.getByTestId('colored-password');
    expect(colored.textContent).toBe('Abc1!Xyz2@');

    // Digits are colored with text-accent, symbols with blue
    const digit1 = Array.from(colored.querySelectorAll('span')).find((el) => el.textContent === '1');
    expect(digit1?.className).toContain('text-accent');

    const symbol1 = Array.from(colored.querySelectorAll('span')).find((el) => el.textContent === '!');
    expect(symbol1?.className).toContain('text-blue-600');

    const letterA = Array.from(colored.querySelectorAll('span')).find((el) => el.textContent === 'A');
    expect(letterA?.className).toContain('text-fg');

    // Strength meter renders
    await waitFor(() => {
      expect(screen.getByTestId('strength-meter')).toBeTruthy();
    });
  });

  it('regenerates password via Regenerate button and ⌘R keyboard shortcut', async () => {
    const genSpy = vi
      .spyOn(api, 'generatePassword')
      .mockResolvedValueOnce('first-pass-1')
      .mockResolvedValueOnce('second-pass-2')
      .mockResolvedValueOnce('third-pass-3');

    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('first-pass-1');
    });

    // Click Regenerate button
    const regenBtn = screen.getByRole('button', { name: /Regenerate/i });
    fireEvent.click(regenBtn);

    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('second-pass-2');
    });

    // Press Cmd+R inside window
    fireEvent.keyDown(window, { key: 'r', metaKey: true });

    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('third-pass-3');
    });

    expect(genSpy).toHaveBeenCalledTimes(3);
  });

  it('Copy button calls the copy API and shows toast', async () => {
    vi.spyOn(api, 'generatePassword').mockResolvedValue('super-secret-copy-pw-123');
    const copySpy = vi.spyOn(api, 'copyPassword').mockResolvedValue(undefined);

    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('super-secret-copy-pw-123');
    });

    const copyBtn = screen.getByRole('button', { name: /Copy/i });
    expect(copyBtn.className).toContain('bg-[#f97316]'); // Orange accent

    fireEvent.click(copyBtn);

    await waitFor(() => {
      expect(copySpy).toHaveBeenCalledWith({ password: 'super-secret-copy-pw-123' });
    });
  });

  it('"Save as new Login…" opens create form with password filled', async () => {
    vi.spyOn(api, 'generatePassword').mockResolvedValue('prefilled-secret-login-pw');

    render(<App />);

    // Open generator dialog
    act(() => {
      useApp.getState().openModal('password-generator');
    });

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Password generator/i })).toBeTruthy();
      expect(screen.getByTestId('colored-password').textContent).toBe('prefilled-secret-login-pw');
    });

    const saveLoginBtn = screen.getByRole('button', { name: /Save as new Login/i });
    fireEvent.click(saveLoginBtn);

    // Modal state transitions to new-item with login category and password
    await waitFor(() => {
      expect(useApp.getState().modal?.id).toBe('new-item');
      expect(useApp.getState().modal?.props?.category).toBe('login');
      expect((useApp.getState().modal?.props?.initial as any)?.password).toBe('prefilled-secret-login-pw');
    });

    // Form is rendered with password field filled and visible
    await waitFor(() => {
      expect(screen.getByRole('form', { name: /New Login/i })).toBeTruthy();
      const pwInput = screen.getByLabelText('password') as HTMLInputElement;
      expect(pwInput.value).toBe('prefilled-secret-login-pw');
      expect(pwInput.type).toBe('text'); // shown by default for prefilled
    });
  });

  it('persists generator settings (mode, length, toggles) but never stores the password', async () => {
    vi.spyOn(api, 'generatePassword').mockResolvedValue('pwd-never-persisted');
    const saveSpy = vi.spyOn(useSettings.getState(), 'save');

    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('pwd-never-persisted');
    });

    // Change length via spinbutton
    const numInput = screen.getByRole('spinbutton', { name: /Length input/i }) as HTMLInputElement;
    fireEvent.change(numInput, { target: { value: '32' } });

    // Switch mode to Memorable
    const memTab = screen.getByRole('tab', { name: /Memorable/i });
    fireEvent.click(memTab);

    // Wait for debounced persist
    await waitFor(() => {
      expect(saveSpy).toHaveBeenCalled();
    });

    // Verify settings object received settings, and never password
    const lastCall = saveSpy.mock.calls[saveSpy.mock.calls.length - 1][0];
    expect(lastCall).toHaveProperty('generatorMode');
    expect(lastCall).not.toHaveProperty('password');
    expect(lastCall).not.toHaveProperty('pw');
    expect(lastCall).not.toHaveProperty('generated');
    expect(curSettings).not.toHaveProperty('password');
  });

  it('supports PIN mode with numeric digits only and synced inputs', async () => {
    const genSpy = vi.spyOn(api, 'generatePassword').mockResolvedValue('748291');
    renderWithTooltip(<PasswordGenerator standalone />);

    // Click PIN tab
    const pinTab = screen.getByRole('tab', { name: /PIN/i });
    fireEvent.click(pinTab);

    await waitFor(() => {
      expect(genSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          pin: true,
          words: false,
          symbols: false,
          digits: true,
        })
      );
    });

    // Slider and number input are in sync
    const numInput = screen.getByRole('spinbutton', { name: /Digits input/i }) as HTMLInputElement;
    const slider = screen.getByRole('slider', { name: 'Digits' }) as HTMLInputElement;

    fireEvent.change(slider, { target: { value: '8' } });
    expect(numInput.value).toBe('8');

    fireEvent.change(numInput, { target: { value: '4' } });
    expect(slider.value).toBe('4');
  });

  it('shows separator and capitalize controls only in Memorable mode', async () => {
    vi.spyOn(api, 'generatePassword').mockResolvedValue('apple-river-stone');
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('apple-river-stone');
    });

    // Random mode: no words-mode controls
    expect(screen.queryByRole('radiogroup', { name: /Separator/i })).toBeNull();
    expect(screen.queryByRole('checkbox', { name: /Capitalize words/i })).toBeNull();

    // Memorable mode: both controls appear
    fireEvent.click(screen.getByRole('tab', { name: /Memorable/i }));
    await waitFor(() => {
      expect(screen.getByRole('radiogroup', { name: /Separator/i })).toBeTruthy();
    });
    expect(screen.getByRole('checkbox', { name: /Capitalize words/i })).toBeTruthy();
    expect(screen.getByRole('radio', { name: '-' })).toHaveAttribute('aria-checked', 'true');

    // PIN mode: controls disappear again
    fireEvent.click(screen.getByRole('tab', { name: /PIN/i }));
    await waitFor(() => {
      expect(screen.queryByRole('radiogroup', { name: /Separator/i })).toBeNull();
    });
    expect(screen.queryByRole('checkbox', { name: /Capitalize words/i })).toBeNull();
  });

  it('regenerates with the chosen separator and capitalize, and persists both', async () => {
    const genSpy = vi.spyOn(api, 'generatePassword').mockResolvedValue('apple-river-stone');
    const saveSpy = vi.spyOn(useSettings.getState(), 'save');

    renderWithTooltip(<PasswordGenerator standalone />);
    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('apple-river-stone');
    });

    fireEvent.click(screen.getByRole('tab', { name: /Memorable/i }));
    await waitFor(() => {
      expect(screen.getByRole('radiogroup', { name: /Separator/i })).toBeTruthy();
    });

    // Pick '_' then enable Capitalize: each change regenerates immediately.
    fireEvent.click(screen.getByRole('radio', { name: '_' }));
    await waitFor(() => {
      expect(genSpy).toHaveBeenCalledWith(expect.objectContaining({ words: true, separator: '_', capitalize: false }));
    });
    fireEvent.click(screen.getByRole('checkbox', { name: /Capitalize words/i }));
    await waitFor(() => {
      expect(genSpy).toHaveBeenCalledWith(expect.objectContaining({ words: true, separator: '_', capitalize: true }));
    });

    // ArrowRight on the segmented control moves to the next separator (space).
    const group = screen.getByRole('radiogroup', { name: /Separator/i });
    fireEvent.keyDown(group, { key: 'ArrowRight' });
    await waitFor(() => {
      expect(genSpy).toHaveBeenCalledWith(expect.objectContaining({ separator: ' ' }));
    });

    // Debounced persist carries both new settings.
    await waitFor(() => {
      const lastCall = saveSpy.mock.calls[saveSpy.mock.calls.length - 1][0];
      expect(lastCall).toHaveProperty('generatorSeparator', ' ');
      expect(lastCall).toHaveProperty('generatorCapitalize', true);
    });
  });
});


describe('Password Generator History', () => {
  const mockHistoryItems: GeneratorHistoryEntry[] = [
    { id: 'gh-1', value: 'FAKE-pwd-first-123', createdAt: Math.floor(Date.now() / 1000) - 60, source: 'desktop', host: null },
    { id: 'gh-2', value: 'FAKE-pwd-second-456', createdAt: Math.floor(Date.now() / 1000) - 3600, source: 'extension', host: 'github.com' },
  ];

  beforeEach(() => {
    vi.spyOn(api, 'generatePassword').mockResolvedValue('current-pwd');
    vi.spyOn(api, 'getGeneratorHistory').mockResolvedValue([...mockHistoryItems]);
    vi.spyOn(api, 'deleteGeneratorHistoryEntry').mockResolvedValue(true);
    vi.spyOn(api, 'clearGeneratorHistory').mockResolvedValue(undefined);
    vi.spyOn(api, 'copyPassword').mockResolvedValue(undefined);
  });

  it('renders history items with values masked by default, sources and hosts', async () => {
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getByTestId('generator-history')).toBeTruthy();
      expect(screen.getAllByTestId('generator-history-item').length).toBe(2);
    });

    const items = screen.getAllByTestId('generator-history-item');
    expect(within(items[0]).getByText('desktop')).toBeTruthy();
    expect(within(items[0]).getByTestId('history-entry-value').textContent).toBe('••••••••••••••••••••');
    expect(within(items[1]).getByText('extension')).toBeTruthy();
    expect(within(items[1]).getByTestId('history-entry-host').textContent).toBe('github.com');
    expect(within(items[1]).getByTestId('history-entry-value').textContent).toBe('••••••••••••••••••••');
  });

  it('reveals plaintext password on reveal click and re-masks on click', async () => {
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getAllByTestId('generator-history-item').length).toBe(2);
    });

    const items = screen.getAllByTestId('generator-history-item');
    const revealBtn = within(items[0]).getByTestId('history-reveal-btn');

    fireEvent.click(revealBtn);
    await waitFor(() => {
      expect(within(items[0]).getByTestId('history-entry-value').textContent).toBe('FAKE-pwd-first-123');
    });

    fireEvent.click(revealBtn);
    await waitFor(() => {
      expect(within(items[0]).getByTestId('history-entry-value').textContent).toBe('••••••••••••••••••••');
    });
  });

  it('copies entry password using concealed pasteboard path', async () => {
    const copySpy = vi.spyOn(api, 'copyPassword');
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getAllByTestId('generator-history-item').length).toBe(2);
    });

    const items = screen.getAllByTestId('generator-history-item');
    const copyBtn = within(items[1]).getByTestId('history-copy-btn');

    fireEvent.click(copyBtn);
    await waitFor(() => {
      expect(copySpy).toHaveBeenCalledWith({ password: 'FAKE-pwd-second-456' });
    });
  });

  it('deletes an individual history entry', async () => {
    const deleteSpy = vi.spyOn(api, 'deleteGeneratorHistoryEntry');
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getAllByTestId('generator-history-item').length).toBe(2);
    });

    const items = screen.getAllByTestId('generator-history-item');
    const deleteBtn = within(items[0]).getByTestId('history-delete-btn');

    fireEvent.click(deleteBtn);
    await waitFor(() => {
      expect(deleteSpy).toHaveBeenCalledWith({ id: 'gh-1' });
      expect(screen.getAllByTestId('generator-history-item').length).toBe(1);
    });
  });

  it('clears history with confirmation modal and shows empty state', async () => {
    const clearSpy = vi.spyOn(api, 'clearGeneratorHistory');
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getAllByTestId('generator-history-item').length).toBe(2);
    });

    const clearBtn = screen.getByTestId('clear-history-btn');
    fireEvent.click(clearBtn);

    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: /Clear generator history\?/i })).toBeTruthy();
    });

    const confirmBtn = screen.getByTestId('confirm-clear-history-btn');
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(clearSpy).toHaveBeenCalled();
      expect(screen.getByTestId('generator-history-empty')).toBeTruthy();
      expect(screen.queryByTestId('generator-history-item')).toBeNull();
    });
  });

  it('renders empty state when history has no entries', async () => {
    vi.spyOn(api, 'getGeneratorHistory').mockResolvedValue([]);
    renderWithTooltip(<PasswordGenerator standalone />);

    await waitFor(() => {
      expect(screen.getByTestId('generator-history-empty')).toBeTruthy();
    });
    expect(screen.queryByTestId('clear-history-btn')).toBeNull();
  });

  it('Save as new Login from history item pre-fills password and host', async () => {
    const saveLoginSpy = vi.fn();
    renderWithTooltip(<PasswordGenerator standalone onSaveAsLogin={saveLoginSpy} />);

    await waitFor(() => {
      expect(screen.getAllByTestId('generator-history-item').length).toBe(2);
    });

    const items = screen.getAllByTestId('generator-history-item');
    const saveLoginBtn = within(items[1]).getByTestId('history-save-login-btn');

    fireEvent.click(saveLoginBtn);
    expect(saveLoginSpy).toHaveBeenCalledWith('FAKE-pwd-second-456', 'github.com');
  });
});
