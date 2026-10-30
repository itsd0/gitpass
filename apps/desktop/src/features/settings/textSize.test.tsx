import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { translate, useI18n } from '../../i18n';
import { useApp } from '../../lib/store';
import { builtinCommands } from '../palette/commands';
import { SettingsPage } from './SettingsPage';
import { searchSettings } from './searchIndex';
import { TEXT_SIZE_KEY, TEXT_SIZE_PERCENT, loadTextSize, textSizeKeyAction, useTextSize, useTextSizeShortcuts } from './textSize';
import { useSettings } from './useSettings';

const root = () => document.documentElement.style.fontSize;
const key = (init: KeyboardEventInit) => act(() => { window.dispatchEvent(new KeyboardEvent('keydown', { cancelable: true, ...init })); });
function Host() { useTextSizeShortcuts(); return null; }

beforeEach(() => {
  localStorage.clear(); useTextSize.getState().set('default');
  vi.spyOn(api, 'getSettings').mockResolvedValue({ autoLockMinutes: 10, clipboardClearSecs: 30, theme: 'light', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false });
});
afterEach(() => {
  cleanup(); vi.restoreAllMocks(); localStorage.clear(); useTextSize.getState().set('default');
  useSettings.setState({ settings: null }); useI18n.getState().setLang('en');
  useApp.setState({ route: { name: 'settings', tab: 'general' } });
});

describe('text size setting', () => {
  it('round-trips through localStorage and scales the root font size', () => {
    expect(loadTextSize()).toBe('default');
    useTextSize.getState().set('large');
    expect(localStorage.getItem(TEXT_SIZE_KEY)).toBe('large');
    expect(root()).toBe(`${TEXT_SIZE_PERCENT.large}%`);
    expect(loadTextSize()).toBe('large');
    useTextSize.getState().set('default');
    expect(localStorage.getItem(TEXT_SIZE_KEY)).toBeNull();
    expect(root()).toBe('');
  });
  it('ignores an invalid stored value', () => {
    localStorage.setItem(TEXT_SIZE_KEY, 'huge');
    expect(loadTextSize()).toBe('default');
  });
  it('steps within the five sizes and clamps at both ends', () => {
    const s = useTextSize.getState();
    s.step(-1); s.step(-1); expect(useTextSize.getState().size).toBe('smaller');
    for (let i = 0; i < 8; i++) useTextSize.getState().step(1);
    expect(useTextSize.getState().size).toBe('largest');
    useTextSize.getState().reset(); expect(useTextSize.getState().size).toBe('default');
  });
  it('maps ⌘+ ⌘= ⌘− ⌘0 on macOS and Ctrl elsewhere, ignoring other combos', () => {
    const e = (o: Partial<KeyboardEvent>) => ({ key: '', metaKey: false, ctrlKey: false, altKey: false, ...o });
    expect(textSizeKeyAction(e({ key: '=', metaKey: true }), true)).toBe('larger');
    expect(textSizeKeyAction(e({ key: '+', metaKey: true }), true)).toBe('larger');
    expect(textSizeKeyAction(e({ key: '-', metaKey: true }), true)).toBe('smaller');
    expect(textSizeKeyAction(e({ key: '0', metaKey: true }), true)).toBe('reset');
    expect(textSizeKeyAction(e({ key: '=', ctrlKey: true }), true)).toBeNull();
    expect(textSizeKeyAction(e({ key: '=', ctrlKey: true }), false)).toBe('larger');
    expect(textSizeKeyAction(e({ key: '=' }), true)).toBeNull();
    expect(textSizeKeyAction(e({ key: '0', metaKey: true, altKey: true }), true)).toBeNull();
    expect(textSizeKeyAction(e({ key: 'k', metaKey: true }), true)).toBeNull();
  });
  it('keyboard shortcuts change, persist and reset the size', () => {
    const mac = /Mac|iPhone|iPad/.test(navigator.platform);
    const mod = mac ? { metaKey: true } : { ctrlKey: true };
    render(<Host />);
    key({ key: '=', ...mod }); expect(useTextSize.getState().size).toBe('larger');
    key({ key: '+', shiftKey: true, ...mod }); expect(useTextSize.getState().size).toBe('large');
    expect(localStorage.getItem(TEXT_SIZE_KEY)).toBe('large');
    key({ key: '-', ...mod }); expect(useTextSize.getState().size).toBe('larger');
    key({ key: '0', ...mod }); expect(useTextSize.getState().size).toBe('default');
    expect(root()).toBe('');
  });
  it('applies the saved size on mount and follows changes from another window', () => {
    localStorage.setItem(TEXT_SIZE_KEY, 'smaller'); useTextSize.setState({ size: loadTextSize() });
    render(<Host />);
    expect(root()).toBe(`${TEXT_SIZE_PERCENT.smaller}%`);
    localStorage.setItem(TEXT_SIZE_KEY, 'largest');
    act(() => { window.dispatchEvent(new StorageEvent('storage', { key: TEXT_SIZE_KEY })); });
    expect(useTextSize.getState().size).toBe('largest');
    expect(root()).toBe(`${TEXT_SIZE_PERCENT.largest}%`);
  });
  it('Settings → Appearance shows the control and a preview, and picking a step applies it', async () => {
    const u = userEvent.setup(); render(<SettingsPage initialTab="general" />);
    expect(await screen.findByText('Text size')).toBeTruthy();
    expect(screen.getByTestId('text-size-preview')).toBeTruthy();
    expect(screen.getByRole('radio', { name: 'Default' }).getAttribute('aria-checked')).toBe('true');
    await u.click(screen.getByRole('radio', { name: 'Largest' }));
    expect(screen.getByRole('radio', { name: 'Largest' }).getAttribute('aria-checked')).toBe('true');
    expect(root()).toBe(`${TEXT_SIZE_PERCENT.largest}%`);
    expect(localStorage.getItem(TEXT_SIZE_KEY)).toBe('largest');
  });
  it('is available as command palette actions and in settings search', () => {
    const ids = builtinCommands().map((c) => c.id);
    expect(ids).toEqual(expect.arrayContaining(['text-larger', 'text-smaller', 'text-reset']));
    builtinCommands().find((c) => c.id === 'text-larger')!.run();
    expect(useTextSize.getState().size).toBe('larger');
    builtinCommands().find((c) => c.id === 'text-reset')!.run();
    expect(useTextSize.getState().size).toBe('default');
    expect(searchSettings('zoom', (k) => translate('en', k)).some((r) => r.id === 'setting-text-size')).toBe(true);
  });
});
