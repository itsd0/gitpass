import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../api/client';
import type { ItemDetail, ItemSummary, Settings } from '../api/types';
import { useI18n } from '../i18n';
import { useApp } from './store';
import { useSettings } from '../features/settings/useSettings';
import { Toaster, TooltipProvider, useToasts } from '../ui';
import { copyPrimarySecret, copyTotpCode, copyUsername } from '../features/items/actions';
import { FieldRow } from '../features/items/FieldView';
import { PasswordGenerator } from '../features/new-item/PasswordGenerator';
import { QuickAccess } from '../features/quick-access/QuickAccess';
import { SettingsPage } from '../features/settings/SettingsPage';
import { isConcealedCopy, toastConcealedCopied } from './concealedCopy';

const settings = (clipboardClearSecs: number): Settings => ({
  autoLockMinutes: 10,
  clipboardClearSecs,
  theme: 'dark',
  language: 'en',
  biometricUnlock: false,
  quickAccessHotkey: 'CmdOrCtrl+Shift+Space',
  hibpCheck: false,
});

const summary: ItemSummary = {
  id: 'i1', vaultId: 'v1', category: 'login', title: 'Fake Mail', subtitle: 'fake@example.invalid',
  favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: '2026-10-01T00:00:00Z',
  createdAt: '2026-10-01T00:00:00Z', url: 'https://example.invalid',
};
const detail = (): ItemDetail => ({
  ...summary, hasTotp: true, notes: null, attachments: [], history: [],
  sections: [{ id: 's', title: null, fields: [
    { id: 'fu', label: 'username', kind: 'text', value: 'fake@example.invalid' },
    { id: 'fp', label: 'password', kind: 'concealed', value: null },
    { id: 'ft', label: 'one-time password', kind: 'totp', value: null },
    { id: 'fw', label: 'website', kind: 'url', value: 'https://example.invalid/login' },
  ] }],
});

describe('concealed copy toast', () => {
  beforeEach(() => {
    useI18n.setState({ lang: 'en' });
    useToasts.setState({ list: [] });
    useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, locked: false, selectedItemId: 'i1', modal: null, editing: false });
    useSettings.setState({ settings: settings(90), error: null });
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
    useToasts.setState({ list: [] });
    useSettings.setState({ settings: null, error: null });
  });

  it('counts down once per second from the clipboard setting and announces only the start and Cleared', () => {
    vi.useFakeTimers();
    render(<Toaster />);
    act(() => { toastConcealedCopied('Password copied'); });
    expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent('Password copied · clears in 90s');
    expect(screen.getByTestId('toast-copy-live')).toHaveTextContent('Password copied · clears in 90s');
    expect(screen.getByTestId('toast-copy-cleared')).toHaveTextContent('');
    expect(screen.getByTestId('toast-clear-now')).toHaveTextContent('Clear now');

    act(() => { vi.advanceTimersByTime(1000); });
    expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent('Password copied · clears in 89s');
    expect(screen.getByTestId('toast-copy-live')).toHaveTextContent('Password copied · clears in 90s');

    for (let i = 0; i < 89; i++) act(() => { vi.advanceTimersByTime(1000); });
    expect(screen.getByTestId('toast-copy-live')).toHaveTextContent('Cleared');
    expect(screen.getByTestId('toast-copy-cleared')).toHaveTextContent('Cleared');
    expect(screen.queryByTestId('toast-clear-now')).toBeNull();
  });

  it('does not move focus onto the toast', () => {
    render(<Toaster />);
    const input = document.body.appendChild(document.createElement('input'));
    input.focus();
    act(() => { toastConcealedCopied('Password copied'); });
    expect(document.activeElement).toBe(input);
    input.remove();
  });

  it('does not animate the progress bar when reduced motion is requested', () => {
    const prev = window.matchMedia;
    window.matchMedia = ((q: string) => ({
      matches: q.includes('prefers-reduced-motion'),
      media: q,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    })) as typeof window.matchMedia;
    render(<Toaster />);
    act(() => { toastConcealedCopied('Password copied'); });
    const fill = screen.getByTestId('toast-copy-bar').firstElementChild as HTMLElement;
    expect(fill.style.transition).toBe('none');
    expect(fill.className).toContain('bg-accent');
    window.matchMedia = prev;
  });

  it('links to the clipboard setting when automatic clearing is off', async () => {
    useSettings.setState({ settings: settings(0), error: null });
    render(<><Toaster /><SettingsPage /></>);
    act(() => { toastConcealedCopied('Password copied'); });
    expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent("Password copied · won't be cleared automatically");
    expect(screen.queryByTestId('toast-clear-now')).toBeNull();
    fireEvent.click(screen.getByTestId('toast-clipboard-setting'));
    expect(useApp.getState().route).toEqual({ name: 'settings', tab: 'security', highlight: 'setting-clipboard' });
    await waitFor(() => expect(document.getElementById('setting-clipboard')?.className).toMatch(/ring-accent/));
  });

  it('Clear now clears the clipboard only while it still holds our value', async () => {
    let store = '';
    const clip = {
      writeText: async (s: string) => { store = s; },
      readText: async () => store,
    };
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: clip });
    const items = await api.listItems({ filter: { kind: 'all' } });
    const login = items.find((i) => i.category === 'login' && i.title === 'GitHub (demo)')!;
    const full = await api.getItem({ vaultId: login.vaultId, itemId: login.id });
    const password = full.sections.flatMap((s) => s.fields).find((f) => f.kind === 'concealed' && /pass/i.test(f.label))!;
    render(<Toaster />);

    await api.copyField({ vaultId: login.vaultId, itemId: login.id, fieldId: password.id });
    store = 'user-copied-other';
    act(() => { toastConcealedCopied('Password copied'); });
    fireEvent.click(screen.getByTestId('toast-clear-now'));
    await waitFor(() => expect(screen.queryByTestId('toast-copy-visual')).toBeNull());
    expect(store).toBe('user-copied-other');

    await api.copyField({ vaultId: login.vaultId, itemId: login.id, fieldId: password.id });
    const ours = store;
    expect(ours.length).toBeGreaterThan(0);
    act(() => { toastConcealedCopied('Password copied'); });
    fireEvent.click(screen.getByTestId('toast-clear-now'));
    await waitFor(() => expect(screen.getByTestId('toast-copy-cleared')).toHaveTextContent('Cleared'));
    expect(store).toBe('');
  });

  it('password and one-time code copies count down; a username copy does not', async () => {
    vi.spyOn(api, 'getItem').mockResolvedValue(detail());
    vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    render(<Toaster />);
    await copyPrimarySecret('v1', 'i1');
    expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent('Password copied · clears in 90s');

    await copyUsername('v1', 'i1');
    expect(screen.queryByTestId('toast-copy-visual')).toBeNull();
    expect(screen.getByText('Username copied')).toBeTruthy();

    await copyTotpCode('v1', 'i1');
    expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent('One-time code copied · clears in 90s');
  });

  it('item detail, Quick Access and the generator use the countdown only for secrets', async () => {
    useSettings.setState({ settings: settings(45), error: null });
    vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    vi.spyOn(api, 'getItem').mockResolvedValue(detail());
    vi.spyOn(api, 'listItems').mockResolvedValue([summary]);
    vi.spyOn(api, 'generatePassword').mockResolvedValue('FAKE-gen-pw');
    vi.spyOn(api, 'passwordStrength').mockResolvedValue(80);
    vi.spyOn(api, 'getGeneratorHistory').mockResolvedValue([]);
    vi.spyOn(api, 'copyPassword').mockResolvedValue(undefined);

    const { unmount } = render(<TooltipProvider><Toaster /><FieldRow field={{ id: 'fu', label: 'username', kind: 'text', value: 'fake@example.invalid' }} ctx={{ vaultId: 'v1', itemId: 'i1' }} /></TooltipProvider>);
    fireEvent.click(screen.getByLabelText('Copy username'));
    await waitFor(() => expect(screen.getByText('Copied username')).toBeTruthy());
    expect(screen.queryByTestId('toast-copy-visual')).toBeNull();
    unmount();
    useToasts.setState({ list: [] });

    const detailView = render(<TooltipProvider><Toaster /><FieldRow field={{ id: 'fp', label: 'password', kind: 'concealed', value: null }} ctx={{ vaultId: 'v1', itemId: 'i1' }} /></TooltipProvider>);
    fireEvent.click(screen.getByLabelText('Copy password'));
    await waitFor(() => expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent('Password copied · clears in 45s'));
    detailView.unmount();
    useToasts.setState({ list: [] });

    const qa = render(<><Toaster /><QuickAccess /></>);
    await waitFor(() => expect(screen.getAllByText('Fake Mail').length).toBeGreaterThan(0));
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });
    await waitFor(() => expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent('Password copied · clears in 45s'));
    qa.unmount();
    useToasts.setState({ list: [] });

    render(<TooltipProvider><Toaster /><PasswordGenerator standalone /></TooltipProvider>);
    await waitFor(() => expect(screen.getByTestId('colored-password')).toHaveTextContent('FAKE-gen-pw'));
    fireEvent.click(screen.getByRole('button', { name: 'Copy' }));
    await waitFor(() => expect(screen.getByTestId('toast-copy-visual')).toHaveTextContent('Copied password to clipboard · clears in 45s'));
  });

  it('treats card numbers, CVVs and API secrets as concealed, and usernames and URLs as plain', () => {
    expect(isConcealedCopy({ id: 'password', label: 'password', kind: 'concealed' }, 'login')).toBe(true);
    expect(isConcealedCopy({ id: 'otp', label: 'one-time password', kind: 'totp' }, 'login')).toBe(true);
    expect(isConcealedCopy({ id: 'credential', label: 'credential', kind: 'concealed' }, 'api_credential')).toBe(true);
    expect(isConcealedCopy({ id: 'number', label: 'number', kind: 'text' }, 'credit_card')).toBe(true);
    expect(isConcealedCopy({ id: 'cvv', label: 'cvv', kind: 'text' }, 'credit_card')).toBe(true);
    expect(isConcealedCopy({ id: 'username', label: 'username', kind: 'text' }, 'login')).toBe(false);
    expect(isConcealedCopy({ id: 'website', label: 'website', kind: 'url' }, 'login')).toBe(false);
    expect(isConcealedCopy({ id: 'expiry', label: 'expiry', kind: 'month_year' }, 'credit_card')).toBe(false);
  });
});
