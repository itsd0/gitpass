import * as React from "react";
import { useEffect, useRef, useState } from 'react';
import { Archive, Check, ChevronDown, KeyRound, LayoutGrid, Lock, Moon, MoreHorizontal, Pencil, PanelLeftClose, Plus, Settings as SettingsIcon, ShieldCheck, Star, Sun, Tag, Trash2, Users } from 'lucide-react';
import { VaultIcon } from '../features/home/vaultIcons';
import { DeleteVaultDialog, EditVaultDialog } from '../features/home/VaultDialogs';
import { NewVaultDialog } from '../features/home/NewVaultDialog';
import { DeleteTagDialog, MergeTagDialog } from './TagDialogs';
import type { BulkResult, VaultInfo } from '../api/types';
import { api } from '../api/client';
import type { Filter } from '../api/types';
import { features } from '../features/registry';
import { useVaultsAndTags } from '../lib/data';
import { useApp, type Route } from '../lib/store';
import { cn } from '../lib/utils';
import { useT, useI18n, translate, type Key } from '../i18n';
import { OnboardingBadge } from '../features/onboarding';
import { useSettings } from '../features/settings/useSettings';
import { useDragStore, canDropOnVault } from '../features/items/dragState';
import { runBulk } from '../features/items/BulkPane';
import { Button, Logo, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger, Tooltip, toast } from '../ui';

const same = (r: Route, f: Filter) => r.name === 'items' && r.filter.kind === f.kind && r.filter.vaultId === f.vaultId && r.filter.tag === f.tag;

const NavItem = React.forwardRef<
  HTMLButtonElement,
  {
    active?: boolean;
    icon: React.ReactNode;
    label: string;
    onClick: () => void;
    right?: React.ReactNode;
    onContextMenu?: (e: React.MouseEvent) => void;
    onKeyDown?: (e: React.KeyboardEvent) => void;
    onDragOver?: (e: React.DragEvent) => void;
    onDragLeave?: (e: React.DragEvent) => void;
    onDrop?: (e: React.DragEvent) => void;
    className?: string;
  }
>(({ active, icon, label, onClick, right, onContextMenu, onKeyDown, onDragOver, onDragLeave, onDrop, className }, ref) => {
  return (
    <button
      ref={ref}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onKeyDown={onKeyDown}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      className={cn(
        'group flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-[14px]',
        active ? 'bg-active font-medium' : 'hover:bg-hover',
        className
      )}
    >
      <span className="flex w-4 justify-center">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {right}
    </button>
  );
});
NavItem.displayName = 'NavItem';

function SectionHeader({ label, action }: { label: string; action?: React.ReactNode }) {
  return <div className="mt-5 flex items-center justify-between px-2.5 pb-1 text-[11px] font-semibold tracking-wider text-muted"><span className="flex items-center gap-1"><ChevronDown size={12} />{label}</span>{action}</div>;
}

function SidebarTagItem({
  tag,
  active,
  isEditing,
  menuOpen,
  onOpenMenu,
  onCloseMenu,
  onStartRename,
  onCancelRename,
  onRename,
  onSelect,
  onPromptDelete,
}: {
  tag: string;
  active: boolean;
  isEditing: boolean;
  menuOpen: boolean;
  onOpenMenu: () => void;
  onCloseMenu: () => void;
  onStartRename: () => void;
  onCancelRename: () => void;
  onRename: (newTag: string) => Promise<void>;
  onSelect: () => void;
  onPromptDelete: () => void;
}) {
  const t = useT();
  const [val, setVal] = useState(tag);
  const inputRef = useRef<HTMLInputElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const committingRef = useRef(false);
  const dragged = useDragStore((s) => s.dragged);
  const [hovered, setHovered] = useState(false);

  useEffect(() => {
    if (!dragged) setHovered(false);
  }, [dragged]);

  useEffect(() => {
    if (isEditing) {
      setVal(tag);
      committingRef.current = false;
      setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 0);
    }
  }, [isEditing, tag]);

  const commit = async () => {
    committingRef.current = true;
    const trimmed = val.trim();
    if (!trimmed || trimmed === tag) {
      onCancelRename();
      committingRef.current = false;
      buttonRef.current?.focus();
      return;
    }
    await onRename(trimmed);
    committingRef.current = false;
    buttonRef.current?.focus();
  };

  if (isEditing) {
    return (
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void commit();
        }}
        className="flex w-full items-center gap-2 rounded-md bg-active px-2 py-1 text-[14px]"
        data-testid="sidebar-tag-rename"
      >
        <Tag size={14} className="shrink-0 text-ok" />
        <input
          ref={inputRef}
          type="text"
          value={val}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              void commit();
            } else if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              onCancelRename();
              buttonRef.current?.focus();
            }
          }}
          onBlur={() => {
            if (!committingRef.current) {
              onCancelRename();
            }
          }}
          className="min-w-0 flex-1 rounded border border-accent bg-panel px-1.5 py-0.5 text-[14px] text-fg outline-none ring-1 ring-accent"
          autoFocus
          aria-label={t("tag.renameAria", { tag })}
        />
      </form>
    );
  }

  return (
    <div
      className="group/tag relative"
      data-testid="sidebar-tag"
      data-tag-name={tag}
      onContextMenu={(e) => {
        e.preventDefault();
        onOpenMenu();
      }}
      onDragOver={(e) => {
        if (!dragged) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
        setHovered(true);
      }}
      onDragLeave={(e) => { const next = e.relatedTarget as Node | null; if (next && e.currentTarget.contains(next)) return; setHovered(false); }}
      onDrop={(e) => {
        if (!dragged) return;
        e.preventDefault();
        setHovered(false);
        void runBulk('add_tag', { tag }, dragged.items);
        useDragStore.getState().endDrag();
      }}
    >
      <NavItem
        ref={buttonRef}
        active={active}
        icon={<Tag size={14} className="text-ok" />}
        label={tag}
        onClick={onSelect}
        className={cn(hovered && 'ring-2 ring-inset ring-accent bg-accent/10')}
        onKeyDown={(e) => {
          if ((e.shiftKey && e.key === 'F10') || e.key === 'ContextMenu') {
            e.preventDefault();
            onOpenMenu();
          }
        }}
      />
      <DropdownMenu
        open={menuOpen}
        onOpenChange={(o) => {
          if (o) onOpenMenu();
          else {
            onCloseMenu();
            buttonRef.current?.focus();
          }
        }}
      >
        <DropdownMenuTrigger asChild>
          <button
            aria-label={`${t('tag.more')}: ${tag}`}
            className={cn(
              'absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted hover:bg-active hover:text-fg',
              menuOpen ? 'visible' : 'invisible group-hover/tag:visible focus:visible'
            )}
          >
            <MoreHorizontal size={14} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem
            onSelect={() => {
              setTimeout(() => {
                onStartRename();
              }, 0);
            }}
          >
            <Pencil size={14} />
            {t('tag.rename')}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            className="text-danger"
            onSelect={onPromptDelete}
          >
            <Trash2 size={14} />
            {t('tag.delete')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function SidebarVaultItem({
  vault,
  active,
  menuOpen,
  onOpenMenu,
  onCloseMenu,
  onEdit,
  onDelete,
  onSelect,
}: {
  vault: VaultInfo;
  active: boolean;
  menuOpen: boolean;
  onOpenMenu: () => void;
  onCloseMenu: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onSelect: () => void;
}) {
  const t = useT();
  const setRoute = useApp((s) => s.setRoute);
  const dragged = useDragStore((s) => s.dragged);
  const [hovered, setHovered] = useState(false);
  const canDrop = dragged ? canDropOnVault(vault, dragged.items) : false;

  useEffect(() => {
    if (!dragged) setHovered(false);
  }, [dragged]);

  return (
    <div
      className={cn('group/vault relative', dragged && !canDrop && 'cursor-not-allowed')}
      data-testid="sidebar-vault"
      data-vault-id={vault.id}
      onContextMenu={(e) => {
        e.preventDefault();
        onOpenMenu();
      }}
      onDragOver={(e) => {
        if (!dragged) return;
        if (!canDrop) {
          e.dataTransfer.dropEffect = 'none';
          return;
        }
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        setHovered(true);
      }}
      onDragLeave={(e) => { const next = e.relatedTarget as Node | null; if (next && e.currentTarget.contains(next)) return; setHovered(false); }}
      onDrop={(e) => {
        if (!dragged) return;
        e.preventDefault();
        setHovered(false);
        if (!canDrop) return;
        void runBulk('move', { toVaultId: vault.id }, dragged.items);
        useDragStore.getState().endDrag();
      }}
    >
      <NavItem
        active={active}
        label={vault.name}
        icon={<VaultIcon icon={vault.icon} color={vault.color} size={16} />}
        onClick={onSelect}
        className={cn(
          hovered && canDrop && 'ring-2 ring-inset ring-accent bg-accent/10',
          dragged && !canDrop && 'cursor-not-allowed text-muted/70 hover:bg-transparent'
        )}
        right={
          <span className="flex w-8 justify-end group-hover/vault:invisible">
            {vault.shared ? (
              <Users size={13} className="text-muted" aria-label={t("sidebar.sharedVault")} />
            ) : (
              <span className="text-xs text-muted">{vault.itemCount}</span>
            )}
          </span>
        }
      />
      <DropdownMenu open={menuOpen} onOpenChange={(o) => (o ? onOpenMenu() : onCloseMenu())}>
        <DropdownMenuTrigger asChild>
          <button
            aria-label={`${t('vault.more')}: ${vault.name}`}
            className={cn(
              'absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted hover:bg-active hover:text-fg',
              menuOpen ? 'visible' : 'invisible group-hover/vault:visible'
            )}
          >
            <MoreHorizontal size={14} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem onSelect={onEdit}>
            <Pencil size={14} />
            {t('vault.edit')}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => setRoute({ name: 'settings', tab: `vault:${vault.id}` })}>
            <SettingsIcon size={14} />
            {t('nav.settings')}…
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem className="text-danger" onSelect={onDelete}>
            <Trash2 size={14} />
            {t('vault.delete')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

export function Sidebar() {
  const { route, setRoute, account, lock, theme, setTheme, toggleSidebar, refreshItems, openModal, modal } = useApp();
  const { vaults, tags } = useVaultsAndTags();
  const t = useT(); const L = (label: string) => t(label as Key); // feature sidebar labels are i18n keys
  const toggleTheme = () => { const next = theme === 'dark' ? 'light' : 'dark'; setTheme(next); if (useSettings.getState().settings) void useSettings.getState().save({ theme: next }).catch(() => {}); };
  const go = (filter: Filter) => setRoute({ name: 'items', filter: { sort: 'updated', ...filter } });
  const [newVault, setNewVault] = useState(false);
  // `account` is the account UUID: show a friendly name instead (first vault, else Personal).
  const accountName = vaults.find((v) => v.name === 'Personal')?.name ?? vaults[0]?.name ?? 'Personal';
  void account;
  const [editV, setEditV] = useState<VaultInfo | null>(null); const [delV, setDelV] = useState<VaultInfo | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);

  // Tag management state
  const [menuForTag, setMenuForTag] = useState<string | null>(null);
  const [editingTag, setEditingTag] = useState<string | null>(null);
  const [mergePrompt, setMergePrompt] = useState<{ oldTag: string; newTag: string } | null>(null);
  const [delTag, setDelTag] = useState<string | null>(null);

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
            <DropdownMenuLabel>{t("nav.vaults")}</DropdownMenuLabel>
            {(vaults.length ? vaults : [{ id: '', name: accountName, color: null, icon: null }]).map((v) => <DropdownMenuItem key={v.id || 'default'} onSelect={() => v.id && go({ kind: 'vault', vaultId: v.id })}><VaultIcon icon={v.icon} color={v.color} size={18} />{v.name}{v.name === accountName && <Check size={14} className="ml-auto text-accent" />}</DropdownMenuItem>)}
            <DropdownMenuItem onSelect={() => setNewVault(true)}><Plus size={14} />{t("sidebar.newVaultEllipsis")}</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={toggleTheme}>{theme === 'dark' ? <Sun size={14} /> : <Moon size={14} />}{theme === 'dark' ? t("sidebar.lightTheme") : t("sidebar.darkTheme")}</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => setRoute({ name: 'settings' })}><SettingsIcon size={14} />{t('nav.settings')}…</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => { void api.lock().finally(lock); }}><Lock size={14} />{t('cmd.lock')}</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <Tooltip content={t("sidebar.collapse")}><Button variant="ghost" size="iconSm" aria-label={t("sidebar.collapse")} onClick={toggleSidebar}><PanelLeftClose size={15} /></Button></Tooltip>
      </div>

      <nav className="flex-1 overflow-y-auto p-2">
        {extraTop.filter((f) => f.id === 'home').map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} right={<OnboardingBadge />} onClick={() => setRoute({ name: f.route } as Route)} />; })}
        <NavItem active={same(route, { kind: 'all' })} icon={<LayoutGrid size={15} className="text-accent" />} label={t('nav.allItems')} onClick={() => go({ kind: 'all' })} />
        <NavItem active={same(route, { kind: 'favorites' })} icon={<Star size={15} className="fill-warn text-warn" />} label={t('nav.favorites')} onClick={() => go({ kind: 'favorites' })} />
        <NavItem active={route.name === 'watchtower'} icon={<ShieldCheck size={15} className="text-ok" />} label={t('nav.watchtower')} onClick={() => setRoute({ name: 'watchtower' })} />
        <NavItem active={modal?.id === 'password-generator'} icon={<KeyRound size={15} className="text-muted" />} label={t('generator.title')} onClick={() => openModal('password-generator')} />
        {extraTop.filter((f) => !['home', 'watchtower'].includes(f.id)).map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} onClick={() => setRoute({ name: f.route } as Route)} />; })}

        <SectionHeader label={t('nav.vaults').toUpperCase()} action={<Tooltip content={t("sidebar.newVault")}><button aria-label={t("sidebar.newVault")} className="rounded p-0.5 hover:bg-hover" onClick={() => setNewVault(true)}><Plus size={14} /></button></Tooltip>} />
        {vaults.map((v) => (
          <SidebarVaultItem
            key={v.id}
            vault={v}
            active={same(route, { kind: 'vault', vaultId: v.id })}
            menuOpen={menuFor === v.id}
            onOpenMenu={() => setMenuFor(v.id)}
            onCloseMenu={() => setMenuFor(null)}
            onEdit={() => setEditV(v)}
            onDelete={() => setDelV(v)}
            onSelect={() => go({ kind: 'vault', vaultId: v.id })}
          />
        ))}

        {tags.length > 0 && <SectionHeader label={t('nav.tags').toUpperCase()} />}
        {tags.map((tName) => (
          <SidebarTagItem
            key={tName}
            tag={tName}
            active={same(route, { kind: 'tag', tag: tName })}
            isEditing={editingTag === tName}
            menuOpen={menuForTag === tName}
            onOpenMenu={() => setMenuForTag(tName)}
            onCloseMenu={() => setMenuForTag(null)}
            onStartRename={() => setEditingTag(tName)}
            onCancelRename={() => setEditingTag(null)}
            onRename={async (newTag) => {
              setEditingTag(null);
              if (tags.includes(newTag)) {
                setMergePrompt({ oldTag: tName, newTag });
                return;
              }
              try {
                await api.renameTag({ old: tName, new: newTag });
                toast(t('tag.renamed', { old: tName, new: newTag }));
                refreshItems();
                if (same(route, { kind: 'tag', tag: tName })) {
                  go({ kind: 'tag', tag: newTag });
                }
              } catch (err) {
                toast(String(err), 'danger');
              }
            }}
            onSelect={() => go({ kind: 'tag', tag: tName })}
            onPromptDelete={() => setDelTag(tName)}
          />
        ))}
      </nav>

      <div className="border-t border-border p-2">
        {extraBottom.map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} onClick={() => setRoute({ name: f.route } as Route)} />; })}
        <NavItem active={same(route, { kind: 'archive' })} icon={<Archive size={15} />} label={t('nav.archive')} onClick={() => go({ kind: 'archive' })} />
        <NavItem active={same(route, { kind: 'deleted' })} icon={<Trash2 size={15} />} label={t('nav.deleted')} onClick={() => go({ kind: 'deleted' })} />
      </div>

      <NewVaultDialog
        open={newVault}
        onOpenChange={setNewVault}
        onCreated={(v) => {
          refreshItems();
          go({ kind: 'vault', vaultId: v.id });
        }}
      />
      <EditVaultDialog vault={editV} onOpenChange={(o) => !o && setEditV(null)} onSaved={() => { toast(t('vault.saved')); refreshItems(); }} />
      <DeleteVaultDialog vault={delV} isLast={vaults.length <= 1} onOpenChange={(o) => !o && setDelV(null)}
        onDeleted={(id) => { toast(t('vault.deleted', { vault: delV?.name ?? '' })); if (same(route, { kind: 'vault', vaultId: id })) go({ kind: 'all' }); useApp.getState().selectItem(null); refreshItems(); }} />

      <MergeTagDialog
        prompt={mergePrompt}
        onOpenChange={(o) => !o && setMergePrompt(null)}
        onMerged={(oldTag, newTag) => {
          toast(t('tag.merged', { old: oldTag, new: newTag }));
          refreshItems();
          if (same(route, { kind: 'tag', tag: oldTag })) {
            go({ kind: 'tag', tag: newTag });
          }
        }}
      />
      <DeleteTagDialog
        tag={delTag}
        onOpenChange={(o) => !o && setDelTag(null)}
        onDeleted={(deletedTag, res: BulkResult) => {
          if (same(route, { kind: 'tag', tag: deletedTag })) {
            go({ kind: 'all' });
          }
          refreshItems();
          const lang = useI18n.getState().lang;
          toast(translate(lang, 'tag.deleted', { tag: deletedTag }), {
            duration: 5000,
            action: {
              label: translate(lang, 'bulk.undo'),
              altText: translate(lang, 'bulk.undo'),
              onClick: async () => {
                try {
                  await api.bulkItems({ req: { action: 'add_tag', tag: deletedTag, items: res.items } });
                  toast(translate(lang, 'bulk.undone'));
                  refreshItems();
                } catch (e) {
                  toast(String(e), 'danger');
                }
              },
            },
          });
        }}
      />
    </aside>
  );
}
