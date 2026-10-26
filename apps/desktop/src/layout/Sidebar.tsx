import type * as React from "react";
import { useState } from 'react';
import { Archive, Check, ChevronDown, LayoutGrid, Lock, Moon, MoreHorizontal, Pencil, PanelLeftClose, Plus, Settings as SettingsIcon, ShieldCheck, Star, Sun, Tag, Trash2, Users } from 'lucide-react';
import { VaultIcon } from '../features/home/vaultIcons';
import { DeleteVaultDialog, EditVaultDialog } from '../features/home/VaultDialogs';
import type { VaultInfo } from '../api/types';
import { api } from '../api/client';
import type { Filter } from '../api/types';
import { features } from '../features/registry';
import { useVaultsAndTags } from '../lib/data';
import { useApp, type Route } from '../lib/store';
import { cn } from '../lib/utils';
import { useT, type Key } from '../i18n';
import { OnboardingBadge } from '../features/onboarding';
import { useSettings } from '../features/settings/useSettings';
import { Button, Logo, Dialog, DialogContent, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger, Input, Tooltip, toast } from '../ui';

const same = (r: Route, f: Filter) => r.name === 'items' && r.filter.kind === f.kind && r.filter.vaultId === f.vaultId && r.filter.tag === f.tag;

function NavItem({ active, icon, label, onClick, right }: { active?: boolean; icon: React.ReactNode; label: string; onClick: () => void; right?: React.ReactNode }) {
  return (
    <button onClick={onClick} className={cn('group flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-[14px]', active ? 'bg-active font-medium' : 'hover:bg-hover')}>
      <span className="flex w-4 justify-center">{icon}</span><span className="flex-1 truncate">{label}</span>{right}
    </button>
  );
}
function SectionHeader({ label, action }: { label: string; action?: React.ReactNode }) {
  return <div className="mt-5 flex items-center justify-between px-2.5 pb-1 text-[11px] font-semibold tracking-wider text-muted"><span className="flex items-center gap-1"><ChevronDown size={12} />{label}</span>{action}</div>;
}

export function Sidebar() {
  const { route, setRoute, account, lock, theme, setTheme, toggleSidebar, refreshItems } = useApp();
  const { vaults, tags } = useVaultsAndTags();
  const t = useT(); const L = (label: string) => t(label as Key); // feature sidebar labels are i18n keys
  const toggleTheme = () => { const next = theme === 'dark' ? 'light' : 'dark'; setTheme(next); if (useSettings.getState().settings) void useSettings.getState().save({ theme: next }).catch(() => {}); };
  const go = (filter: Filter) => setRoute({ name: 'items', filter: { sort: 'updated', ...filter } });
  const [newVault, setNewVault] = useState(false);
  // `account` is the account UUID: show a friendly name instead (first vault, else Personal).
  const accountName = vaults.find((v) => v.name === 'Personal')?.name ?? vaults[0]?.name ?? 'Personal';
  void account;
  const [vName, setVName] = useState('');
  const [editV, setEditV] = useState<VaultInfo | null>(null); const [delV, setDelV] = useState<VaultInfo | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const extraTop = features.filter((f) => f.sidebar?.section === 'top' && f.id !== 'items' && f.route);
  const extraBottom = features.filter((f) => f.sidebar?.section === 'bottom' && f.route);

  return (
    <aside className="flex h-full flex-col bg-sidebar" data-testid="sidebar">
      <div className="flex items-center gap-1 border-b border-border p-2.5">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="flex min-w-0 flex-1 items-center gap-2.5 rounded-md px-1.5 py-1.5 hover:bg-hover">
              <Logo size={30} />
              <span className="flex min-w-0 flex-1 flex-col text-left leading-tight"><span className="truncate font-medium">{accountName}</span><span className="truncate text-[11px] text-muted">gitpass</span></span><ChevronDown size={14} className="text-muted" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-56">
            <DropdownMenuLabel>Vaults</DropdownMenuLabel>
            {(vaults.length ? vaults : [{ id: '', name: accountName, color: null, icon: null }]).map((v) => <DropdownMenuItem key={v.id || 'default'} onSelect={() => v.id && go({ kind: 'vault', vaultId: v.id })}><VaultIcon icon={v.icon} color={v.color} size={18} />{v.name}{v.name === accountName && <Check size={14} className="ml-auto text-accent" />}</DropdownMenuItem>)}
            <DropdownMenuItem onSelect={() => setNewVault(true)}><Plus size={14} />New vault…</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={toggleTheme}>{theme === 'dark' ? <Sun size={14} /> : <Moon size={14} />}{theme === 'dark' ? 'Light theme' : 'Dark theme'}</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => setRoute({ name: 'settings' })}><SettingsIcon size={14} />{t('nav.settings')}…</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => { void api.lock().finally(lock); }}><Lock size={14} />{t('cmd.lock')}</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <Tooltip content="Collapse sidebar"><Button variant="ghost" size="iconSm" aria-label="Collapse sidebar" onClick={toggleSidebar}><PanelLeftClose size={15} /></Button></Tooltip>
      </div>

      <nav className="flex-1 overflow-y-auto p-2">
        {extraTop.filter((f) => f.id === 'home').map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} right={<OnboardingBadge />} onClick={() => setRoute({ name: f.route } as Route)} />; })}
        <NavItem active={same(route, { kind: 'all' })} icon={<LayoutGrid size={15} className="text-accent" />} label={t('nav.allItems')} onClick={() => go({ kind: 'all' })} />
        <NavItem active={same(route, { kind: 'favorites' })} icon={<Star size={15} className="fill-warn text-warn" />} label={t('nav.favorites')} onClick={() => go({ kind: 'favorites' })} />
        <NavItem active={route.name === 'watchtower'} icon={<ShieldCheck size={15} className="text-ok" />} label={t('nav.watchtower')} onClick={() => setRoute({ name: 'watchtower' })} />
        {extraTop.filter((f) => !['home', 'watchtower'].includes(f.id)).map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} onClick={() => setRoute({ name: f.route } as Route)} />; })}

        <SectionHeader label={t('nav.vaults').toUpperCase()} action={<Tooltip content="New vault"><button aria-label="New vault" className="rounded p-0.5 hover:bg-hover" onClick={() => setNewVault(true)}><Plus size={14} /></button></Tooltip>} />
        {vaults.map((v) => (
          <div key={v.id} className="group/vault relative" data-testid="sidebar-vault" onContextMenu={(e) => { e.preventDefault(); setMenuFor(v.id); }}>
            <NavItem active={same(route, { kind: 'vault', vaultId: v.id })} label={v.name}
              icon={<VaultIcon icon={v.icon} color={v.color} size={16} />}
              onClick={() => go({ kind: 'vault', vaultId: v.id })}
              right={<span className="flex w-8 justify-end group-hover/vault:invisible">{v.shared ? <Users size={13} className="text-muted" aria-label="Shared vault" /> : <span className="text-xs text-muted">{v.itemCount}</span>}</span>} />
            <DropdownMenu open={menuFor === v.id} onOpenChange={(o) => setMenuFor(o ? v.id : null)}>
              <DropdownMenuTrigger asChild><button aria-label={`${t('vault.more')}: ${v.name}`} className={cn('absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted hover:bg-active hover:text-fg', menuFor === v.id ? 'visible' : 'invisible group-hover/vault:visible')}><MoreHorizontal size={14} /></button></DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                <DropdownMenuItem onSelect={() => setEditV(v)}><Pencil size={14} />{t('vault.edit')}</DropdownMenuItem>
                <DropdownMenuItem onSelect={() => setRoute({ name: 'settings', tab: `vault:${v.id}` })}><SettingsIcon size={14} />{t('nav.settings')}…</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem className="text-danger" onSelect={() => setDelV(v)}><Trash2 size={14} />{t('vault.delete')}</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        ))}

        {tags.length > 0 && <SectionHeader label={t('nav.tags').toUpperCase()} />}
        {tags.map((t) => <NavItem key={t} active={same(route, { kind: 'tag', tag: t })} icon={<Tag size={14} className="text-ok" />} label={t} onClick={() => go({ kind: 'tag', tag: t })} />)}
      </nav>

      <div className="border-t border-border p-2">
        {extraBottom.map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} onClick={() => setRoute({ name: f.route } as Route)} />; })}
        <NavItem active={same(route, { kind: 'archive' })} icon={<Archive size={15} />} label={t('nav.archive')} onClick={() => go({ kind: 'archive' })} />
        <NavItem active={same(route, { kind: 'deleted' })} icon={<Trash2 size={15} />} label={t('nav.deleted')} onClick={() => go({ kind: 'deleted' })} />
      </div>

      <Dialog open={newVault} onOpenChange={setNewVault}>
        <DialogContent title="New vault" description="Vaults keep items separate and can be shared.">
          <form onSubmit={(e) => { e.preventDefault(); if (!vName.trim()) return; api.createVault({ name: vName.trim() }).then((v) => { toast(`Created ${v.name}`); setNewVault(false); setVName(''); refreshItems(); go({ kind: 'vault', vaultId: v.id }); }, (er) => toast(String(er), 'danger')); }}>
            <Input autoFocus placeholder="Vault name" value={vName} onChange={(e) => setVName(e.target.value)} />
            <div className="mt-4 flex justify-end gap-2"><Button type="button" variant="outline" onClick={() => setNewVault(false)}>Cancel</Button><Button type="submit">Create</Button></div>
          </form>
        </DialogContent>
      </Dialog>
      <EditVaultDialog vault={editV} onOpenChange={(o) => !o && setEditV(null)} onSaved={() => { toast(t('vault.saved')); refreshItems(); }} />
      <DeleteVaultDialog vault={delV} isLast={vaults.length <= 1} onOpenChange={(o) => !o && setDelV(null)}
        onDeleted={(id) => { toast(t('vault.deleted', { vault: delV?.name ?? '' })); if (same(route, { kind: 'vault', vaultId: id })) go({ kind: 'all' }); useApp.getState().selectItem(null); refreshItems(); }} />
    </aside>
  );
}
