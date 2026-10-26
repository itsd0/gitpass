import { useEffect, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import type { VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { useT } from '../../i18n';
import { Button, Modal, Input } from '../../ui';
import { VAULT_COLORS, VAULT_ICONS, VaultIcon } from './vaultIcons';

/** Rename / change description, icon and color of an existing vault. */
export function EditVaultDialog({ vault, onOpenChange, onSaved }: { vault: VaultInfo | null; onOpenChange: (o: boolean) => void; onSaved: (v: VaultInfo) => void }) {
  const t = useT();
  const [name, setName] = useState(''); const [description, setDescription] = useState(''); const [icon, setIcon] = useState('vault'); const [color, setColor] = useState(VAULT_COLORS[0]);
  const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  useEffect(() => { if (vault) { setName(vault.name); setDescription(vault.description ?? ''); setIcon(vault.icon ?? 'vault'); setColor(vault.color ?? VAULT_COLORS[0]); setErr(null); } }, [vault]);
  const submit = async (e: React.FormEvent) => { e.preventDefault(); if (!vault || !name.trim()) return; setBusy(true); setErr(null);
    try { const v = await api.updateVault({ vaultId: vault.id, name: name.trim(), description: description.trim() || null, icon, color }); onSaved(v); onOpenChange(false); }
    catch (e) { setErr(String((e as Error)?.message ?? e)); } finally { setBusy(false); } };
  return (<Modal open={!!vault} onOpenChange={onOpenChange} title={t('vault.editTitle')} className="max-w-md">
    <form onSubmit={submit} className="space-y-4" data-testid="edit-vault">
      <div className="flex justify-center"><VaultIcon icon={icon} color={color} size={56} /></div>
      <label className="block text-xs text-muted">{t('settings.vaultName')}<Input className="mt-1" autoFocus aria-label="Vault name" value={name} onChange={e => setName(e.target.value)} /></label>
      <label className="block text-xs text-muted">{t('settings.vaultDescription')}<Input className="mt-1" aria-label="Vault description" value={description} onChange={e => setDescription(e.target.value)} /></label>
      <div><div className="mb-1 text-xs text-muted">{t('vault.icon')}</div><div className="flex flex-wrap gap-1">{Object.entries(VAULT_ICONS).map(([k, I]) => (
        <button type="button" key={k} aria-label={`Icon ${k}`} aria-pressed={icon === k} onClick={() => setIcon(k)} className={`rounded p-2 ${icon === k ? 'bg-accent' : 'bg-hover'}`}><I size={16} /></button>))}</div></div>
      <div><div className="mb-1 text-xs text-muted">{t('vault.color')}</div><div className="flex gap-2">{VAULT_COLORS.map(c => (
        <button type="button" key={c} aria-label={`Color ${c}`} aria-pressed={color === c} onClick={() => setColor(c)} className={`h-6 w-6 rounded-full ${color === c ? 'ring-2 ring-white ring-offset-2 ring-offset-neutral-900' : ''}`} style={{ background: c }} />))}</div></div>
      {err && <p role="alert" className="text-sm text-danger">{err}</p>}
      <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>{t('common.cancel')}</Button><Button type="submit" disabled={!name.trim() || busy}>{t('vault.save')}</Button></div>
    </form></Modal>);
}

/** Permanent vault deletion: strong warning + the user must type the vault name. The last vault cannot be deleted. */
export function DeleteVaultDialog({ vault, isLast, onOpenChange, onDeleted }: { vault: VaultInfo | null; isLast: boolean; onOpenChange: (o: boolean) => void; onDeleted: (id: string) => void }) {
  const t = useT();
  const [typed, setTyped] = useState(''); const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  useEffect(() => { setTyped(''); setErr(null); }, [vault]);
  const ok = !!vault && typed === vault.name && !isLast;
  const submit = async (e: React.FormEvent) => { e.preventDefault(); if (!ok || !vault) return; setBusy(true); setErr(null);
    try { await api.deleteVault({ vaultId: vault.id, confirmName: typed }); onDeleted(vault.id); onOpenChange(false); }
    catch (e) { setErr(String((e as Error)?.message ?? e)); } finally { setBusy(false); } };
  return (<Modal open={!!vault} onOpenChange={onOpenChange} title={t('vault.deleteTitle', { vault: vault?.name ?? '' })} className="max-w-md">
    <form onSubmit={submit} className="space-y-4" data-testid="delete-vault">
      {isLast ? <p role="alert" className="text-sm text-danger">{t('vault.lastVault')}</p> : <>
        <div className="flex gap-3 rounded-lg border border-danger/50 bg-danger/10 p-3 text-sm"><AlertTriangle size={18} className="shrink-0 text-danger" />
          <div className="space-y-1"><p className="font-medium">{t('vault.deleteWarn', { n: vault?.itemCount ?? 0 })}</p><p className="text-muted">{t('vault.deleteDetail')}</p>
            {vault?.shared && <p className="text-muted">{t('vault.deleteShared')}</p>}</div></div>
        <label className="block text-xs text-muted">{t('vault.typeName', { vault: vault?.name ?? '' })}<Input className="mt-1" autoFocus aria-label="Confirm vault name" value={typed} onChange={e => setTyped(e.target.value)} /></label></>}
      {err && <p role="alert" className="text-sm text-danger">{err}</p>}
      <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>{t('common.cancel')}</Button>
        {!isLast && <Button type="submit" variant="danger" disabled={!ok || busy}>{t('vault.deleteSubmit')}</Button>}</div>
    </form></Modal>);
}
