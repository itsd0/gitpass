import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { RELEASES } from './releases';
import { dictionaries, useI18n } from '../i18n';
import { useApp } from '../lib/store';
import { useSettings } from '../features/settings/useSettings';
import * as updater from '../lib/updater';
import { useWhatsNewOnStartup, resetWhatsNewStartupCheckForTesting } from './useWhatsNew';
import { WhatsNewModal } from './WhatsNewModal';
import { builtinCommands } from '../features/palette/commands';
import { whatsNewFeature } from './index';
import { AboutTab } from '../features/settings/tabs/AboutTab';
import { api } from '../api/client';
import type { Settings } from '../api/types';

function StartupTestComponent() {
  useWhatsNewOnStartup();
  return <div data-testid="startup-mounted">mounted</div>;
}

const baseSettings: Settings = {
  autoLockMinutes: 5,
  clipboardClearSecs: 90,
  theme: 'dark',
  language: 'en',
  biometricUnlock: false,
  quickAccessHotkey: 'CommandOrControl+Shift+Space',
  hibpCheck: false,
};

describe("What's new", () => {
  beforeEach(() => {
    cleanup();
    resetWhatsNewStartupCheckForTesting();
    useI18n.setState({ lang: 'en' });
    useApp.setState({ modal: null, route: { name: 'items', filter: { kind: 'all', sort: 'updated' } } });
    useSettings.setState({ settings: { ...baseSettings }, error: null });
    vi.restoreAllMocks();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  describe('en/my/th key parity', () => {
    it('covers all highlights in all supported languages', () => {
      expect(RELEASES.length).toBeGreaterThan(0);
      for (const release of RELEASES) {
        expect(release.highlights.length).toBeGreaterThanOrEqual(3);
        expect(release.highlights.length).toBeLessThanOrEqual(6);

        for (const h of release.highlights) {
          expect(h.title.en, `highlight ${h.id} missing title.en`).toBeTruthy();
          expect(h.title.my, `highlight ${h.id} missing title.my`).toBeTruthy();
          expect(h.title.th, `highlight ${h.id} missing title.th`).toBeTruthy();

          expect(h.description.en, `highlight ${h.id} missing description.en`).toBeTruthy();
          expect(h.description.my, `highlight ${h.id} missing description.my`).toBeTruthy();
          expect(h.description.th, `highlight ${h.id} missing description.th`).toBeTruthy();

          if (h.showMe?.label) {
            expect(h.showMe.label.en).toBeTruthy();
            expect(h.showMe.label.my).toBeTruthy();
            expect(h.showMe.label.th).toBeTruthy();
          }
        }
      }
    });

    it('covers all whatsNew, about, and cmd i18n keys in en, my, and th', () => {
      const keys = [
        'about.whatsNew',
        'about.whatsNewDesc',
        'about.viewWhatsNew',
        'cmd.whatsNew',
        'whatsNew.title',
        'whatsNew.releaseNotes',
        'whatsNew.showMe',
        'whatsNew.gotIt',
        'whatsNew.noHighlights',
      ] as const;

      for (const k of keys) {
        expect(dictionaries.en[k], `missing en key: ${k}`).toBeTruthy();
        expect(dictionaries.my[k], `missing my key: ${k}`).toBeTruthy();
        expect(dictionaries.th[k], `missing th key: ${k}`).toBeTruthy();
      }
    });
  });

  describe('startup behavior', () => {
    it('shows sheet on first launch after upgrade and records version', async () => {
      vi.spyOn(updater, 'getAppVersion').mockResolvedValue('0.6.5');
      const saveSpy = vi.spyOn(api, 'setSettings').mockImplementation(async (a) => a.settings);

      useSettings.setState({
        settings: { ...baseSettings, lastSeenVersion: '0.6.4' },
      });

      render(<StartupTestComponent />);

      await waitFor(() => {
        expect(saveSpy).toHaveBeenCalledWith({
          settings: expect.objectContaining({ lastSeenVersion: '0.6.5' }),
        });
        expect(useApp.getState().modal).toEqual({
          id: 'whats-new',
          props: { version: '0.6.5' },
        });
      });
    });

    it('hides sheet on fresh install and records version silently', async () => {
      vi.spyOn(updater, 'getAppVersion').mockResolvedValue('0.6.5');
      const saveSpy = vi.spyOn(api, 'setSettings').mockImplementation(async (a) => a.settings);

      useSettings.setState({
        settings: { ...baseSettings, lastSeenVersion: undefined },
      });

      render(<StartupTestComponent />);

      await waitFor(() => {
        expect(saveSpy).toHaveBeenCalledWith({
          settings: expect.objectContaining({ lastSeenVersion: '0.6.5' }),
        });
      });

      expect(useApp.getState().modal).toBeNull();
    });

    it('hides sheet when on same version', async () => {
      vi.spyOn(updater, 'getAppVersion').mockResolvedValue('0.6.5');
      const saveSpy = vi.spyOn(api, 'setSettings').mockImplementation(async (a) => a.settings);

      useSettings.setState({
        settings: { ...baseSettings, lastSeenVersion: '0.6.5' },
      });

      render(<StartupTestComponent />);

      await new Promise((r) => setTimeout(r, 20));

      expect(saveSpy).not.toHaveBeenCalled();
      expect(useApp.getState().modal).toBeNull();
    });

    it('hides sheet on downgrade', async () => {
      vi.spyOn(updater, 'getAppVersion').mockResolvedValue('0.6.4');
      const saveSpy = vi.spyOn(api, 'setSettings').mockImplementation(async (a) => a.settings);

      useSettings.setState({
        settings: { ...baseSettings, lastSeenVersion: '0.6.5' },
      });

      render(<StartupTestComponent />);

      await new Promise((r) => setTimeout(r, 20));

      expect(saveSpy).not.toHaveBeenCalled();
      expect(useApp.getState().modal).toBeNull();
    });
  });

  describe('palette entry and about tab', () => {
    it('palette command opens the whats-new modal', () => {
      const cmds = builtinCommands();
      const cmd = cmds.find((c) => c.id === 'whats-new');
      expect(cmd).toBeDefined();

      cmd?.run();
      expect(useApp.getState().modal?.id).toBe('whats-new');
    });

    it('whatsNewFeature registry definition provides the command and modal', () => {
      expect(whatsNewFeature.id).toBe('whats-new');
      expect(whatsNewFeature.Modal).toBe(WhatsNewModal);
      const cmd = whatsNewFeature.commands?.find((c) => c.id === 'whats-new');
      expect(cmd).toBeDefined();

      cmd?.run();
      expect(useApp.getState().modal?.id).toBe('whats-new');
    });

    it('about tab button opens the whats-new modal', async () => {
      const u = userEvent.setup();
      render(<AboutTab />);

      const btn = await screen.findByRole('button', { name: "View what's new" });
      await u.click(btn);

      expect(useApp.getState().modal?.id).toBe('whats-new');
    });
  });

  describe('WhatsNewModal UI and interactions', () => {
    it('renders highlights, release notes link, and closes on Got it click', async () => {
      const u = userEvent.setup();
      useApp.setState({ modal: { id: 'whats-new', props: { version: '0.6.5' } } });

      render(<WhatsNewModal />);

      expect(screen.getByText("What's new in gitpass 0.6.5")).toBeInTheDocument();
      const releaseLink = screen.getByRole('link', { name: /view full release notes/i });
      expect(releaseLink).toHaveAttribute('href', 'https://github.com/itsd0/gitpass/releases/tag/v0.6.5');
      expect(releaseLink).toHaveAttribute('target', '_blank');

      // Check highlights
      expect(screen.getByText('Safe Markdown Notes')).toBeInTheDocument();
      expect(screen.getByText('Screen Recording Protection')).toBeInTheDocument();
      expect(screen.getByText('Watchtower Password Health')).toBeInTheDocument();
      expect(screen.getByText('Collapsible Sidebar Rail')).toBeInTheDocument();
      expect(screen.getByText('Website Favicons')).toBeInTheDocument();

      // Got it button closes modal
      const gotItBtn = screen.getByTestId('whats-new-got-it');
      await u.click(gotItBtn);

      expect(useApp.getState().modal).toBeNull();
    });

    it('Show me button navigates to the feature and closes modal', async () => {
      const u = userEvent.setup();
      useApp.setState({
        modal: { id: 'whats-new', props: { version: '0.6.5' } },
        route: { name: 'home' },
      });

      render(<WhatsNewModal />);

      const showMeMarkdown = screen.getByTestId('whats-new-show-me-markdown-notes');
      await u.click(showMeMarkdown);

      expect(useApp.getState().modal).toBeNull();
      expect(useApp.getState().route).toEqual({ name: 'settings', tab: 'general' });
    });

    it('Show me for watchtower navigates to watchtower', async () => {
      const u = userEvent.setup();
      useApp.setState({
        modal: { id: 'whats-new', props: { version: '0.6.5' } },
        route: { name: 'home' },
      });

      render(<WhatsNewModal />);

      const showMeWatchtower = screen.getByTestId('whats-new-show-me-watchtower-enhancements');
      await u.click(showMeWatchtower);

      expect(useApp.getState().modal).toBeNull();
      expect(useApp.getState().route).toEqual({ name: 'watchtower' });
    });

    it('closes on Escape key press', () => {
      useApp.setState({ modal: { id: 'whats-new', props: { version: '0.6.5' } } });

      render(<WhatsNewModal />);

      fireEvent.keyDown(screen.getByTestId('whats-new-highlights'), { key: 'Escape' });
      expect(useApp.getState().modal).toBeNull();
    });

    it('closes on Enter key press when not activating a button or link', () => {
      useApp.setState({ modal: { id: 'whats-new', props: { version: '0.6.5' } } });

      render(<WhatsNewModal />);

      fireEvent.keyDown(screen.getByTestId('whats-new-highlights'), { key: 'Enter' });
      expect(useApp.getState().modal).toBeNull();
    });

    it('renders in Burmese when language is set to my', () => {
      useI18n.setState({ lang: 'my' });
      useApp.setState({ modal: { id: 'whats-new', props: { version: '0.6.5' } } });

      render(<WhatsNewModal />);

      expect(screen.getByText('gitpass 0.6.5 တွင် အသစ်ပါဝင်မှုများ')).toBeInTheDocument();
      expect(screen.getByText('ထုတ်ပြန်ချက် မှတ်စုအပြည့်အစုံကို ကြည့်ပါ')).toBeInTheDocument();
      expect(screen.getByText('လုံခြုံသော Markdown မှတ်စုများ')).toBeInTheDocument();
      expect(screen.getByText('နားလည်ပါပြီ')).toBeInTheDocument();
    });

    it('renders in Thai when language is set to th', () => {
      useI18n.setState({ lang: 'th' });
      useApp.setState({ modal: { id: 'whats-new', props: { version: '0.6.5' } } });

      render(<WhatsNewModal />);

      expect(screen.getByText('มีอะไรใหม่ใน gitpass 0.6.5')).toBeInTheDocument();
      expect(screen.getByText('ดูบันทึกประจำรุ่นฉบับเต็ม')).toBeInTheDocument();
      expect(screen.getByText('บันทึก Markdown ปลอดภัย')).toBeInTheDocument();
      expect(screen.getByText('รับทราบ')).toBeInTheDocument();
    });
  });
});
