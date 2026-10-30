import { useEffect, useState } from 'react';
import { Copy } from 'lucide-react';
import { api } from '../../../api/client';
import { SshActivityEntry, SshAgentInfo } from '../../../api/types';
import { cn } from '../../../lib/utils';
import { Badge, Button, Skeleton, toast } from '../../../ui';
import { useT } from '../../../i18n';
import { useSettings } from '../useSettings';
import { SettingsSection, SettingsRow } from '../SettingsSection';
import { errMsg } from '../shared';

const SSH_RESULT_KEY = { signed: 'ssh.resultSigned', denied: 'ssh.resultDenied', refused: 'ssh.resultRefused' } as const;
const SSH_RESULT_TONE = { signed: 'accent', denied: 'danger', refused: 'default' } as const;

function sshRelativeTime(iso: string, t: ReturnType<typeof useT>): string {
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 60) return t('ssh.timeNow');
  const m = Math.floor(s / 60);
  if (m < 60) return t('ssh.timeMin', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('ssh.timeHour', { n: h });
  return t('ssh.timeDay', { n: Math.floor(h / 24) });
}

function sshClientLabel(a: SshActivityEntry, t: ReturnType<typeof useT>): string {
  if (!a.client) return a.pid ? `${t('ssh.activityUnknownClient')} (${a.pid})` : t('ssh.activityUnknownClient');
  return a.pid ? `${a.client} (${a.pid})` : a.client;
}

export function SshAgentTab({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const { settings, save } = useSettings();
  const [info, setInfo] = useState<SshAgentInfo | null>(null);
  const [err, setErr] = useState('');
  const [activity, setActivity] = useState<SshActivityEntry[] | null>(null);
  const [actErr, setActErr] = useState('');
  useEffect(() => {
    api
      .sshAgentInfo()
      .then(setInfo)
      .catch((e) => setErr(errMsg(e)));
  }, []);
  useEffect(() => {
    api
      .sshAgentActivity()
      .then(setActivity)
      .catch((e) => {
        setActivity([]);
        setActErr(errMsg(e));
      });
  }, []);
  const clearActivity = () =>
    api
      .clearSshAgentActivity()
      .then(() => {
        setActivity([]);
        toast(t('ssh.activityCleared'));
      })
      .catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger'));
  if (!settings) return <Skeleton className="h-24" />;
  const on = !!settings.sshApproveEachUse;
  return (
    <>
      {err && (
        <p role="alert" className="py-3 text-sm text-danger">
          {t('common.error', { msg: err })}
        </p>
      )}
      <SettingsSection title={t('settings.sectionSsh')}>
        <SettingsRow
          id="setting-ssh-status"
          highlighted={highlightedId === 'setting-ssh-status'}
          label={t('ssh.status')}
          description={info?.socketPath}
        >
          {info ? (
            <Badge tone={info.running ? 'ok' : 'default'}>
              {info.running ? t('ssh.running') : t('ssh.stopped')}
            </Badge>
          ) : (
            <Skeleton className="h-5 w-16" />
          )}
        </SettingsRow>
        <SettingsRow
          id="setting-ssh-config"
          highlighted={highlightedId === 'setting-ssh-config'}
          label={t('ssh.config')}
          description={t('ssh.configHelp')}
        >
          <div className="flex max-w-sm items-center gap-1.5">
            <pre
              data-testid="ssh-config-line"
              className="overflow-x-auto rounded-md border border-border bg-panel p-2 font-mono text-xs"
            >
              {info?.configLine ?? ''}
            </pre>
            <Button
              size="icon"
              variant="ghost"
              aria-label={t('ssh.copy')}
              disabled={!info}
              onClick={() =>
                info &&
                navigator.clipboard?.writeText(info.configLine).then(() => toast(t('ssh.copied')))
              }
            >
              <Copy className="h-4 w-4" />
            </Button>
          </div>
        </SettingsRow>
        <SettingsRow
          id="setting-ssh-approve"
          highlighted={highlightedId === 'setting-ssh-approve'}
          label={t('ssh.approve')}
          description={t('ssh.approveHelp')}
        >
          <button
            role="switch"
            aria-checked={on}
            aria-label={t('ssh.approve')}
            data-testid="ssh-approve-switch"
            onClick={() =>
              save({ sshApproveEachUse: !on })
                .then(() => toast(t('common.saved')))
                .catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger'))
            }
            className={cn(
              'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              on ? 'bg-accent' : 'bg-border'
            )}
          >
            <span
              className={cn(
                'absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform',
                on ? 'translate-x-5' : 'translate-x-0.5'
              )}
            />
          </button>
        </SettingsRow>
        <SettingsRow
          id="setting-ssh-cli"
          highlighted={highlightedId === 'setting-ssh-cli'}
          label={t('ssh.cli')}
          description={t('ssh.cliHelp')}
        >
          <code className="font-mono text-xs">gitpass ssh config</code>
        </SettingsRow>
      </SettingsSection>
      <SettingsSection
        id="setting-ssh-activity"
        title={t('ssh.activity')}
        className={cn(highlightedId === 'setting-ssh-activity' && 'rounded-lg bg-accent/15 ring-2 ring-accent')}
      >
        <div className="flex items-start justify-between gap-4 py-3.5">
          <p className="min-w-0 text-xs leading-relaxed text-muted">{t('ssh.activityHelp')}</p>
          <Button
            size="sm"
            variant="ghost"
            data-testid="ssh-activity-clear"
            disabled={!activity?.length}
            onClick={clearActivity}
          >
            {t('ssh.activityClear')}
          </Button>
        </div>
        {actErr && (
          <p role="alert" className="pb-3 text-sm text-danger">
            {t('common.error', { msg: actErr })}
          </p>
        )}
        {activity === null ? (
          <Skeleton className="my-3 h-12" />
        ) : activity.length === 0 ? (
          <p data-testid="ssh-activity-empty" className="py-4 text-sm text-muted">
            {t('ssh.activityEmpty')}
          </p>
        ) : (
          <ul
            tabIndex={0}
            aria-label={t('ssh.activity')}
            data-testid="ssh-activity-list"
            className="max-h-64 divide-y divide-border overflow-y-auto rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          >
            {activity.map((a, i) => (
              <li
                key={`${a.time}-${i}`}
                data-testid="ssh-activity-row"
                className="flex items-start justify-between gap-4 py-2.5"
              >
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-fg">
                    {a.keyTitle || t('ssh.activityUnknownKey')}
                  </div>
                  <div className="mt-0.5 truncate font-mono text-xs text-muted">
                    {a.algorithm}
                    {a.fingerprint ? ` · ${a.fingerprint}` : ''}
                  </div>
                  <div className="mt-0.5 truncate text-xs text-muted">
                    {sshClientLabel(a, t)} · {sshRelativeTime(a.time, t)}
                  </div>
                </div>
                <Badge tone={SSH_RESULT_TONE[a.result]}>{t(SSH_RESULT_KEY[a.result])}</Badge>
              </li>
            ))}
          </ul>
        )}
      </SettingsSection>
    </>
  );
}

