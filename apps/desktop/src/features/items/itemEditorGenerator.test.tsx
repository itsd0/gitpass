import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

import { api } from '../../api/client';
import type { ItemDetail, Settings, VaultInfo } from '../../api/types';
import { useI18n } from '../../i18n';
import { defaultRoute, useApp } from '../../lib/store';
import { TooltipProvider } from '../../ui';
import { useSettings } from '../settings/useSettings';
import { ItemEditor } from './ItemEditor';

const vaults: VaultInfo[] = [
  { id: 'vault-1', name: 'Personal', description: null, itemCount: 1, shared: false, members: [] },
];

function makeDetail(): ItemDetail {
  return {
    id: 'item-1',
    vaultId: 'vault-1',
    category: 'secure_note',
    title: 'Test note',
    subtitle: '',
    favorite: false,
    archived: false,
    deletedAt: null,
    tags: [],
    updatedAt: '2026-10-29T00:00:00Z',
    createdAt: '2026-10-29T00:00:00Z',
    sections: [
      { id: 's1', title: null, fields: [{ id: 'f1', label: 'Password', kind: 'concealed', value: 'old-secret-1' }] },
    ],
    notes: null,
    attachments: [],
    history: [],
    hasTotp: false,
  };
}

const baseSettings: Settings = {
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

describe('Item editor generator popover', () => {
  let curSettings: Settings;

  beforeEach(() => {
    curSettings = { ...baseSettings };
    vi.spyOn(api, 'passwordStrength').mockResolvedValue(85);
    vi.spyOn(api, 'generatePassword').mockImplementation(async (a) =>
      a.pin ? `pin-${a.length}` : a.words ? `words-${a.length}` : `random-${a.length}`,
    );
    vi.spyOn(api, 'getSettings').mockImplementation(async () => ({ ...curSettings }));
    vi.spyOn(api, 'setSettings').mockImplementation(async ({ settings }) => {
      curSettings = { ...settings };
      return curSettings;
    });

    act(() => {
      useApp.setState({ locked: false, route: defaultRoute, modal: null, editing: true });
      useSettings.setState({ settings: curSettings, error: null });
      useI18n.setState({ lang: 'en' });
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  function renderEditor() {
    return render(
      <TooltipProvider>
        <ItemEditor detail={makeDetail()} vaults={vaults} />
      </TooltipProvider>,
    );
  }

  function passwordField() {
    return screen.getByLabelText('Password value') as HTMLInputElement;
  }

  async function openViaDice() {
    renderEditor();
    fireEvent.click(screen.getByRole('button', { name: 'Generate password' }));
    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: 'Password Generator' })).toBeTruthy();
    });
    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('random-20');
    });
  }

  it('opens the popover via the dice button and applies with Use password', async () => {
    await openViaDice();
    fireEvent.click(screen.getByRole('button', { name: 'Use password' }));
    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: 'Password Generator' })).toBeNull();
    });
    expect(passwordField().value).toBe('random-20');
  });

  it('opens the popover via ⌥⌘G while the password field is focused', async () => {
    renderEditor();
    const field = passwordField();
    field.focus();
    fireEvent.keyDown(field, { key: 'g', altKey: true, metaKey: true });
    await waitFor(() => {
      expect(screen.getByRole('dialog', { name: 'Password Generator' })).toBeTruthy();
    });
    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('random-20');
    });
  });

  it('regenerates when options change', async () => {
    await openViaDice();
    fireEvent.click(screen.getByRole('tab', { name: 'Memorable' }));
    // Mode switch clamps the current length (20) to the Words max (15).
    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('words-15');
    });
    const lengthInput = screen.getByLabelText('Words input');
    fireEvent.change(lengthInput, { target: { value: '8' } });
    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('words-8');
    });
  });

  it('Enter inside the popover applies the password and closes', async () => {
    await openViaDice();
    const lengthInput = screen.getByLabelText('Length input');
    fireEvent.keyDown(lengthInput, { key: 'Enter' });
    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: 'Password Generator' })).toBeNull();
    });
    expect(passwordField().value).toBe('random-20');
  });

  it('Esc closes the popover and returns focus to the password field', async () => {
    await openViaDice();
    // Focus moved into the popover on open; Esc there must not close the editor.
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Regenerate');
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: 'Password Generator' })).toBeNull();
    });
    expect(document.activeElement).toBe(passwordField());
    expect(screen.getByTestId('item-editor')).toBeTruthy();
  });

  it('remembers options from settings and persists changes', async () => {
    curSettings = { ...baseSettings, generatorMode: 'pin', generatorLength: 8 };
    act(() => {
      useSettings.setState({ settings: curSettings, error: null });
    });
    renderEditor();
    fireEvent.click(screen.getByRole('button', { name: 'Generate password' }));
    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('pin-8');
    });
    const digitsInput = screen.getByLabelText('Digits input');
    fireEvent.change(digitsInput, { target: { value: '6' } });
    await waitFor(() => {
      expect(screen.getByTestId('colored-password').textContent).toBe('pin-6');
    });
    // Settings persistence is debounced by 250ms in the generator.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 350));
    });
    expect(curSettings.generatorLength).toBe(6);
    expect(curSettings.generatorMode).toBe('pin');
  });
});
