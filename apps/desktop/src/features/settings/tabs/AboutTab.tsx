import { Logo } from '../../../ui/logo';
import { useEffect, useState } from 'react';
import { ExternalLink } from 'lucide-react';
import { cn } from '../../../lib/utils';
import { useT } from '../../../i18n';
import { Button } from '../../../ui';
import { useApp } from '../../../lib/store';
import { UpdateRow } from '../UpdateRow';
import { getAppVersion } from '../../../lib/updater';
import { SettingsSection, SettingsRow } from '../SettingsSection';

/** Build-time fallback; About shows the running app's version from getAppVersion(). */
export const APP_VERSION = __APP_VERSION__;
export const REPO_URL = 'https://github.com/itsd0/gitpass';
export const ISSUES_URL = `${REPO_URL}/issues`;


export function AboutTab({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const [ver, setVer] = useState(APP_VERSION);
  useEffect(() => {
    let on = true;
    getAppVersion().then((v) => on && setVer(v));
    return () => {
      on = false;
    };
  }, []);
  return (
    <>
      <SettingsSection title={t('settings.sectionApp')}>
        <div className="flex items-center gap-3 py-4">
          <Logo size={48} />
          <div>
            <div className="text-lg font-semibold">gitpass</div>
            <div className="text-sm text-muted">{t('about.license')}</div>
          </div>
        </div>
        <SettingsRow
          id="setting-about-version"
          highlighted={highlightedId === 'setting-about-version'}
          label={t('about.version')}
          description={t('about.license')}
        >
          <span className="font-mono text-sm">{ver}</span>
        </SettingsRow>
        <SettingsRow
          id="setting-about-whats-new"
          highlighted={highlightedId === 'setting-about-whats-new'}
          label={t('about.whatsNew')}
          description={t('about.whatsNewDesc')}
        >
          <Button
            variant="outline"
            size="sm"
            onClick={() => useApp.getState().openModal('whats-new', { version: ver })}
          >
            {t('about.viewWhatsNew')}
          </Button>
        </SettingsRow>
        <div
          id="setting-about-updates"
          data-setting-id="setting-about-updates"
          className={cn(
            'transition-colors duration-300 motion-reduce:transition-none',
            highlightedId === 'setting-about-updates' && 'rounded-md bg-accent/15 px-2 ring-2 ring-accent'
          )}
        >
          <UpdateRow />
        </div>
      </SettingsSection>
      <SettingsSection title={t('settings.sectionCommunity')}>
        <SettingsRow
          id="setting-about-repo"
          highlighted={highlightedId === 'setting-about-repo'}
          label={t('about.repo')}
          description={t('settings.repoHelp')}
        >
          <a
            className="inline-flex items-center gap-1 text-sm text-accent hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            href={REPO_URL}
            target="_blank"
            rel="noreferrer"
          >
            {REPO_URL.replace('https://', '')}
            <ExternalLink className="h-3 w-3" aria-hidden />
          </a>
        </SettingsRow>
        <SettingsRow
          id="setting-about-issues"
          highlighted={highlightedId === 'setting-about-issues'}
          label={t('about.issues')}
          description={t('settings.issuesHelp')}
        >
          <a
            className="text-sm text-accent hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            href={ISSUES_URL}
            target="_blank"
            rel="noreferrer"
          >
            {t('about.issues')}
          </a>
        </SettingsRow>
      </SettingsSection>
    </>
  );
}

/** Settings → Vaults: rename / restyle / delete every vault. */
