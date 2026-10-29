import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '../../api/client';
import { useI18n } from '../../i18n';
import { useApp } from '../../lib/store';
import { SettingsPage } from './SettingsPage';
import { useSettings } from './useSettings';

const defaults = {
  autoLockMinutes: 10,
  clipboardClearSecs: 30,
  theme: 'dark' as const,
  language: 'en' as const,
  biometricUnlock: false,
  quickAccessHotkey: 'CmdOrCtrl+Shift+Space',
  hibpCheck: false,
};

beforeEach(() => {
  vi.spyOn(api, 'getSettings').mockResolvedValue({ ...defaults });
  vi.spyOn(api, 'listVaults').mockResolvedValue([
    { id: 'v1', name: 'Personal', description: 'My passwords', itemCount: 12, icon: 'vault', color: '#f97316', members: [] },
  ]);
  vi.spyOn(api, 'listDevices').mockResolvedValue([
    { id: 'd1', name: 'MacBook Pro', current: true, revoked: false, lastSeen: new Date().toISOString() },
  ]);
  vi.spyOn(api, 'sshAgentInfo').mockResolvedValue({
    socketPath: '/tmp/ssh.sock',
    configLine: 'IdentityAgent /tmp/ssh.sock',
    running: true,
    approveEachUse: false,
  });
  vi.spyOn(api, 'syncStatus').mockResolvedValue({
    remote: 'git@github.com:test/vault.git',
    lastSync: new Date().toISOString(),
    ahead: 0,
    behind: 0,
    dirty: false,
  });
  // Mock scrollIntoView
  Element.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useSettings.setState({ settings: null });
  useI18n.getState().setLang('en');
  useApp.setState({ route: { name: 'settings', tab: 'general' } });
});

describe('Settings search and consistent layout', () => {
  it('renders search input at the top of the sidebar with placeholder and shortcut hint', async () => {
    render(<SettingsPage />);
    const searchInput = await screen.findByTestId('settings-search-input');
    expect(searchInput).toBeTruthy();
    expect(searchInput).toHaveAttribute('placeholder', 'Search settings… (⌘F)');
    expect(searchInput).toHaveAttribute('aria-label', 'Search settings');
  });

  it('focuses search input when ⌘F or Ctrl+F is pressed', async () => {
    render(<SettingsPage />);
    const searchInput = await screen.findByTestId('settings-search-input');
    expect(document.activeElement).not.toBe(searchInput);

    fireEvent.keyDown(window, { key: 'f', metaKey: true });
    expect(document.activeElement).toBe(searchInput);
  });

  it('filters settings across all tabs and displays matching results with tab name', async () => {
    const user = userEvent.setup();
    render(<SettingsPage />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'password');

    const results = await screen.findByTestId('settings-search-results');
    expect(results).toBeTruthy();

    const options = screen.getAllByRole('option');
    expect(options.length).toBeGreaterThanOrEqual(1);

    // Option shows setting title and tab name
    expect(screen.getByText('Change master password')).toBeTruthy();
    expect(screen.getAllByText('Security').length).toBeGreaterThan(0);
  });

  it('navigates through search results with Arrow keys and activates with Enter', async () => {
    render(<SettingsPage initialTab="general" />);
    const searchInput = await screen.findByTestId('settings-search-input');

    fireEvent.change(searchInput, { target: { value: 'password' } });

    const options = await screen.findAllByRole('option');
    expect(options[0]).toHaveAttribute('aria-selected', 'true');

    // Arrow down moves selection
    fireEvent.keyDown(searchInput, { key: 'ArrowDown' });
    if (options.length > 1) {
      expect(options[1]).toHaveAttribute('aria-selected', 'true');
    }

    // Press Enter to select
    fireEvent.keyDown(searchInput, { key: 'Enter' });

    // Should switch to Security tab
    await waitFor(() => {
      expect(screen.getByRole('tabpanel', { name: 'Security' })).toBeTruthy();
    });
    await waitFor(() => expect(Element.prototype.scrollIntoView).toHaveBeenCalled());
  });

  it('clicking a search result switches tab, scrolls, and highlights the target row', async () => {
    const user = userEvent.setup();
    render(<SettingsPage initialTab="general" />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'theme');
    const resultBtn = await screen.findByRole('option', { name: /Theme/i });

    await user.click(resultBtn);

    const themeRow = document.getElementById('setting-theme');
    expect(themeRow).toBeTruthy();
    expect(themeRow?.className).toContain('ring-accent');
  });

  it('Escape clears the search input and restores the default 7 tabs', async () => {
    const user = userEvent.setup();
    render(<SettingsPage />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'theme');
    expect(screen.queryByRole('tablist')).toBeNull();

    fireEvent.keyDown(searchInput, { key: 'Escape' });
    expect(searchInput).toHaveValue('');

    // Tablist is restored
    const tablist = screen.getByRole('tablist');
    expect(tablist).toBeTruthy();
    expect(screen.getAllByRole('tab').length).toBe(7);
  });

  it('shows empty results state with localized message and Clear button when no settings match', async () => {
    const user = userEvent.setup();
    render(<SettingsPage />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'nonexistentquery123');

    const empty = await screen.findByTestId('settings-search-empty');
    expect(empty).toBeTruthy();
    expect(screen.getByText('No settings match “nonexistentquery123”')).toBeTruthy();

    const clearBtn = screen.getByRole('button', { name: 'Clear search' });
    await user.click(clearBtn);

    expect(searchInput).toHaveValue('');
    expect(screen.getByRole('tablist')).toBeTruthy();
  });

  it('supports localized searching in Burmese (my)', async () => {
    useI18n.getState().setLang('my');
    useSettings.setState({ settings: { ...defaults, language: 'my' } });
    vi.spyOn(api, 'getSettings').mockResolvedValue({ ...defaults, language: 'my' });
    const user = userEvent.setup();
    render(<SettingsPage initialTab="general" />);

    const searchInput = await screen.findByTestId('settings-search-input');
    expect(searchInput).toHaveAttribute('placeholder', 'ဆက်တင်များကို ရှာရန်… (⌘F)');

    // Search for password in Burmese: စကားဝှက်
    await user.type(searchInput, 'စကားဝှက်');

    const results = await screen.findByTestId('settings-search-results');
    expect(results).toBeTruthy();
    expect(screen.getByText('မာစတာ စကားဝှက် ပြောင်းရန်')).toBeTruthy();

    const option = screen.getAllByRole('option')[0];
    await user.click(option);

    expect(screen.getByRole('tabpanel', { name: 'လုံခြုံရေး' })).toBeTruthy();
  });

  it('supports localized searching in Thai (th)', async () => {
    useI18n.getState().setLang('th');
    useSettings.setState({ settings: { ...defaults, language: 'th' } });
    vi.spyOn(api, 'getSettings').mockResolvedValue({ ...defaults, language: 'th' });
    const user = userEvent.setup();
    render(<SettingsPage initialTab="general" />);

    const searchInput = await screen.findByTestId('settings-search-input');
    expect(searchInput).toHaveAttribute('placeholder', 'ค้นหาการตั้งค่า… (⌘F)');

    // Search for password in Thai: รหัสผ่าน
    await user.type(searchInput, 'รหัสผ่าน');

    const results = await screen.findByTestId('settings-search-results');
    expect(results).toBeTruthy();
    expect(screen.getByText('เปลี่ยนรหัสผ่านหลัก')).toBeTruthy();
  });

  it('allows keyboard navigation across vertical tabs with Arrow keys when search is empty', async () => {
    render(<SettingsPage initialTab="general" />);
    const tabs = await screen.findAllByRole('tab');

    tabs[0].focus();
    fireEvent.keyDown(tabs[0], { key: 'ArrowDown' });

    expect(screen.getByRole('tab', { name: /Security/i })).toHaveAttribute('aria-selected', 'true');

    fireEvent.keyDown(tabs[1], { key: 'ArrowUp' });
    expect(screen.getByRole('tab', { name: /General/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('groups settings under consistent section headings across tabs', async () => {
    render(<SettingsPage initialTab="security" />);

    expect(await screen.findByRole('heading', { name: 'Auto-Lock & Clipboard' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Authentication & Protection' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Master Credentials' })).toBeTruthy();
  });

  it('searches for sleep and screen lock settings and switches to security tab', async () => {
    const user = userEvent.setup();
    render(<SettingsPage initialTab="general" />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'sleep');
    const sleepResult = await screen.findByRole('option', { name: /Lock when Mac sleeps/i });
    expect(sleepResult).toBeTruthy();
    await user.click(sleepResult);

    await waitFor(() => {
      expect(screen.getByRole('tabpanel', { name: 'Security' })).toBeTruthy();
    });
  });

  it('searches for website icons / favicon and targets appearance row', async () => {
    const user = userEvent.setup();
    render(<SettingsPage initialTab="security" />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'favicon');
    const result = await screen.findByRole('option', { name: /website icons/i });
    expect(result).toBeTruthy();
    await user.click(result);

    const row = document.getElementById('setting-show-website-icons');
    expect(row).toBeTruthy();
    expect(row?.className).toContain('ring-accent');
  });

  it('searches for setup guide and highlights the row in General tab', async () => {
    const user = userEvent.setup();
    render(<SettingsPage initialTab="security" />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'setup guide');
    const result = await screen.findByRole('option', { name: /Show setup guide/i });
    expect(result).toBeTruthy();
    await user.click(result);

    await waitFor(() => {
      expect(screen.getByRole('tabpanel', { name: 'General' })).toBeTruthy();
    });
    const row = document.getElementById('setting-setup-guide');
    expect(row).toBeTruthy();
    expect(row?.className).toContain('ring-accent');
  });

  it('searches for generator history setting and highlights the row in Security tab', async () => {
    const user = userEvent.setup();
    render(<SettingsPage initialTab="general" />);
    const searchInput = await screen.findByTestId('settings-search-input');

    await user.type(searchInput, 'generator history');
    const result = await screen.findByRole('option', { name: /Remember generated passwords/i });
    expect(result).toBeTruthy();
    await user.click(result);

    await waitFor(() => {
      expect(screen.getByRole('tabpanel', { name: 'Security' })).toBeTruthy();
    });
    const row = document.getElementById('setting-remember-generated-passwords');
    expect(row).toBeTruthy();
    expect(row?.className).toContain('ring-accent');
  });
});
