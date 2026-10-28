import { useEffect, useState } from 'react';
import { ExternalLink, GitMerge, GitBranch, Link2Off, RefreshCw } from 'lucide-react';
import { api } from '../../api/client';
import type { SyncStatus } from '../../api/types';
import { cn } from '../../lib/utils';
import { Badge, Button, Input, Modal, Skeleton } from '../../ui';
import { useT, type Key } from '../../i18n';
import { useOnboarding } from '../onboarding/useOnboarding';
import { SettingsSection, SettingsRow } from './SettingsSection';

const errMsg = (e: unknown) => String((e as Error)?.message ?? e);
export const NEW_REPO_URL = 'https://github.com/new';

/** Client-side mirror of `gitpass_core::remote::validate_url` (the backend validates again). */
export function isValidRemoteUrl(raw: string): boolean {
  const u = raw.trim();
  if (!u || u.startsWith('-') || /\s/.test(u)) return false;
  const path = (p: string) => { const s = p.replace(/^\/+|\/+$/g, ''); return !!s && !s.includes('..') && s.split('/').every(Boolean); };
  const host = (h: string) => /^[A-Za-z0-9.\-:[\]]+$/.test(h);
  let m = /^https:\/\/([^/]+)\/(.*)$/.exec(u);
  if (m) return !m[1]!.includes('@') && host(m[1]!) && path(m[2]!);
  m = /^ssh:\/\/(?:[A-Za-z0-9._-]+@)?([^/@]+)\/(.*)$/.exec(u);
  if (m) return host(m[1]!) && path(m[2]!);
  if (u.includes('://')) return false;
  m = /^([A-Za-z0-9._-]+)@([A-Za-z0-9.-]+):(.+)$/.exec(u);
  return !!m && path(m[3]!);
}

/** Map a backend error (prefixed with a `remote::E_*` marker) to a localized explanation. */
export function remoteErrorKey(msg: string): Key | null {
  if (msg.includes('GIT_NOT_INSTALLED')) return 'sync.err.git';
  if (msg.includes('AUTH_FAILED')) return 'sync.err.auth';
  if (msg.includes('WRONG_ACCOUNT')) return 'sync.err.account';
  if (msg.includes('REMOTE_NOT_EMPTY')) return 'sync.err.notEmpty';
  if (msg.includes('DIVERGED')) return 'sync.err.diverged';
  if (msg.includes('INVALID_URL')) return 'sync.err.url';
  if (msg.includes('SECRET_STAGED')) return 'sync.err.secret';
  if (msg.includes('ALREADY_CONNECTED')) return 'sync.err.already';
  return null;
}

function ErrorBox({ text }: { text: string }) {
  const t = useT(); const k = remoteErrorKey(text);
  return <div role="alert" className="mt-4 rounded-md border border-danger p-3 text-sm text-danger">
    {k && <div className="mb-1 font-medium">{t(k)}</div>}
    {k === 'sync.err.auth' && <ul className="mb-2 list-disc pl-5 text-fg"><li>{t('sync.err.authSsh')}</li><li>{t('sync.err.authGh')}</li></ul>}
    <pre className="whitespace-pre-wrap break-words font-mono text-xs opacity-80">{text.replace(/^[A-Z_]+: /, '')}</pre></div>;
}

export function ConnectForm({ onConnected }: { onConnected: (s: SyncStatus) => void }) {
  const t = useT(); const { patch } = useOnboarding();
  const [url, setUrl] = useState(''); const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  const valid = isValidRemoteUrl(url);
  const connect = async () => {
    setBusy(true); setErr(null);
    try { const s = await api.remoteConnect({ url: url.trim() }); await patch({ repoConnected: true }).catch(() => {}); onConnected(s); }
    catch (e) { setErr(errMsg(e)); } finally { setBusy(false); }
  };
  return <form aria-label={t('sync.connectTitle')} className="py-4" onSubmit={(e) => { e.preventDefault(); if (valid && !busy) connect(); }}>
    <div className="flex items-center gap-2 text-sm font-medium"><GitBranch className="h-4 w-4" aria-hidden />{t('sync.connectTitle')}</div>
    <ol className="mt-2 list-decimal space-y-1 pl-5 text-xs text-muted">
      <li>{t('sync.hintCreate')} <button type="button" className="inline-flex items-center gap-1 text-accent underline" onClick={() => api.openNewRepoPage().catch(() => {})}>{t('sync.openGithubNew')}<ExternalLink className="h-3 w-3" aria-hidden /></button></li>
      <li>{t('sync.hintPaste')}</li>
      <li>{t('sync.hintAuth')}</li>
    </ol>
    <label className="mt-3 block text-xs font-medium" htmlFor="remote-url">{t('sync.repoUrl')}</label>
    <div className="mt-1 flex gap-2">
      <Input id="remote-url" className="flex-1 font-mono" placeholder="git@github.com:you/gitpass-vault.git" value={url} disabled={busy}
        onChange={(e) => setUrl(e.target.value)} autoComplete="off" spellCheck={false} aria-invalid={!!url && !valid} />
      <Button type="submit" variant="default" disabled={!valid || busy}>{busy && <RefreshCw className="h-4 w-4 motion-safe:animate-spin" aria-hidden />}{busy ? t('sync.connecting') : t('sync.connect')}</Button>
    </div>
    {url && !valid && <p className="mt-1 text-xs text-danger">{t('sync.err.url')}</p>}
    {busy && <p role="status" className="mt-2 text-xs text-muted">{t('sync.progress')}</p>}
    {err && <ErrorBox text={err} />}
  </form>;
}

export function SyncPanel({ highlightedId }: { highlightedId?: string | null } = {}) {
  const t = useT(); const { patch } = useOnboarding();
  const [st, setSt] = useState<SyncStatus | null>(null); const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false); const [confirm, setConfirm] = useState(false);
  const load = () => api.syncStatus().then(setSt).catch((e) => setResult({ ok: false, text: errMsg(e) }));
  useEffect(() => { load(); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const run = async (merge: boolean) => { setBusy(true); try { setResult({ ok: true, text: (await api.sync({ merge })).message }); } catch (e) { setResult({ ok: false, text: errMsg(e) }); } finally { setBusy(false); load(); } };
  const disconnect = async () => {
    setBusy(true); try { setSt(await api.remoteDisconnect()); await patch({ repoConnected: false }).catch(() => {}); setResult(null); }
    catch (e) { setResult({ ok: false, text: errMsg(e) }); } finally { setBusy(false); setConfirm(false); }
  };
  if (!st) return result ? <ErrorBox text={result.text} /> : <Skeleton className="h-24" />;
  if (!st.remote) return <ConnectForm onConnected={(s) => { setSt(s); setResult({ ok: true, text: t('sync.connected') }); }} />;
  return <div id="setting-sync" data-setting-id="setting-sync">
    <SettingsSection title={t('settings.sectionSync')}>
      <SettingsRow label={t('sync.remote')} highlighted={highlightedId === 'setting-sync'}>
        <span className="break-all font-mono text-sm">{st.remote}</span>
      </SettingsRow>
      <SettingsRow label={t('sync.lastSync')}>
        <span className="text-sm">{st.lastSync ? new Date(st.lastSync).toLocaleString() : t('sync.never')}</span>
      </SettingsRow>
      <SettingsRow label={t('sync.status')}>
        <div className="flex gap-1">
          <Badge>{t('sync.ahead', { n: st.ahead })}</Badge>
          <Badge>{t('sync.behind', { n: st.behind })}</Badge>
          {st.dirty && <Badge tone="warn">{t('sync.dirty')}</Badge>}
        </div>
      </SettingsRow>
    </SettingsSection>
    <div className="mt-4 flex flex-wrap gap-2">
      <Button variant="default" disabled={busy} onClick={() => run(false)}>
        <RefreshCw className={cn('h-4 w-4', busy && 'motion-safe:animate-spin')} aria-hidden />{t('sync.now')}
      </Button>
      <Button variant="outline" disabled={busy} onClick={() => run(true)}>
        <GitMerge className="h-4 w-4" aria-hidden />{t('sync.merge')}
      </Button>
      <Button variant="outline" disabled={busy} onClick={() => setConfirm(true)}>
        <Link2Off className="h-4 w-4" aria-hidden />{t('sync.disconnect')}
      </Button>
    </div>
    {result && (result.ok ? <div role="status" className="mt-4 rounded-md border border-border p-3 text-sm"><div className="text-xs text-muted">{t('sync.result')}</div><pre className="whitespace-pre-wrap font-mono text-xs">{result.text}</pre></div> : <ErrorBox text={result.text} />)}
    {confirm && <Modal open onOpenChange={(o) => !o && setConfirm(false)} title={t('sync.disconnectTitle')}
      footer={<><Button variant="outline" onClick={() => setConfirm(false)}>{t('common.cancel')}</Button><Button variant="danger" disabled={busy} onClick={disconnect}>{t('sync.disconnect')}</Button></>}>
      <p className="text-sm">{t('sync.disconnectBody')}</p></Modal>}
  </div>;
}
