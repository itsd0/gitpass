import { useState } from 'react';
import type { VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { Button, Modal, Input } from '../../ui';
import { VAULT_COLORS, VAULT_ICONS, VaultIcon } from './vaultIcons';
export function NewVaultDialog({ open, onOpenChange, onCreated }: { open: boolean; onOpenChange: (o: boolean) => void; onCreated: (v: VaultInfo) => void }) {
  const [name, setName] = useState(''); const [description, setDescription] = useState(''); const [icon, setIcon] = useState('vault'); const [color, setColor] = useState(VAULT_COLORS[1]);
  const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  const submit = async (e: React.FormEvent) => { e.preventDefault(); if (!name.trim()) return; setBusy(true); setErr(null);
    try { const v = await api.createVault({ name: name.trim(), description: description.trim() || null, icon, color }); onCreated(v); onOpenChange(false); setName(''); setDescription(''); }
    catch (e) { setErr(String(e)); } finally { setBusy(false); } };
  return (<Modal open={open} onOpenChange={onOpenChange} title="New Vault" className="max-w-md">
    <form onSubmit={submit} className="space-y-4">
      <div className="flex justify-center"><VaultIcon icon={icon} color={color} size={56} /></div>
      <label className="block text-xs text-muted">Name<Input className="mt-1" autoFocus aria-label="Vault name" value={name} onChange={e => setName(e.target.value)} /></label>
      <label className="block text-xs text-muted">Description<Input className="mt-1" aria-label="Vault description" value={description} onChange={e => setDescription(e.target.value)} /></label>
      <div><div className="mb-1 text-xs text-muted">Icon</div><div className="flex flex-wrap gap-1">{Object.entries(VAULT_ICONS).map(([k, I]) => (
        <button type="button" key={k} aria-label={`Icon ${k}`} aria-pressed={icon === k} onClick={() => setIcon(k)} className={`rounded p-2 ${icon === k ? 'bg-accent' : 'bg-hover hover:bg-hover'}`}><I size={16} /></button>))}</div></div>
      <div><div className="mb-1 text-xs text-muted">Color</div><div className="flex gap-2">{VAULT_COLORS.map(c => (
        <button type="button" key={c} aria-label={`Color ${c}`} aria-pressed={color === c} onClick={() => setColor(c)} className={`h-6 w-6 rounded-full ${color === c ? 'ring-2 ring-white ring-offset-2 ring-offset-neutral-900' : ''}`} style={{ background: c }} />))}</div></div>
      {err && <p role="alert" className="text-sm text-danger">{err}</p>}
      <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button><Button type="submit" variant="default" disabled={!name.trim() || busy}>Create vault</Button></div>
    </form></Modal>);
}
