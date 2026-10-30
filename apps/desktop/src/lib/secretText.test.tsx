import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../api/client';
import { translate, useI18n } from '../i18n';
import { TooltipProvider } from '../ui';
import { FieldRow } from '../features/items/FieldView';
import { LargeTypeDialog } from '../features/items/LargeType';
import { ColoredPassword } from '../features/new-item/PasswordGenerator';
import { SettingsPage } from '../features/settings/SettingsPage';
import { searchSettings } from '../features/settings/searchIndex';
import { COLOR_PASSWORDS_KEY, loadColorPasswords, useColorPasswords } from '../features/settings/colorPasswords';
import { useSettings } from '../features/settings/useSettings';
import { useApp } from './store';
import { SecretText, charKind } from './secretText';

const SECRET = 'Ab3-xY 7!é9';

beforeEach(() => { localStorage.clear(); useColorPasswords.getState().set(true); useApp.setState({ locked: false }); });
afterEach(() => {
  cleanup(); vi.restoreAllMocks(); localStorage.clear(); useColorPasswords.getState().set(true);
  useSettings.setState({ settings: null }); useI18n.getState().setLang('en');
  useApp.setState({ route: { name: 'settings', tab: 'general' } });
});

describe('charKind', () => {
  it('classifies letters, digits, symbols, spaces and non-ASCII', () => {
    expect(['a', 'Z', 'é', 'ก', 'မ'].map(charKind)).toEqual(['letter', 'letter', 'letter', 'letter', 'letter']);
    expect(['5', '٣'].map(charKind)).toEqual(['digit', 'digit']);
    expect(['!', '#', '-', '·', '€'].map(charKind)).toEqual(['symbol', 'symbol', 'symbol', 'symbol', 'symbol']);
    expect([' ', '\t'].map(charKind)).toEqual(['space', 'space']);
  });
});

describe('SecretText', () => {
  it('renders one span per character with a kind, keeps the exact text and one aria-label', () => {
    render(<SecretText value={SECRET} testId="s" />);
    const el = screen.getByTestId('s');
    expect(el.textContent).toBe(SECRET);
    expect(el.getAttribute('role')).toBe('img');
    expect(el.getAttribute('aria-label')).toBe(SECRET);
    const kinds = Array.from(el.querySelectorAll('[data-kind]')).map((n) => (n as HTMLElement).dataset.kind);
    expect(kinds).toEqual(['letter', 'letter', 'digit', 'symbol', 'letter', 'letter', 'space', 'digit', 'symbol', 'letter', 'digit']);
    const digit = el.querySelector('[data-kind="digit"]')!;
    expect(digit.className).toContain('text-accent');
    expect(el.querySelector('[data-kind="symbol"]')!.className).toContain('text-blue-600');
    expect(el.querySelector('[data-kind="symbol"]')!.className).toContain('dark:text-blue-400');
    expect(el.querySelector('[data-kind="space"]')!.className).toContain("before:content-['·']");
  });

  it('renders the plain value when the setting is off', () => {
    useColorPasswords.getState().set(false);
    render(<SecretText value={SECRET} testId="s" />);
    const el = screen.getByTestId('s');
    expect(el.textContent).toBe(SECRET);
    expect(el.children.length).toBe(0);
    expect(el.getAttribute('aria-label')).toBeNull();
  });
});

describe('colour-code setting storage', () => {
  it('is on by default, stored per device only when turned off', () => {
    expect(loadColorPasswords()).toBe(true);
    useColorPasswords.getState().set(false);
    expect(localStorage.getItem(COLOR_PASSWORDS_KEY)).toBe('off');
    expect(loadColorPasswords()).toBe(false);
    useColorPasswords.getState().set(true);
    expect(localStorage.getItem(COLOR_PASSWORDS_KEY)).toBeNull();
  });
});

describe('where it applies', () => {
  const ctx = { vaultId: 'v', itemId: 'i' };

  it('desktop item detail reveal is colour-coded and copy still goes through the backend with the real value', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    const copy = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    render(<TooltipProvider><FieldRow field={{ id: 'pw', label: 'password', kind: 'concealed', value: null }} ctx={ctx} /></TooltipProvider>);
    await act(async () => { fireEvent.click(screen.getByLabelText('Reveal password')); });
    const v = screen.getByTestId('concealed-value');
    expect(v.textContent).toBe(SECRET);
    expect(v.querySelectorAll('[data-kind="digit"]').length).toBe(3);
    await act(async () => { fireEvent.click(screen.getByLabelText('Copy password')); });
    expect(copy).toHaveBeenCalledWith({ vaultId: 'v', itemId: 'i', fieldId: 'pw' });
    expect(v.textContent).toBe(SECRET);
  });

  it('a revealed card number stays plain grouped text', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue('4111111111111111');
    render(<TooltipProvider><FieldRow field={{ id: 'cn', label: 'number', kind: 'concealed', value: null, masked: '•••• 1111' }} ctx={ctx} /></TooltipProvider>);
    await act(async () => { fireEvent.click(screen.getByLabelText('Reveal number')); });
    expect(screen.getByTestId('concealed-value').querySelector('[data-kind]')).toBeNull();
  });

  it('the generator preview uses the same colours and turns plain with the setting off', () => {
    const { unmount } = render(<ColoredPassword password="a1-b" />);
    expect(screen.getByTestId('colored-password').textContent).toBe('a1-b');
    expect(screen.getByTestId('colored-password').querySelector('[data-kind="symbol"]')).not.toBeNull();
    unmount();
    useColorPasswords.getState().set(false);
    render(<ColoredPassword password="a1-b" />);
    expect(screen.getByTestId('colored-password').querySelector('[data-kind]')).toBeNull();
  });

  it('Large type keeps its index numbers; colours follow the setting', () => {
    const { unmount } = render(<LargeTypeDialog open onOpenChange={() => {}} title="password" value="a1!" />);
    expect(screen.getAllByTestId('large-type-pos').map((n) => n.textContent)).toEqual(['1', '2', '3']);
    const chars = screen.getAllByTestId('large-type-char').map((c) => c.querySelector('span')!);
    expect(chars[1]!.className).toContain('text-accent');
    expect(chars[2]!.className).toContain('text-blue-600');
    unmount();
    useColorPasswords.getState().set(false);
    render(<LargeTypeDialog open onOpenChange={() => {}} title="password" value="a1!" />);
    const plain = screen.getAllByTestId('large-type-char').map((c) => c.querySelector('span')!);
    expect(plain[1]!.className).not.toContain('text-accent');
    expect(plain[2]!.className).not.toContain('text-blue-600');
    expect(screen.getAllByTestId('large-type-pos').map((n) => n.textContent)).toEqual(['1', '2', '3']);
  });
});

describe('Settings → Appearance', () => {
  beforeEach(() => {
    vi.spyOn(api, 'getSettings').mockResolvedValue({ autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'light', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false });
  });
  it('has an on-by-default switch that turns it off, and is searchable', async () => {
    const u = userEvent.setup();
    render(<SettingsPage initialTab="general" />);
    const sw = await screen.findByTestId('color-passwords-switch');
    expect(sw.getAttribute('aria-checked')).toBe('true');
    await u.click(sw);
    expect(sw.getAttribute('aria-checked')).toBe('false');
    expect(localStorage.getItem(COLOR_PASSWORDS_KEY)).toBe('off');
    expect(searchSettings('colour', (k) => translate('en', k)).some((r) => r.id === 'setting-color-passwords')).toBe(true);
  });
  it('is translated in Burmese and Thai', () => {
    for (const lang of ['my', 'th'] as const) {
      expect(translate(lang, 'settings.colorPasswords')).not.toBe(translate('en', 'settings.colorPasswords'));
      expect(translate(lang, 'settings.colorPasswordsHelp')).not.toBe(translate('en', 'settings.colorPasswordsHelp'));
    }
  });
});
