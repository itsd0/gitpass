import { ShortcutRecorder } from './ShortcutRecorder';
import { Logo } from '../../ui/logo';
import { EmergencyKitFlow } from '../onboarding/EmergencyKit';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Archive as VaultsTabIcon,
  Copy,
  Download,
  ExternalLink,
  Pencil,
  Trash2,
  Laptop,
  Lock,
  Info,
  RefreshCw,
  Search,
  SlidersHorizontal,
  Smartphone,
  Terminal,
} from 'lucide-react';
import { api } from '../../api/client';
import type { DeviceInfo, SshAgentInfo, VaultInfo } from '../../api/types';
import { cn } from '../../lib/utils';
import { Badge, Button, Modal, Input, Skeleton, toast } from '../../ui';
import { languages, useT, type Lang } from '../../i18n';
import { useApp } from '../../lib/store';
import { useSettings } from './useSettings';
import { useOnboarding } from '../onboarding/useOnboarding';
import { SyncPanel } from './SyncPanel';
import { UpdateRow } from './UpdateRow';
import { getAppVersion } from '../../lib/updater';
import { VaultIcon } from '../home/vaultIcons';
import { DeleteVaultDialog, EditVaultDialog } from '../home/VaultDialogs';
import { ExportDialog } from '../export';
import { SettingsSection, SettingsRow } from './SettingsSection';
import { searchSettings, type SearchResult } from './searchIndex';

const errMsg = (e: unknown) => String((e as Error)?.message ?? e);
/** Build-time fallback; About shows the running app's version from getAppVersion(). */
export const APP_VERSION = __APP_VERSION__;
export const REPO_URL = 'https://github.com/itsd0/gitpass';
export const ISSUES_URL = `${REPO_URL}/issues`;

const TABS = [
  { id: 'general', icon: SlidersHorizontal },
  { id: 'security', icon: Lock },
  { id: 'vaults', icon: VaultsTabIcon },
  { id: 'devices', icon: Laptop },
  { id: 'sync', icon: RefreshCw },
  { id: 'ssh', icon: Terminal },
  { id: 'about', icon: Info },
] as const;

type Tab = (typeof TABS)[number]['id'];

const Seg = <V extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: V;
  options: { v: V; l: string }[];
  onChange: (v: V) => void;
  label: string;
}) => (
  <div role="radiogroup" aria-label={label} className="flex rounded-lg border border-border p-0.5">
    {options.map((o) => (
      <button
        key={o.v}
        role="radio"
        aria-checked={value === o.v}
        onClick={() => onChange(o.v)}
        className={cn(
          'rounded-md px-3 py-1 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
          value === o.v ? 'bg-accent text-white' : 'text-muted hover:text-fg'
        )}
      >
        {o.l}
      </button>
    ))}
  </div>
);

function General({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const { settings, save } = useSettings();
  const { onboarding, patch } = useOnboarding();
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

function NumberSetting({
  id,
  highlighted,
  label,
  description,
  value,
  min,
  max,
  onSave,
}: {
  id?: string;
  highlighted?: boolean;
  label: string;
  description?: string;
  value: number;
  min: number;
  max: number;
  onSave: (n: number) => void;
}) {
  const [v, setV] = useState(String(value));
  useEffect(() => setV(String(value)), [value]);
  const n = Number(v);
  const ok = Number.isInteger(n) && n >= min && n <= max;
  return (
    <SettingsRow id={id} highlighted={highlighted} label={label} description={description}>
      <Input
        aria-label={label}
        type="number"
        min={min}
        max={max}
        value={v}
        className="w-24 focus-visible:ring-accent"
        aria-invalid={!ok}
        onChange={(e) => setV(e.target.value)}
        onBlur={() => ok && n !== value && onSave(n)}
        onKeyDown={(e) => e.key === 'Enter' && ok && n !== value && onSave(n)}
      />
    </SettingsRow>
  );
}

function ChangePassword({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const [cur, setCur] = useState('');
  const [next, setNext] = useState('');
  const [conf, setConf] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const problem =
    next && next.length < 10
      ? t('settings.passwordShort')
      : conf && next !== conf
      ? t('settings.passwordMismatch')
      : null;
  const close = () => {
    setCur('');
    setNext('');
    setConf('');
    setErr(null);
    onOpenChange(false);
  };
  return (
    <Modal
      open={open}
      onOpenChange={(o) => !o && close()}
      title={t('settings.changePassword')}
      footer={
        <>
          <Button variant="outline" onClick={close}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="default"
            disabled={!cur || !next || next !== conf || !!problem || busy}
            onClick={async () => {
              setBusy(true);
              setErr(null);
              try {
                await api.changePassword({ current: cur, next });
                toast(t('settings.passwordChanged'));
                close();
              } catch (e) {
                setErr(errMsg(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Input
          type="password"
          autoComplete="current-password"
          aria-label={t('settings.currentPassword')}
          placeholder={t('settings.currentPassword')}
          value={cur}
          onChange={(e) => setCur(e.target.value)}
        />
        <Input
          type="password"
          autoComplete="new-password"
          aria-label={t('settings.newPassword')}
          placeholder={t('settings.newPassword')}
          value={next}
          onChange={(e) => setNext(e.target.value)}
        />
        <Input
          type="password"
          autoComplete="new-password"
          aria-label={t('settings.confirmPassword')}
          placeholder={t('settings.confirmPassword')}
          value={conf}
          onChange={(e) => setConf(e.target.value)}
        />
        {(problem || err) && (
          <p role="alert" className="text-sm text-danger">
            {problem ?? t('common.error', { msg: err! })}
          </p>
        )}
      </div>
    </Modal>
  );
}

function RotateKey({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const [kit, setKit] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [showKit, setShowKit] = useState(false);
  const close = () => {
    if (kit && !saved) return;
    setKit(null);
    setSaved(false);
    onOpenChange(false);
  };
  return (
    <Modal
      open={open}
      onOpenChange={(o) => !o && close()}
      title={kit ? t('settings.emergencyKit') : t('settings.rotateKey')}
      description={kit ? t('settings.kitWarning') : t('settings.rotateKeyHelp')}
      wide={!!kit}
      footer={
        kit ? (
          <Button variant="default" disabled={!saved} onClick={close}>
            {t('settings.done')}
          </Button>
        ) : (
          <>
            <Button variant="outline" onClick={close}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                try {
                  setKit((await api.rotateSecretKey()).emergencyKit);
                  setShowKit(true);
                } catch (e) {
                  toast(t('common.error', { msg: errMsg(e) }), 'danger');
                } finally {
                  setBusy(false);
                }
              }}
            >
              {t('settings.rotateKeyConfirm')}
            </Button>
          </>
        )
      }
    >
      {kit && (
        <>
          <pre
            className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-bg p-3 font-mono text-xs"
            data-testid="emergency-kit"
          >
            {kit}
          </pre>
          {showKit && <EmergencyKitFlow onClose={() => setShowKit(false)} />}
          <div className="mt-3 flex gap-2">
            <Button variant="default" size="sm" onClick={() => setShowKit(true)}>
              {t('kit.openPrintable')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                navigator.clipboard.writeText(kit).then(() => toast(t('common.copied')))
              }
            >
              {t('common.copy')}
            </Button>
          </div>
          <label className="mt-4 flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="h-4 w-4"
              checked={saved}
              onChange={(e) => setSaved(e.target.checked)}
            />
            {t('settings.kitSaved')}
          </label>
        </>
      )}
    </Modal>
  );
}

function Security({ highlightedId }: { highlightedId?: string | null }) {
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
      <ChangePassword open={pw} onOpenChange={setPw} />
      <RotateKey open={rot} onOpenChange={setRot} />
    </>
  );
}

function Devices({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const [list, setList] = useState<DeviceInfo[] | null>(null);
  const [target, setTarget] = useState<DeviceInfo | null>(null);
  const [rotate, setRotate] = useState(true);
  const [busy, setBusy] = useState(false);
  const load = () =>
    api
      .listDevices()
      .then(setList)
      .catch((e) => {
        setList([]);
        toast(t('common.error', { msg: errMsg(e) }), 'danger');
      });
  useEffect(() => {
    load();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  if (!list) return <Skeleton className="h-24" />;
  if (!list.length) return <p className="py-6 text-sm text-muted">{t('devices.empty')}</p>;
  return (
    <>
      <SettingsSection id="setting-devices" title={t('settings.sectionDevices')} className={cn(highlightedId === 'setting-devices' && 'rounded-lg bg-accent/15 ring-2 ring-accent')}>
        {list.map((d) => (
          <SettingsRow
            key={d.id}
            label={<div className="flex items-center gap-2"><Smartphone className="h-4 w-4 text-muted" aria-hidden /><span>{d.name}</span></div>}
            description={
              <div className="flex flex-col gap-0.5">
                <div className="flex items-center gap-1.5">
                  {d.current && <Badge tone="ok">{t('devices.current')}</Badge>}
                  {d.revoked && <Badge tone="danger">{t('devices.revoked')}</Badge>}
                </div>
                <div>{t('devices.lastSeen', { when: new Date(d.lastSeen).toLocaleString() })}</div>
              </div>
            }
          >
            {!d.current && !d.revoked && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  setRotate(true);
                  setTarget(d);
                }}
              >
                {t('devices.revoke')}
              </Button>
            )}
          </SettingsRow>
        ))}
      </SettingsSection>
      <Modal
        open={!!target}
        onOpenChange={(o) => !o && setTarget(null)}
        title={t('devices.revokeTitle', { name: target?.name ?? '' })}
        description={t('devices.revokeBody')}
        footer={
          <>
            <Button variant="outline" onClick={() => setTarget(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={busy}
              onClick={async () => {
                if (!target) return;
                setBusy(true);
                try {
                  await api.revokeDevice({ id: target.id, rotateCredentials: rotate });
                  toast(t('devices.done'));
                  setTarget(null);
                  load();
                } catch (e) {
                  toast(t('common.error', { msg: errMsg(e) }), 'danger');
                } finally {
                  setBusy(false);
                }
              }}
            >
              {t('devices.revoke')}
            </Button>
          </>
        }
      >
        <label className="flex items-start gap-2 text-sm">
          <input
            type="checkbox"
            className="mt-0.5 h-4 w-4"
            checked={rotate}
            onChange={(e) => setRotate(e.target.checked)}
          />
          {t('devices.rotate')}
        </label>
        <p className="mt-2 text-xs text-muted">{t('devices.rotateHelp')}</p>
      </Modal>
    </>
  );
}

const Sync = SyncPanel;

function SshAgent({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const { settings, save } = useSettings();
  const [info, setInfo] = useState<SshAgentInfo | null>(null);
  const [err, setErr] = useState('');
  useEffect(() => {
    api
      .sshAgentInfo()
      .then(setInfo)
      .catch((e) => setErr(errMsg(e)));
  }, []);
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
    </>
  );
}

function About({ highlightedId }: { highlightedId?: string | null }) {
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
function Vaults({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const [vs, setVs] = useState<VaultInfo[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [editV, setEditV] = useState<VaultInfo | null>(null);
  const [delV, setDelV] = useState<VaultInfo | null>(null);
  const [exportV, setExportV] = useState<VaultInfo | null>(null);
  const reload = () => api.listVaults().then(setVs, (e) => setErr(errMsg(e)));
  useEffect(() => {
    void reload();
  }, []);
  if (err) return <p role="alert" className="py-4 text-sm text-danger">{err}</p>;
  if (!vs) return <Skeleton className="my-4 h-24" />;
  const done = () => {
    void reload();
    useApp.getState().refreshItems();
  };
  return (
    <div data-testid="vaults-settings">
      <SettingsSection
        id="setting-vaults"
        title={t('settings.sectionVaults')}
        description={t('vault.manageHelp')}
        className={cn(highlightedId === 'setting-vaults' && 'rounded-lg bg-accent/15 ring-2 ring-accent')}
      >
        {vs.map((v) => (
          <SettingsRow
            key={v.id}
            label={<div className="flex items-center gap-2"><VaultIcon icon={v.icon} color={v.color} size={20} /><span>{v.name}</span></div>}
            description={`${v.description ? `${v.description} · ` : ''}${t('settings.vaultItems')}: ${v.itemCount}`}
          >
            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                aria-label={t('export.vaultLabel', { vault: v.name })}
                title={t('export.vaultLabel', { vault: v.name })}
                onClick={() => setExportV(v)}
              >
                <Download size={14} />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                aria-label={`${t('vault.edit')} ${v.name}`}
                onClick={() => setEditV(v)}
              >
                <Pencil size={14} />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                aria-label={`${t('vault.delete')} ${v.name}`}
                disabled={vs.length <= 1}
                title={vs.length <= 1 ? t('vault.lastVault') : undefined}
                onClick={() => setDelV(v)}
              >
                <Trash2 size={14} className="text-danger" />
              </Button>
            </div>
          </SettingsRow>
        ))}
      </SettingsSection>
      <EditVaultDialog
        vault={editV}
        onOpenChange={(o) => !o && setEditV(null)}
        onSaved={() => {
          toast(t('vault.saved'));
          done();
        }}
      />
      <DeleteVaultDialog
        vault={delV}
        isLast={vs.length <= 1}
        onOpenChange={(o) => !o && setDelV(null)}
        onDeleted={() => {
          toast(t('vault.deleted', { vault: delV?.name ?? '' }));
          useApp.getState().selectItem(null);
          done();
        }}
      />
      <ExportDialog
        open={!!exportV}
        onOpenChange={(o) => !o && setExportV(null)}
        vaultId={exportV?.id}
        vaultName={exportV?.name}
      />
    </div>
  );
}

/** `settings` route with tab `vault:<id>` (Home → vault card → settings). */
function VaultPanel({ vaultId }: { vaultId: string }) {
  const t = useT();
  const [v, setV] = useState<VaultInfo | null | undefined>(undefined);
  const [count, setCount] = useState(0);
  const [edit, setEdit] = useState(false);
  const [del, setDel] = useState(false);
  const [exp, setExp] = useState(false);
  const reload = () =>
    api
      .listVaults()
      .then((vs) => {
        setCount(vs.length);
        setV(vs.find((x) => x.id === vaultId) ?? null);
      })
      .catch(() => setV(null));
  useEffect(() => {
    void reload();
  }, [vaultId]); // eslint-disable-line react-hooks/exhaustive-deps
  if (v === undefined) return <Skeleton className="h-24" />;
  if (v === null) return <p role="alert" className="py-4 text-sm text-danger">{t('settings.vaultMissing')}</p>;
  const app = useApp.getState();
  return (
    <SettingsSection title={t('settings.vault')}>
      <SettingsRow label={t('settings.vaultName')}>
        <span className="text-sm font-medium">{v.name}</span>
      </SettingsRow>
      {v.description && (
        <SettingsRow label={t('settings.vaultDescription')}>
          <span className="text-sm">{v.description}</span>
        </SettingsRow>
      )}
      <SettingsRow label={t('settings.vaultItems')}>
        <span className="text-sm">{v.itemCount}</span>
      </SettingsRow>
      <SettingsRow label={t('people.vaultMembers')}>
        <span className="text-sm">{v.members.length}</span>
      </SettingsRow>
      <div className="flex gap-2 py-4">
        <Button
          variant="default"
          onClick={() => app.setRoute({ name: 'items', filter: { kind: 'vault', vaultId: v.id } })}
        >
          {t('settings.vaultOpen')}
        </Button>
        <Button variant="outline" onClick={() => app.openModal('people', { vaultId: v.id })}>
          {t('people.invite')}
        </Button>
        <Button variant="outline" onClick={() => setExp(true)}>
          <Download size={14} />
          {t('export.buttonVault')}
        </Button>
        <Button variant="outline" onClick={() => setEdit(true)}>
          <Pencil size={14} />
          {t('vault.edit')}
        </Button>
        <Button
          variant="outline"
          className="text-danger"
          disabled={count <= 1}
          title={count <= 1 ? t('vault.lastVault') : undefined}
          onClick={() => setDel(true)}
        >
          <Trash2 size={14} />
          {t('vault.delete')}
        </Button>
      </div>
      <EditVaultDialog
        vault={edit ? v : null}
        onOpenChange={setEdit}
        onSaved={(nv) => {
          toast(t('vault.saved'));
          setV(nv);
          app.refreshItems();
        }}
      />
      <DeleteVaultDialog
        vault={del ? v : null}
        isLast={count <= 1}
        onOpenChange={setDel}
        onDeleted={() => {
          toast(t('vault.deleted', { vault: v.name }));
          app.selectItem(null);
          app.refreshItems();
          app.setRoute({ name: 'settings', tab: 'vaults' });
        }}
      />
      <ExportDialog open={exp} onOpenChange={setExp} vaultId={v.id} vaultName={v.name} />
    </SettingsSection>
  );
}

export function SettingsPage({ initialTab }: { initialTab?: string } = {}) {
  const t = useT();
  const route = useApp((s) => s.route);
  const setRoute = useApp((s) => s.setRoute);
  const { settings, load, error } = useSettings();
  const rawTab = initialTab ?? (route.name === 'settings' ? route.tab : undefined);
  const vaultId = rawTab?.startsWith('vault:') ? rawTab.slice(6) : null;
  // 'help' (toolbar) → About; unknown → General.
  const fromRoute = rawTab === 'help' ? 'about' : rawTab;
  const tab: Tab = (TABS.some((x) => x.id === fromRoute) ? fromRoute : 'general') as Tab;
  const [local, setLocal] = useState<Tab>(tab);
  useEffect(() => setLocal(tab), [tab]);
  useEffect(() => {
    if (!settings) load();
  }, [settings, load]);

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const results = useMemo(() => searchSettings(searchQuery, t), [searchQuery, t]);

  // Global ⌘F / Ctrl+F shortcut focused when settings is open
  useEffect(() => {
    const onGlobalKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener('keydown', onGlobalKeyDown);
    return () => window.removeEventListener('keydown', onGlobalKeyDown);
  }, []);

  function selectResult(item: SearchResult) {
    setLocal(item.tab);
    setRoute({ name: 'settings', tab: item.tab });
    setHighlightedId(item.id);
    setTimeout(() => {
      const el =
        document.getElementById(item.id) ||
        document.querySelector(`[data-setting-id="${item.id}"]`);
      if (el) {
        const reduceMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches;
        el.scrollIntoView({ behavior: reduceMotion ? 'auto' : 'smooth', block: 'nearest' });
      }
    }, 50);
  }

  // Clear highlight after 1.5s
  useEffect(() => {
    if (highlightedId) {
      const timer = setTimeout(() => setHighlightedId(null), 1500);
      return () => clearTimeout(timer);
    }
  }, [highlightedId]);

  function onSearchKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Escape') {
      e.preventDefault();
      setSearchQuery('');
      setSelectedIndex(0);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((idx) => (results.length > 0 ? Math.min(idx + 1, results.length - 1) : 0));
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((idx) => Math.max(idx - 1, 0));
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (results[selectedIndex]) {
        selectResult(results[selectedIndex]);
      }
      return;
    }
  }

  function onTabListKeyDown(e: React.KeyboardEvent) {
    const curIdx = TABS.findIndex((x) => x.id === local);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const nextIdx = (curIdx + 1) % TABS.length;
      const nextId = TABS[nextIdx].id;
      setLocal(nextId);
      setRoute({ name: 'settings', tab: nextId });
      tabRefs.current[nextIdx]?.focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const prevIdx = (curIdx - 1 + TABS.length) % TABS.length;
      const prevId = TABS[prevIdx].id;
      setLocal(prevId);
      setRoute({ name: 'settings', tab: prevId });
      tabRefs.current[prevIdx]?.focus();
    }
  }

  const body = {
    general: <General highlightedId={highlightedId} />,
    security: <Security highlightedId={highlightedId} />,
    vaults: <Vaults highlightedId={highlightedId} />,
    devices: <Devices highlightedId={highlightedId} />,
    sync: <Sync highlightedId={highlightedId} />,
    ssh: <SshAgent highlightedId={highlightedId} />,
    about: <About highlightedId={highlightedId} />,
  }[local];

  return (
    <div className="mx-auto flex max-w-4xl gap-8 p-8">
      <nav aria-label={t('settings.title')} className="w-52 shrink-0">
        <h1 className="mb-3 text-2xl font-semibold">{t('settings.title')}</h1>
        <div className="relative mb-3">
          <Search
            className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted"
            aria-hidden
          />
          <Input
            ref={searchRef}
            type="search"
            data-testid="settings-search-input"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setSelectedIndex(0);
            }}
            onKeyDown={onSearchKeyDown}
            placeholder={t('settings.searchPlaceholder')}
            aria-label={t('settings.searchAria')}
            className="h-8 pl-8 pr-2 text-xs focus-visible:ring-accent"
          />
        </div>

        {searchQuery.trim() !== '' ? (
          results.length > 0 ? (
            <div
              role="listbox"
              aria-label={t('settings.searchAria')}
              className="space-y-1"
              data-testid="settings-search-results"
            >
              {results.map((r, i) => (
                <button
                  key={r.id}
                  role="option"
                  aria-selected={selectedIndex === i}
                  onClick={() => selectResult(r)}
                  className={cn(
                    'flex w-full flex-col items-start gap-0.5 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
                    selectedIndex === i
                      ? 'bg-hover font-medium ring-1 ring-accent/60'
                      : 'text-muted hover:bg-hover hover:text-fg'
                  )}
                >
                  <span className="font-medium text-fg line-clamp-1">{r.label}</span>
                  <span className="text-[10px] text-muted">{r.tabName}</span>
                </button>
              ))}
            </div>
          ) : (
            <div
              role="status"
              className="flex flex-col items-center justify-center p-4 text-center"
              data-testid="settings-search-empty"
            >
              <p className="mb-3 text-xs text-muted leading-relaxed">
                {t('settings.searchEmpty', { q: searchQuery })}
              </p>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs"
                onClick={() => {
                  setSearchQuery('');
                  searchRef.current?.focus();
                }}
              >
                {t('settings.clearSearch')}
              </Button>
            </div>
          )
        ) : (
          <ul
            role="tablist"
            aria-orientation="vertical"
            className="space-y-1"
            onKeyDown={onTabListKeyDown}
          >
            {TABS.map(({ id, icon: I }, idx) => (
              <li key={id}>
                <button
                  ref={(el) => {
                    tabRefs.current[idx] = el;
                  }}
                  role="tab"
                  aria-selected={!vaultId && local === id}
                  onClick={() => {
                    setLocal(id);
                    setRoute({ name: 'settings', tab: id });
                  }}
                  className={cn(
                    'flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
                    !vaultId && local === id
                      ? 'bg-hover font-medium'
                      : 'text-muted hover:bg-hover hover:text-fg'
                  )}
                >
                  <I className="h-4 w-4" aria-hidden />
                  {t(`settings.${id}`)}
                </button>
              </li>
            ))}
          </ul>
        )}
      </nav>
      <section
        role="tabpanel"
        aria-label={vaultId ? t('settings.vault') : t(`settings.${local}`)}
        className="min-w-0 flex-1 rounded-xl border border-border bg-card px-5 py-4"
      >
        {error && (
          <p role="alert" className="mb-4 py-3 text-sm text-danger">
            {t('common.error', { msg: error })}
          </p>
        )}
        {vaultId ? <VaultPanel vaultId={vaultId} /> : body}
      </section>
    </div>
  );
}
