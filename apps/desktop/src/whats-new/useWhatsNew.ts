import { useEffect } from 'react';
import { getAppVersion } from '../lib/updater';
import { useApp } from '../lib/store';
import { useSettings } from '../features/settings/useSettings';
import { determineWhatsNewAction } from './semver';

let checkedThisSession = false;

export function resetWhatsNewStartupCheckForTesting() {
  checkedThisSession = false;
}

/**
 * Hook mounted inside AppShell (only rendered when unlocked).
 * Checks whether the app version has changed compared to settings.lastSeenVersion.
 *
 * Rules:
 * - Fresh install (no lastSeenVersion): records current version silently, never shows sheet.
 * - Upgrade (runningVersion > lastSeenVersion): shows What's new sheet, records current version.
 * - Same version or downgrade: does nothing.
 * - Never steals focus or brings window to the front.
 */
export function useWhatsNewOnStartup() {
  const settings = useSettings((s) => s.settings);

  useEffect(() => {
    if (checkedThisSession || !settings) return;
    checkedThisSession = true;

    void (async () => {
      try {
        const runningVersion = await getAppVersion();
        const action = determineWhatsNewAction(runningVersion, settings.lastSeenVersion);

        if (action === 'record_silent') {
          // Fresh install: save version silently without opening sheet
          await useSettings.getState().save({ lastSeenVersion: runningVersion });
        } else if (action === 'show') {
          // Upgrade: save version and open sheet
          await useSettings.getState().save({ lastSeenVersion: runningVersion });
          useApp.getState().openModal('whats-new', { version: runningVersion });
        }
      } catch (err) {
        console.error('Failed to perform whats-new startup check', err);
      }
    })();
  }, [settings]);
}
