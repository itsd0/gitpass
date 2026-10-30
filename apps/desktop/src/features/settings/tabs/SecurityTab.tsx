import { useState } from 'react';
import { cn } from '../../../lib/utils';
import { Button, Skeleton, toast } from '../../../ui';
import { useT } from '../../../i18n';
import { useSettings } from '../useSettings';
import { SettingsSection, SettingsRow } from '../SettingsSection';
import { ChangePasswordDialog } from '../dialogs/ChangePasswordDialog';
import { RotateKeyDialog } from '../dialogs/RotateKeyDialog';
import { errMsg, NumberSetting } from '../shared';

export function SecurityTab({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const { settings, save } = useSettings();
  const [bioErr, setBioErr] = useState('');
  const [pw, setPw] = useState(false);
  const [rot, setRot] = useState(false);
  if (!settings) return <Skeleton className="h-40" />;
  const s = (p: Parameters<typeof save>[0]) =>
    save(p)
      .then(() => toast(t('common.saved')))
      .catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger'));
  return (
    <>
      <SettingsSection title={t('settings.sectionLocking')}>
        <NumberSetting
          id="setting-autolock"
          highlighted={highlightedId === 'setting-autolock'}
          label={t('settings.autoLock')}
          description={t('settings.autoLockHelp')}
          value={settings.autoLockMinutes}
          min={1}
          max={1440}
          onSave={(autoLockMinutes) => s({ autoLockMinutes })}
        />
        <SettingsRow
          id="setting-lock-sleep"
          highlighted={highlightedId === 'setting-lock-sleep'}
          label={t('settings.lockOnSleep')}
          description={t('settings.lockOnSleepHelp')}
        >
          <button
            role="switch"
            aria-checked={settings.lockOnSleep ?? true}
            aria-label={t('settings.lockOnSleep')}
            data-testid="lock-sleep-switch"
            onClick={() => s({ lockOnSleep: !(settings.lockOnSleep ?? true) })}
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              (settings.lockOnSleep ?? true) ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                (settings.lockOnSleep ?? true) ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
        <SettingsRow
          id="setting-lock-screen"
          highlighted={highlightedId === 'setting-lock-screen'}
          label={t('settings.lockOnScreenLock')}
          description={t('settings.lockOnScreenLockHelp')}
        >
          <button
            role="switch"
            aria-checked={settings.lockOnScreenLock ?? true}
            aria-label={t('settings.lockOnScreenLock')}
            data-testid="lock-screen-switch"
            onClick={() => s({ lockOnScreenLock: !(settings.lockOnScreenLock ?? true) })}
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              (settings.lockOnScreenLock ?? true) ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                (settings.lockOnScreenLock ?? true) ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
        <SettingsRow
          id="setting-lock-user-switch"
          highlighted={highlightedId === 'setting-lock-user-switch'}
          label={t('settings.lockOnUserSwitch')}
          description={t('settings.lockOnUserSwitchHelp')}
        >
          <button
            role="switch"
            aria-checked={settings.lockOnUserSwitch ?? true}
            aria-label={t('settings.lockOnUserSwitch')}
            data-testid="lock-user-switch-switch"
            onClick={() => s({ lockOnUserSwitch: !(settings.lockOnUserSwitch ?? true) })}
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              (settings.lockOnUserSwitch ?? true) ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                (settings.lockOnUserSwitch ?? true) ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
        <SettingsRow
          id="setting-hide-screen-recording"
          highlighted={highlightedId === 'setting-hide-screen-recording'}
          label={t('settings.hideFromScreenRecording')}
          description={t('settings.hideFromScreenRecordingHelp')}
        >
          <button
            role="switch"
            aria-checked={settings.hideFromScreenRecording ?? true}
            aria-label={t('settings.hideFromScreenRecording')}
            data-testid="hide-screen-recording-switch"
            onClick={() => s({ hideFromScreenRecording: !(settings.hideFromScreenRecording ?? true) })}
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              (settings.hideFromScreenRecording ?? true) ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                (settings.hideFromScreenRecording ?? true) ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
        <SettingsRow
          id="setting-remember-generated-passwords"
          highlighted={highlightedId === 'setting-remember-generated-passwords'}
          label={t('settings.rememberGeneratedPasswords')}
          description={t('settings.rememberGeneratedPasswordsHelp')}
        >
          <button
            role="switch"
            aria-checked={settings.rememberGeneratedPasswords ?? true}
            aria-label={t('settings.rememberGeneratedPasswords')}
            data-testid="remember-generated-passwords-switch"
            onClick={() => s({ rememberGeneratedPasswords: !(settings.rememberGeneratedPasswords ?? true) })}
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              (settings.rememberGeneratedPasswords ?? true) ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                (settings.rememberGeneratedPasswords ?? true) ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
        <NumberSetting
          id="setting-clipboard"
          highlighted={highlightedId === 'setting-clipboard'}
          label={t('settings.clipboard')}
          description={t('settings.clipboardHelp')}
          value={settings.clipboardClearSecs}
          min={5}
          max={600}
          onSave={(clipboardClearSecs) => s({ clipboardClearSecs })}
        />
      </SettingsSection>
      <SettingsSection title={t('settings.sectionAuth')}>
        <SettingsRow
          id="setting-biometric"
          highlighted={highlightedId === 'setting-biometric'}
          label={t('settings.biometric')}
          description={
            bioErr ? (
              <span role="status" className="text-warn">
                {/not supported|unsupported/i.test(bioErr)
                  ? t('settings.biometricUnsupported')
                  : bioErr}
              </span>
            ) : (
              t('settings.biometricHelp')
            )
          }
        >
          <button
            role="switch"
            aria-checked={settings.biometricUnlock}
            aria-label={t('settings.biometric')}
            onClick={async () => {
              try {
                await save({ biometricUnlock: !settings.biometricUnlock });
                setBioErr('');
              } catch (e) {
                setBioErr(errMsg(e) || 'error');
              }
            }}
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              settings.biometricUnlock ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                settings.biometricUnlock ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
        <SettingsRow
          id="setting-hibp"
          highlighted={highlightedId === 'setting-hibp'}
          label={t('settings.hibp')}
          description={t('settings.hibpHelp')}
        >
          <button
            role="switch"
            aria-checked={settings.hibpCheck}
            aria-label={t('settings.hibp')}
            data-testid="hibp-switch"
            onClick={() => s({ hibpCheck: !settings.hibpCheck })}
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              settings.hibpCheck ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                settings.hibpCheck ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
      </SettingsSection>
      <SettingsSection title={t('settings.sectionCredentials')}>
        <SettingsRow
          id="setting-password"
          highlighted={highlightedId === 'setting-password'}
          label={t('settings.changePassword')}
          description={t('settings.changePasswordHelp')}
        >
          <Button variant="outline" size="sm" onClick={() => setPw(true)}>
            {t('settings.changePassword')}
          </Button>
        </SettingsRow>
        <SettingsRow
          id="setting-rotate-key"
          highlighted={highlightedId === 'setting-rotate-key'}
          label={t('settings.rotateKey')}
          description={t('settings.rotateKeyHelp')}
        >
          <Button size="sm" variant="outline" onClick={() => setRot(true)}>
            {t('settings.rotateKey')}
          </Button>
        </SettingsRow>
      </SettingsSection>
      <ChangePasswordDialog open={pw} onOpenChange={setPw} />
      <RotateKeyDialog open={rot} onOpenChange={setRot} />
    </>
  );
}

