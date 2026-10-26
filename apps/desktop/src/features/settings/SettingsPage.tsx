import { Logo } from '../../ui/logo';
import { EmergencyKitFlow } from '../onboarding/EmergencyKit';
import { useEffect, useState } from 'react';
import { Copy, ExternalLink, Laptop, Lock, Info, RefreshCw, SlidersHorizontal, Smartphone, Terminal } from 'lucide-react';
import { api } from '../../api/client';
import type { DeviceInfo, SshAgentInfo, VaultInfo } from '../../api/types';
import { cn } from '../../lib/utils';
import { Badge, Button, Modal, Input, Skeleton, toast } from '../../ui';
import { languages, useT, type Lang } from '../../i18n';
import { useApp } from '../../lib/store';
import { useSettings } from './useSettings';
import { SyncPanel } from './SyncPanel';
import { UpdateRow } from './UpdateRow';
import { getAppVersion } from '../../lib/updater';

const errMsg = (e: unknown) => String((e as Error)?.message ?? e);
/** Build-time fallback; About shows the running app's version from getAppVersion(). */
export const APP_VERSION = __APP_VERSION__;
export const REPO_URL = 'https://github.com/itsd0/gitpass';
export const ISSUES_URL = `${REPO_URL}/issues`;
const TABS = [{ id: 'general', icon: SlidersHorizontal }, { id: 'security', icon: Lock }, { id: 'devices', icon: Laptop }, { id: 'sync', icon: RefreshCw }, { id: 'ssh', icon: Terminal }, { id: 'about', icon: Info }] as const;
type Tab = (typeof TABS)[number]['id'];
const Row = ({ label, children, help }: { label: string; children: React.ReactNode; help?: React.ReactNode }) =>
  <div className="flex items-start justify-between gap-6 border-b border-border py-4 last:border-0"><div><div className="text-sm font-medium">{label}</div>{help && <div className="mt-0.5 text-xs text-muted">{help}</div>}</div><div className="shrink-0">{children}</div></div>;
const Seg = <V extends string>({ value, options, onChange, label }: { value: V; options: { v: V; l: string }[]; onChange: (v: V) => void; label: string }) =>
  <div role="radiogroup" aria-label={label} className="flex rounded-lg border border-border p-0.5">{options.map((o) =>
    <button key={o.v} role="radio" aria-checked={value === o.v} onClick={() => onChange(o.v)} className={cn('rounded-md px-3 py-1 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent', value === o.v ? 'bg-accent text-white' : 'text-muted hover:text-fg')}>{o.l}</button>)}</div>;

function General() {
  const t = useT(); const { settings, save } = useSettings();
  if (!settings) return <Skeleton className="h-24" />;
  const s = (p: Parameters<typeof save>[0]) => save(p).then(() => toast(t('common.saved'))).catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger'));
  return <>
    <Row label={t('settings.theme')}><Seg label={t('settings.theme')} value={settings.theme} onChange={(theme) => s({ theme })} options={(['dark', 'light', 'system'] as const).map((v) => ({ v, l: t(`settings.theme.${v}`) }))} /></Row>
    <Row label={t('settings.language')}><Seg<Lang> label={t('settings.language')} value={settings.language} onChange={(language) => s({ language })} options={languages.map((l) => ({ v: l.id, l: l.native }))} /></Row>
  </>;
}

function NumberSetting({ label, value, min, max, onSave }: { label: string; value: number; min: number; max: number; onSave: (n: number) => void }) {
  const [v, setV] = useState(String(value)); useEffect(() => setV(String(value)), [value]);
  const n = Number(v); const ok = Number.isInteger(n) && n >= min && n <= max;
  return <Row label={label}><Input aria-label={label} type="number" min={min} max={max} value={v} className="w-24" aria-invalid={!ok}
    onChange={(e) => setV(e.target.value)} onBlur={() => ok && n !== value && onSave(n)} onKeyDown={(e) => e.key === 'Enter' && ok && n !== value && onSave(n)} /></Row>;
}

function ChangePassword({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT(); const [cur, setCur] = useState(''); const [next, setNext] = useState(''); const [conf, setConf] = useState(''); const [err, setErr] = useState<string | null>(null); const [busy, setBusy] = useState(false);
  const problem = next && next.length < 10 ? t('settings.passwordShort') : conf && next !== conf ? t('settings.passwordMismatch') : null;
  const close = () => { setCur(''); setNext(''); setConf(''); setErr(null); onOpenChange(false); };
  return <Modal open={open} onOpenChange={(o) => !o && close()} title={t('settings.changePassword')}
    footer={<><Button variant="outline" onClick={close}>{t('common.cancel')}</Button><Button variant="default" disabled={!cur || !next || next !== conf || !!problem || busy} onClick={async () => { setBusy(true); setErr(null);
      try { await api.changePassword({ current: cur, next }); toast(t('settings.passwordChanged')); close(); } catch (e) { setErr(errMsg(e)); } finally { setBusy(false); } }}>{t('common.save')}</Button></>}>
    <div className="space-y-3">
      <Input type="password" autoComplete="current-password" aria-label={t('settings.currentPassword')} placeholder={t('settings.currentPassword')} value={cur} onChange={(e) => setCur(e.target.value)} />
      <Input type="password" autoComplete="new-password" aria-label={t('settings.newPassword')} placeholder={t('settings.newPassword')} value={next} onChange={(e) => setNext(e.target.value)} />
      <Input type="password" autoComplete="new-password" aria-label={t('settings.confirmPassword')} placeholder={t('settings.confirmPassword')} value={conf} onChange={(e) => setConf(e.target.value)} />
      {(problem || err) && <p role="alert" className="text-sm text-danger">{problem ?? t('common.error', { msg: err! })}</p>}
    </div></Modal>;
}

function RotateKey({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT(); const [kit, setKit] = useState<string | null>(null); const [saved, setSaved] = useState(false); const [busy, setBusy] = useState(false); const [showKit, setShowKit] = useState(false);
  const close = () => { if (kit && !saved) return; setKit(null); setSaved(false); onOpenChange(false); };
  return <Modal open={open} onOpenChange={(o) => !o && close()} title={kit ? t('settings.emergencyKit') : t('settings.rotateKey')} description={kit ? t('settings.kitWarning') : t('settings.rotateKeyHelp')} wide={!!kit}
    footer={kit ? <Button variant="default" disabled={!saved} onClick={close}>{t('settings.done')}</Button>
      : <><Button variant="outline" onClick={close}>{t('common.cancel')}</Button><Button variant="danger" disabled={busy} onClick={async () => { setBusy(true);
        try { setKit((await api.rotateSecretKey()).emergencyKit); setShowKit(true); } catch (e) { toast(t('common.error', { msg: errMsg(e) }), 'danger'); } finally { setBusy(false); } }}>{t('settings.rotateKeyConfirm')}</Button></>}>
    {kit && <>
      <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-bg p-3 font-mono text-xs" data-testid="emergency-kit">{kit}</pre>
      {showKit && <EmergencyKitFlow onClose={() => setShowKit(false)} />}
      <div className="mt-3 flex gap-2"><Button variant="default" size="sm" onClick={() => setShowKit(true)}>{t('kit.openPrintable')}</Button>
        <Button variant="outline" size="sm" onClick={() => navigator.clipboard.writeText(kit).then(() => toast(t('common.copied')))}>{t('common.copy')}</Button></div>
      <label className="mt-4 flex items-center gap-2 text-sm"><input type="checkbox" className="h-4 w-4" checked={saved} onChange={(e) => setSaved(e.target.checked)} />{t('settings.kitSaved')}</label></>}
  </Modal>;
}

function Security() {
  const t = useT(); const { settings, save } = useSettings(); const [bioErr, setBioErr] = useState(''); const [pw, setPw] = useState(false); const [rot, setRot] = useState(false);
  if (!settings) return <Skeleton className="h-40" />;
  const s = (p: Parameters<typeof save>[0]) => save(p).then(() => toast(t('common.saved'))).catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger'));
  return <>
    <NumberSetting label={t('settings.autoLock')} value={settings.autoLockMinutes} min={1} max={1440} onSave={(autoLockMinutes) => s({ autoLockMinutes })} />
    <NumberSetting label={t('settings.clipboard')} value={settings.clipboardClearSecs} min={5} max={600} onSave={(clipboardClearSecs) => s({ clipboardClearSecs })} />
    <Row label={t('settings.biometric')} help={bioErr && <span role="status" className="text-warn">{/not supported|unsupported/i.test(bioErr) ? t('settings.biometricUnsupported') : bioErr}</span>}>
      <button role="switch" aria-checked={settings.biometricUnlock} aria-label={t('settings.biometric')} onClick={async () => { try { await save({ biometricUnlock: !settings.biometricUnlock }); setBioErr(''); } catch (e) { setBioErr(errMsg(e) || 'error'); } }}
        className={cn('relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent', settings.biometricUnlock ? 'bg-accent' : 'bg-border')}>
        <span className={cn('absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform', settings.biometricUnlock ? 'translate-x-5' : 'translate-x-0.5')} /></button></Row>
    <Row label={t('settings.hibp')} help={t('settings.hibpHelp')}>
      <button role="switch" aria-checked={settings.hibpCheck} aria-label={t('settings.hibp')} data-testid="hibp-switch" onClick={() => s({ hibpCheck: !settings.hibpCheck })}
        className={cn('relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent', settings.hibpCheck ? 'bg-accent' : 'bg-border')}>
        <span className={cn('absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform', settings.hibpCheck ? 'translate-x-5' : 'translate-x-0.5')} /></button></Row>
    <Row label={t('settings.changePassword')}><Button variant="outline" size="sm" onClick={() => setPw(true)}>{t('settings.changePassword')}</Button></Row>
    <Row label={t('settings.rotateKey')} help={t('settings.rotateKeyHelp')}><Button size="sm" variant="outline" onClick={() => setRot(true)}>{t('settings.rotateKey')}</Button></Row>
    <ChangePassword open={pw} onOpenChange={setPw} /><RotateKey open={rot} onOpenChange={setRot} />
  </>;
}

function Devices() {
  const t = useT(); const [list, setList] = useState<DeviceInfo[] | null>(null); const [target, setTarget] = useState<DeviceInfo | null>(null); const [rotate, setRotate] = useState(true); const [busy, setBusy] = useState(false);
  const load = () => api.listDevices().then(setList).catch((e) => { setList([]); toast(t('common.error', { msg: errMsg(e) }), 'danger'); });
  useEffect(() => { load(); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  if (!list) return <Skeleton className="h-24" />;
  if (!list.length) return <p className="py-6 text-sm text-muted">{t('devices.empty')}</p>;
  return <><ul className="divide-y divide-border">{list.map((d) => <li key={d.id} className="flex items-center gap-3 py-3">
    <Smartphone className="h-5 w-5 text-muted" aria-hidden />
    <div className="flex-1"><div className="text-sm font-medium">{d.name} {d.current && <Badge tone="ok">{t('devices.current')}</Badge>} {d.revoked && <Badge tone="danger">{t('devices.revoked')}</Badge>}</div>
      <div className="text-xs text-muted">{t('devices.lastSeen', { when: new Date(d.lastSeen).toLocaleString() })}</div></div>
    {!d.current && !d.revoked && <Button size="sm" variant="ghost" onClick={() => { setRotate(true); setTarget(d); }}>{t('devices.revoke')}</Button>}</li>)}</ul>
    <Modal open={!!target} onOpenChange={(o) => !o && setTarget(null)} title={t('devices.revokeTitle', { name: target?.name ?? '' })} description={t('devices.revokeBody')}
      footer={<><Button variant="outline" onClick={() => setTarget(null)}>{t('common.cancel')}</Button><Button variant="danger" disabled={busy} onClick={async () => { if (!target) return; setBusy(true);
        try { await api.revokeDevice({ id: target.id, rotateCredentials: rotate }); toast(t('devices.done')); setTarget(null); load(); } catch (e) { toast(t('common.error', { msg: errMsg(e) }), 'danger'); } finally { setBusy(false); } }}>{t('devices.revoke')}</Button></>}>
      <label className="flex items-start gap-2 text-sm"><input type="checkbox" className="mt-0.5 h-4 w-4" checked={rotate} onChange={(e) => setRotate(e.target.checked)} />{t('devices.rotate')}</label>
      <p className="mt-2 text-xs text-muted">{t('devices.rotateHelp')}</p></Modal></>;
}

const Sync = SyncPanel;

function SshAgent() {
  const t = useT(); const { settings, save } = useSettings(); const [info, setInfo] = useState<SshAgentInfo | null>(null); const [err, setErr] = useState('');
  useEffect(() => { api.sshAgentInfo().then(setInfo).catch((e) => setErr(errMsg(e))); }, []);
  if (!settings) return <Skeleton className="h-24" />;
  const on = !!settings.sshApproveEachUse;
  return <>
    {err && <p role="alert" className="py-3 text-sm text-danger">{t('common.error', { msg: err })}</p>}
    <Row label={t('ssh.status')} help={info?.socketPath}>{info ? <Badge>{info.running ? t('ssh.running') : t('ssh.stopped')}</Badge> : <Skeleton className="h-5 w-16" />}</Row>
    <div className="border-b border-border py-4"><div className="text-sm font-medium">{t('ssh.config')}</div><div className="mt-0.5 text-xs text-muted">{t('ssh.configHelp')}</div>
      <div className="mt-2 flex items-start gap-1"><pre data-testid="ssh-config-line" className="w-full overflow-x-auto rounded-md border border-border bg-panel p-2 font-mono text-xs">{info?.configLine ?? ''}</pre>
        <Button size="icon" variant="ghost" aria-label={t('ssh.copy')} disabled={!info} onClick={() => info && navigator.clipboard?.writeText(info.configLine).then(() => toast(t('ssh.copied')))}><Copy className="h-4 w-4" /></Button></div></div>
    <Row label={t('ssh.approve')} help={t('ssh.approveHelp')}>
      <button role="switch" aria-checked={on} aria-label={t('ssh.approve')} data-testid="ssh-approve-switch" onClick={() => save({ sshApproveEachUse: !on }).then(() => toast(t('common.saved'))).catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger'))}
        className={cn('relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent', on ? 'bg-accent' : 'bg-border')}>
        <span className={cn('absolute left-0 top-0.5 h-5 w-5 rounded-full bg-white transition-transform', on ? 'translate-x-5' : 'translate-x-0.5')} /></button></Row>
    <Row label={t('ssh.cli')} help={t('ssh.cliHelp')}><code className="font-mono text-xs">gitpass ssh config</code></Row></>;
}

function About() {
  const t = useT(); const [ver, setVer] = useState(APP_VERSION);
  useEffect(() => { let on = true; getAppVersion().then((v) => on && setVer(v)); return () => { on = false; }; }, []);
  return <><div className="flex items-center gap-3 py-4"><Logo size={48} />
    <div><div className="text-lg font-semibold">gitpass</div><div className="text-sm text-muted">{t('about.license')}</div></div></div>
    <Row label={t('about.version')}><span className="font-mono text-sm">{ver}</span></Row>
    <UpdateRow />
    <Row label={t('about.repo')}><a className="inline-flex items-center gap-1 text-sm text-accent hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" href={REPO_URL} target="_blank" rel="noreferrer">{REPO_URL.replace('https://', '')}<ExternalLink className="h-3 w-3" aria-hidden /></a></Row>
    <Row label={t('about.issues')}><a className="text-sm text-accent hover:underline" href={ISSUES_URL} target="_blank" rel="noreferrer">{t('about.issues')}</a></Row></>;
}

/** `settings` route with tab `vault:<id>` (Home → vault card → settings). There is no rename/edit-vault
 * command in the contract, so this is a read-only overview with links to items and sharing. */
function VaultPanel({ vaultId }: { vaultId: string }) {
  const t = useT(); const [v, setV] = useState<VaultInfo | null | undefined>(undefined);
  useEffect(() => { api.listVaults().then((vs) => setV(vs.find((x) => x.id === vaultId) ?? null)).catch(() => setV(null)); }, [vaultId]);
  if (v === undefined) return <Skeleton className="h-24" />;
  if (v === null) return <p role="alert" className="py-4 text-sm text-danger">{t('settings.vaultMissing')}</p>;
  const app = useApp.getState();
  return <>
    <Row label={t('settings.vaultName')}><span className="text-sm">{v.name}</span></Row>
    {v.description && <Row label={t('settings.vaultDescription')}><span className="text-sm">{v.description}</span></Row>}
    <Row label={t('settings.vaultItems')}><span className="text-sm">{v.itemCount}</span></Row>
    <Row label={t('people.vaultMembers')}><span className="text-sm">{v.members.length}</span></Row>
    <div className="flex gap-2 py-4"><Button variant="default" onClick={() => app.setRoute({ name: 'items', filter: { kind: 'vault', vaultId: v.id } })}>{t('settings.vaultOpen')}</Button>
      <Button variant="outline" onClick={() => app.openModal('people', { vaultId: v.id })}>{t('people.invite')}</Button></div>
    <p className="pb-3 text-xs text-muted">{t('settings.vaultEditHint')}</p></>;
}

export function SettingsPage({ initialTab }: { initialTab?: string } = {}) {
  const t = useT(); const route = useApp((s) => s.route); const setRoute = useApp((s) => s.setRoute); const { settings, load, error } = useSettings();
  const rawTab = initialTab ?? (route.name === 'settings' ? route.tab : undefined);
  const vaultId = rawTab?.startsWith('vault:') ? rawTab.slice(6) : null;
  // 'help' (toolbar) → About; unknown → General.
  const fromRoute = rawTab === 'help' ? 'about' : rawTab;
  const tab: Tab = (TABS.some((x) => x.id === fromRoute) ? fromRoute : 'general') as Tab;
  const [local, setLocal] = useState<Tab>(tab); useEffect(() => setLocal(tab), [tab]);
  useEffect(() => { if (!settings) load(); }, [settings, load]);
  const body = { general: General, security: Security, devices: Devices, sync: Sync, ssh: SshAgent, about: About }[local];
  const B = body;
  return <div className="mx-auto flex max-w-4xl gap-8 p-8">
    <nav aria-label={t('settings.title')} className="w-44 shrink-0"><h1 className="mb-4 text-2xl font-semibold">{t('settings.title')}</h1>
      <ul role="tablist" aria-orientation="vertical" className="space-y-1">{TABS.map(({ id, icon: I }) => <li key={id}><button role="tab" aria-selected={!vaultId && local === id}
        onClick={() => { setLocal(id); setRoute({ name: 'settings', tab: id }); }} className={cn('flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent', !vaultId && local === id ? 'bg-hover font-medium' : 'text-muted hover:bg-hover hover:text-fg')}>
        <I className="h-4 w-4" aria-hidden />{t(`settings.${id}`)}</button></li>)}</ul></nav>
    <section role="tabpanel" aria-label={vaultId ? t('settings.vault') : t(`settings.${local}`)} className="min-w-0 flex-1 rounded-xl border border-border bg-card px-5 py-2">
      {error && <p role="alert" className="py-3 text-sm text-danger">{t('common.error', { msg: error })}</p>}{vaultId ? <VaultPanel vaultId={vaultId} /> : <B />}</section></div>;
}
