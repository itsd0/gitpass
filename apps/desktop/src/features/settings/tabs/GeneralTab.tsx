import { ShortcutRecorder } from '../ShortcutRecorder';
import { cn } from '../../../lib/utils';
import { Skeleton, toast } from '../../../ui';
import { languages, useT, type Lang } from '../../../i18n';
import { useSettings } from '../useSettings';
import { useOnboarding } from '../../onboarding/useOnboarding';
import { TEXT_SIZES, useTextSize, type TextSize } from '../textSize';
import { SettingsSection, SettingsRow } from '../SettingsSection';
import { errMsg, Seg } from '../shared';

export function GeneralTab({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const { settings, save } = useSettings();
  const { onboarding, patch } = useOnboarding();
  const textSize = useTextSize((x) => x.size); const setTextSize = useTextSize((x) => x.set);
  if (!settings) return <Skeleton className="h-24" />;
  const s = (p: Parameters<typeof save>[0]) =>
    save(p)
      .then(() => toast(t('common.saved')))
      .catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger'));
  return (
    <SettingsSection title={t('settings.sectionAppearance')}>
      <SettingsRow
        id="setting-theme"
        highlighted={highlightedId === 'setting-theme'}
        label={t('settings.theme')}
        description={t('settings.themeHelp')}
      >
        <Seg
          label={t('settings.theme')}
          value={settings.theme}
          onChange={(theme) => s({ theme })}
          options={(['dark', 'light', 'system'] as const).map((v) => ({
            v,
            l: t(`settings.theme.${v}`),
          }))}
        />
      </SettingsRow>
      <SettingsRow
        id="setting-text-size"
        highlighted={highlightedId === 'setting-text-size'}
        label={t('settings.textSize')}
        description={t('settings.textSizeHelp')}
      >
        <div className="flex flex-col items-end gap-2">
          <Seg<TextSize>
            label={t('settings.textSize')}
            value={textSize}
            onChange={setTextSize}
            options={TEXT_SIZES.map((v) => ({ v, l: t(`settings.textSize.${v}`) }))}
          />
          <p className="text-sm text-muted" data-testid="text-size-preview">{t('settings.textSizePreview')}</p>
        </div>
      </SettingsRow>
      <SettingsRow
        id="setting-language"
        highlighted={highlightedId === 'setting-language'}
        label={t('settings.language')}
        description={t('settings.languageHelp')}
      >
        <Seg<Lang>
          label={t('settings.language')}
          value={settings.language}
          onChange={(language) => s({ language })}
          options={languages.map((l) => ({ v: l.id, l: l.native }))}
        />
      </SettingsRow>
      <SettingsRow
        id="setting-show-website-icons"
        highlighted={highlightedId === 'setting-show-website-icons'}
        label={t('settings.showWebsiteIcons')}
        description={t('settings.showWebsiteIconsHelp')}
      >
        <button
          type="button"
          role="switch"
          aria-checked={settings.showWebsiteIcons ?? false}
          aria-label={t('settings.showWebsiteIcons')}
          data-testid="show-website-icons-switch"
          onClick={() => s({ showWebsiteIcons: !(settings.showWebsiteIcons ?? false) })}
          className={cn(
            'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
            (settings.showWebsiteIcons ?? false) ? 'bg-accent' : 'bg-border'
          )}
        >
          <span
            className={cn(
              'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
              (settings.showWebsiteIcons ?? false) ? 'translate-x-5' : 'translate-x-0.5'
            )}
          />
        </button>
      </SettingsRow>
      <SettingsRow
        id="setting-render-notes-markdown"
        highlighted={highlightedId === 'setting-render-notes-markdown'}
        label={t('settings.renderNotesMarkdown')}
        description={t('settings.renderNotesMarkdownHelp')}
      >
        <button
          type="button"
          role="switch"
          aria-checked={settings.renderNotesMarkdown ?? true}
          aria-label={t('settings.renderNotesMarkdown')}
          data-testid="render-notes-markdown-switch"
          onClick={() => s({ renderNotesMarkdown: !(settings.renderNotesMarkdown ?? true) })}
          className={cn(
            'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
            (settings.renderNotesMarkdown ?? true) ? 'bg-accent' : 'bg-border'
          )}
        >
          <span
            className={cn(
              'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
              (settings.renderNotesMarkdown ?? true) ? 'translate-x-5' : 'translate-x-0.5'
            )}
          />
        </button>
      </SettingsRow>
      <SettingsRow
        id="setting-show-menu-bar-icon"
        highlighted={highlightedId === 'setting-show-menu-bar-icon'}
        label={t('settings.showMenuBarIcon')}
        description={t('settings.showMenuBarIconHelp')}
      >
        <button
          type="button"
          role="switch"
          aria-checked={settings.showMenuBarIcon ?? true}
          aria-label={t('settings.showMenuBarIcon')}
          data-testid="show-menu-bar-icon-switch"
          onClick={() => s({ showMenuBarIcon: !(settings.showMenuBarIcon ?? true) })}
          className={cn(
            'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
            (settings.showMenuBarIcon ?? true) ? 'bg-accent' : 'bg-border'
          )}
        >
          <span
            className={cn(
              'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
              (settings.showMenuBarIcon ?? true) ? 'translate-x-5' : 'translate-x-0.5'
            )}
          />
        </button>
      </SettingsRow>
      <SettingsRow
        id="setting-quick-access-hotkey"
        highlighted={highlightedId === 'setting-quick-access-hotkey'}
        label={t('settings.quickAccessHotkey')}
        description={t('settings.quickAccessHotkeyHelp')}
      >
        <ShortcutRecorder
          id="setting-quick-access-hotkey"
          value={settings.quickAccessHotkey}
          onChange={async (quickAccessHotkey) => {
            await save({ quickAccessHotkey });
            toast(t('common.saved'));
          }}
        />
      </SettingsRow>
      <SettingsRow
        id="setting-setup-guide"
        highlighted={highlightedId === 'setting-setup-guide'}
        label={t('settings.showSetupGuide')}
        description={t('settings.showSetupGuideHelp')}
      >
        <button
          type="button"
          role="switch"
          aria-checked={!onboarding?.dismissed}
          aria-label={t('settings.showSetupGuide')}
          data-testid="setup-guide-switch"
          onClick={async () => {
            const nextDismissed = !onboarding?.dismissed;
            try {
              await patch({ dismissed: nextDismissed });
              toast(t(nextDismissed ? 'onboarding.dismissedToast' : 'onboarding.restoredToast'));
            } catch (e) {
              toast(String((e as Error)?.message ?? e), 'danger');
            }
          }}
          className={cn(
            'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
            !onboarding?.dismissed ? 'bg-accent' : 'bg-border'
          )}
        >
          <span
            className={cn(
              'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
              !onboarding?.dismissed ? 'translate-x-5' : 'translate-x-0.5'
            )}
          />
        </button>
      </SettingsRow>
    </SettingsSection>
  );
}

