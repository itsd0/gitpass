import { useEffect, useState } from 'react';
import QRCode from 'qrcode';
import { Copy, Inbox, KeyRound, TriangleAlert, UserPlus, Users, X } from 'lucide-react';
import { api } from '../../api/client';
import type { Identity, Member, VaultInfo } from '../../api/types';
import { Avatar, Badge, Button, Modal, Input, Separator, Skeleton, toast } from '../../ui';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';

const errMsg = (e: unknown) => String((e as Error)?.message ?? e);

function IdentityCard({ id }: { id: Identity | null }) {
  const t = useT(); const [qr, setQr] = useState<string | null>(null);
  useEffect(() => { if (id) QRCode.toDataURL(id.publicKey, { margin: 1, width: 168, color: { dark: '#111111', light: '#ffffff' } }).then(setQr).catch(() => setQr(null)); }, [id]);
  return (
    <section aria-labelledby="identity-h" className="rounded-xl border border-border bg-card p-5">
      <div className="flex gap-6">
        <div className="min-w-0 flex-1">
          <h2 id="identity-h" className="flex items-center gap-2 text-lg font-semibold"><KeyRound className="h-5 w-5 text-accent" aria-hidden />{t('people.identity')}</h2>
          <p className="mt-1 text-sm text-muted">{t('people.identityHelp')}</p>
          {!id ? <div className="mt-4 space-y-2"><Skeleton className="h-4 w-3/4" /><Skeleton className="h-4 w-1/2" /></div> : <>
            <div className="mt-4 text-xs uppercase tracking-wide text-muted">{t('people.publicKey')}</div>
            <div className="mt-1 break-all rounded-md bg-bg p-2 font-mono text-xs" data-testid="public-key">{id.publicKey}</div>
            <div className="mt-3 text-xs uppercase tracking-wide text-muted">{t('people.fingerprint')}</div>
            <div className="mt-1 font-mono text-sm" data-testid="fingerprint">{id.fingerprint}</div>
            <Button variant="outline" className="mt-4" size="sm" onClick={async () => { try { await navigator.clipboard.writeText(id.publicKey); toast(t('common.copied')); } catch (e) { toast(t('common.error', { msg: errMsg(e) }), 'danger'); } }}>
              <Copy className="h-4 w-4" aria-hidden />{t('people.copyKey')}</Button></>}
        </div>
        <div className="shrink-0 rounded-lg bg-white p-2">{qr ? <img src={qr} width={168} height={168} alt={t('people.qr')} /> : <Skeleton className="h-[168px] w-[168px]" />}</div>
      </div>
    </section>);
}

function VaultMembers({ vault, onChange }: { vault: VaultInfo; onChange: () => void }) {
  const t = useT(); const [members, setMembers] = useState<Member[] | null>(null);
  const modalId = useApp((st) => st.modal?.id); const [wasOpen, setWasOpen] = useState(false); const [removing, setRemoving] = useState<Member | null>(null); const [busy, setBusy] = useState(false);
  // Share goes through the global modal host; refresh members once it closes.
  useEffect(() => { if (modalId === 'people') setWasOpen(true); else if (wasOpen) { setWasOpen(false); void load(); onChange(); } }, [modalId]); // eslint-disable-line react-hooks/exhaustive-deps
  const load = () => api.listMembers({ vaultId: vault.id }).then(setMembers).catch((e) => { setMembers([]); toast(t('common.error', { msg: errMsg(e) }), 'danger'); });
  useEffect(() => { load(); }, [vault.id]); // eslint-disable-line react-hooks/exhaustive-deps
  const name = (m: Member) => m.isSelf ? t('people.you') : m.label ?? t('people.unnamed');
  const others = (members ?? []).filter((m) => !m.isSelf);
  return (
    <section className="rounded-xl border border-border bg-card" aria-label={vault.name}>
      <header className="flex items-center justify-between px-5 py-4">
        <div><h3 className="font-semibold">{vault.name}</h3>{vault.description && <p className="text-sm text-muted">{vault.description}</p>}</div>
        <Button variant="outline" size="sm" onClick={() => useApp.getState().openModal('people', { vaultId: vault.id })}><UserPlus className="h-4 w-4" aria-hidden />{t('people.invite')}</Button>
      </header>
      <Separator />
      <ul className="divide-y divide-border">
        {members === null && <li className="px-5 py-3"><Skeleton className="h-8 w-60" /></li>}
        {members?.map((m) => (
          <li key={m.fingerprint} className="flex items-center gap-3 px-5 py-3">
            <Avatar name={name(m)} />
            <div className="min-w-0 flex-1"><div className="text-sm font-medium">{name(m)} {m.isSelf && <Badge className="ml-1">{t('people.you')}</Badge>}</div>
              <div className="truncate font-mono text-xs text-muted">{m.fingerprint}</div></div>
            {!m.isSelf && <Button size="sm" variant="ghost" aria-label={`${t('people.remove')} ${name(m)}`} onClick={() => setRemoving(m)}><X className="h-4 w-4" aria-hidden />{t('people.remove')}</Button>}
          </li>))}
        {members && others.length === 0 && <li className="px-5 py-3 text-sm text-muted">{t('people.noMembers')}</li>}
      </ul>
      <Modal open={!!removing} onOpenChange={(o) => !o && setRemoving(null)} title={t('unshare.title', { name: removing ? name(removing) : '' })} description={t('unshare.body')}
        footer={<><Button variant="outline" onClick={() => setRemoving(null)}>{t('common.cancel')}</Button>
          <Button variant="danger" disabled={busy} onClick={async () => { if (!removing) return; setBusy(true);
            try { await api.unshareVault({ vaultId: vault.id, fingerprint: removing.fingerprint }); toast(t('unshare.done')); setRemoving(null); load(); onChange(); }
            catch (e) { toast(t('common.error', { msg: errMsg(e) }), 'danger'); } finally { setBusy(false); } }}>{t('unshare.submit')}</Button></>} />
    </section>);
}

export function AcceptShare({ onAccepted }: { onAccepted?: (v: VaultInfo) => void }) {
  const t = useT(); const [path, setPath] = useState(''); const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  const pick = async () => {
    // Native picker requires @tauri-apps/plugin-dialog (not in contract deps). Fallback: manual path entry.
    try { const mod = '@tauri-apps/plugin-dialog'; const d: any = await import(/* @vite-ignore */ mod); const p = await d.open({ multiple: false }); if (typeof p === 'string') setPath(p); }
    catch { document.getElementById('share-path')?.focus(); }
  };
  return (
    <section className="rounded-xl border border-border bg-card p-5">
      <h2 className="flex items-center gap-2 text-lg font-semibold"><Inbox className="h-5 w-5 text-accent" aria-hidden />{t('invite.title')}</h2>
      <p className="mt-1 text-sm text-muted">{t('invite.desc')}</p>
      <div role="note" className="mt-3 flex gap-2 rounded-lg border border-warn/40 bg-warn/5 p-3 text-sm"><TriangleAlert className="h-4 w-4 shrink-0 text-warn" aria-hidden />{t('invite.warning')}</div>
      <label htmlFor="share-path" className="mt-4 block text-sm text-muted">{t('invite.path')}</label>
      <div className="mt-1 flex gap-2"><Input id="share-path" value={path} onChange={(e) => setPath(e.target.value)} placeholder="~/Downloads/vault.gitpass-share" className="font-mono" />
        <Button variant="outline" onClick={pick}>{t('invite.pick')}</Button>
        <Button variant="default" disabled={!path.trim() || busy} onClick={async () => { setBusy(true); setErr(null);
          try { const v = await api.acceptShare({ path: path.trim() }); toast(t('invite.done', { vault: v.name })); setPath(''); onAccepted?.(v); } catch (e) { setErr(errMsg(e)); } finally { setBusy(false); } }}>{t('invite.accept')}</Button></div>
      {err && <p role="alert" className="mt-2 text-sm text-danger">{t('common.error', { msg: err })}</p>}
      <p className="mt-6 text-center text-sm text-muted">{t('invite.empty')}</p>
    </section>);
}

export function PeoplePage() {
  const t = useT(); const [tab, setTab] = useState<'members' | 'invitations'>('members');
  const [id, setId] = useState<Identity | null>(null); const [vaults, setVaults] = useState<VaultInfo[] | null>(null);
  const loadVaults = () => api.listVaults().then(setVaults).catch(() => setVaults([]));
  useEffect(() => { api.identity().then(setId).catch((e) => toast(t('common.error', { msg: errMsg(e) }), 'danger')); loadVaults(); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const tabBtn = (k: typeof tab, label: string) => <button role="tab" aria-selected={tab === k} onClick={() => setTab(k)}
    className={`rounded-md px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${tab === k ? 'bg-hover font-medium' : 'text-muted hover:text-fg'}`}>{label}</button>;
  return (
    <div className="mx-auto max-w-4xl space-y-6 p-8">
      <header className="flex items-center justify-between"><h1 className="flex items-center gap-2 text-2xl font-semibold"><Users className="h-6 w-6" aria-hidden />{t('people.title')}</h1>
        <div role="tablist" className="flex gap-1 rounded-lg border border-border p-1">{tabBtn('members', t('people.tabMembers'))}{tabBtn('invitations', t('people.tabInvitations'))}</div></header>
      {tab === 'members' ? <>
        <IdentityCard id={id} />
        <h2 className="pt-2 text-lg font-semibold">{t('people.vaultMembers')}</h2>
        {vaults === null ? <Skeleton className="h-32 w-full" /> : vaults.length === 0 ? <p className="text-sm text-muted">{t('people.noVaults')}</p>
          : vaults.map((v) => <VaultMembers key={v.id} vault={v} onChange={loadVaults} />)}
      </> : <AcceptShare onAccepted={loadVaults} />}
    </div>);
}
