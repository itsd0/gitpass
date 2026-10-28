import { useEffect, useState } from 'react';
import { ArrowRight, ChevronRight, Layers, Plus, Puzzle, Settings, X } from 'lucide-react';
import type { VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { useT } from '../../i18n';
import { Avatar, Button, Skeleton } from '../../ui';
import { useApp } from '../../lib/store';
import { Checklist } from '../onboarding/Checklist';
import { useOnboarding } from '../onboarding/useOnboarding';
import { NewVaultDialog } from './NewVaultDialog';
import { VaultIcon } from './vaultIcons';
function VaultCard({ v, onOpen, onSettings }: { v: VaultInfo | 'all'; onOpen: () => void; onSettings?: () => void }) {
  const all = v === 'all';
  return (<div className={`group flex flex-col rounded-lg border bg-panel ${all ? 'border-violet-500/60' : 'border-border'} hover:border-border`}>
    <button onClick={onOpen} className="relative flex-1 p-4 pt-8 text-left" aria-label={`Open ${all ? 'All Vaults' : v.name}`}>
      <div className="absolute -top-0 right-4 translate-y-[-30%]">{all ? <span className="inline-flex h-10 w-10 items-center justify-center rounded-full bg-gradient-to-br from-pink-500 via-amber-400 to-sky-500 ring-4 ring-panel"><Layers size={20} /></span> : <VaultIcon icon={v.icon} color={v.color} />}</div>
      <div className="text-lg font-semibold">{all ? 'All Vaults' : v.name}</div>
      {!all && <div className="text-sm text-muted">{v.itemCount} {v.itemCount === 1 ? 'item' : 'items'}</div>}
      <p className="mt-2 line-clamp-2 text-sm text-muted">{all ? 'View all of your items together, across all vaults.' : v.description ?? (v.shared ? 'Shared vault.' : 'Only you can view and edit items in this vault.')}</p>
      {!all && <div className="mt-3 flex gap-1">{v.members.slice(0, 5).map(m => <Avatar key={m.fingerprint} name={m.label ?? m.fingerprint} />)}</div>}
    </button>
    <div className="flex items-center border-t border-border">
      {onSettings && <button aria-label="Vault settings" onClick={onSettings} className="border-r border-border p-3 text-muted hover:text-fg"><Settings size={16} /></button>}
      <button aria-label="Open vault" onClick={onOpen} className="ml-auto p-3 text-muted hover:text-fg"><ArrowRight size={18} /></button></div></div>);
}
export function HomePage() {
  const t = useT();
  const { setRoute } = useApp(); const { onboarding, patch } = useOnboarding();
  const [vaults, setVaults] = useState<VaultInfo[] | null>(null); const [err, setErr] = useState<string | null>(null); const [newOpen, setNewOpen] = useState(false);
  useEffect(() => { api.listVaults().then(setVaults).catch(e => setErr(String(e))); }, []);
  return (<div className="mx-auto max-w-5xl space-y-8 p-8">
    {onboarding && !onboarding.dismissed && (<section className="relative rounded-xl border border-border bg-panel p-6" aria-label="Let's get started">
      <button className="absolute right-4 top-3 flex items-center gap-1 text-xs text-muted hover:text-fg" onClick={() => patch({ dismissed: true })}><X size={12} /> Dismiss</button>
      <div className="grid gap-6 md:grid-cols-2">
        <div><h2 className="text-2xl font-semibold">Let’s get started</h2><p className="mt-3 text-fg">We’ll walk you through the most important steps to set up gitpass and keep your vault safe and synced.</p><div className="mt-4"><Checklist /></div></div>
        <div className="flex self-start overflow-hidden rounded-lg border border-border">
          <div className="flex-1 p-4"><h3 className="font-semibold">Get the gitpass browser extension</h3><p className="mt-1 text-sm text-fg">You’ll need it to fill, save and update passwords as you browse.</p>
            <div className="mt-8 border-t border-border pt-3"><button className="flex items-center gap-1 text-sm text-accent hover:underline" onClick={() => patch({ extensionInstalled: true })}>{onboarding.extensionInstalled ? 'Installed' : 'Get it now'} <ChevronRight size={14} /></button></div></div>
          <div className="flex w-28 items-center justify-center bg-gradient-to-br from-teal-700 to-cyan-900"><Puzzle size={48} className="text-cyan-200" /></div></div></div></section>)}
    <section aria-label="Vaults">
      <div className="mb-6 flex items-center justify-between"><h2 className="text-xl font-semibold">Vaults</h2><Button variant="outline" onClick={() => setNewOpen(true)}><Plus size={14} /> {t('vault.newTitle')}</Button></div>
      {err && <p role="alert" className="text-sm text-danger">{err}</p>}
      <div className="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-3">
        {vaults === null && !err ? [0, 1, 2].map(i => <Skeleton key={i} className="h-52" />) : (<>
          <VaultCard v="all" onOpen={() => setRoute({ name: 'items', filter: { kind: 'all' } })} />
          {vaults?.map(v => <VaultCard key={v.id} v={v} onOpen={() => setRoute({ name: 'items', filter: { kind: 'vault', vaultId: v.id } })} onSettings={() => setRoute({ name: 'settings', tab: `vault:${v.id}` })} />)}</>)}
      </div></section>
    <NewVaultDialog open={newOpen} onOpenChange={setNewOpen} onCreated={v => setVaults(vs => [...(vs ?? []), v])} />
  </div>);
}
