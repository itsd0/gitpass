import * as React from "react";
import { useEffect, useRef, useState } from 'react';
import { Archive, Check, ChevronDown, ChevronRight, KeyRound, LayoutGrid, Lock, Moon, MoreHorizontal, Pencil, PanelLeftClose, PanelLeftOpen, Plus, Search, Settings as SettingsIcon, ShieldCheck, Star, Sun, Tag, Trash2, Users } from 'lucide-react';
import { VaultIcon } from '../features/home/vaultIcons';
import { DeleteVaultDialog, EditVaultDialog } from '../features/home/VaultDialogs';
import { NewVaultDialog } from '../features/home/NewVaultDialog';
import { DeleteTagDialog, MergeTagDialog } from './TagDialogs';
import type { BulkResult, VaultInfo } from '../api/types';
import { api } from '../api/client';
import type { Filter } from '../api/types';
import { features } from '../features/registry';
import { useSidebarCounts, useVaultsAndTags } from '../lib/data';
import { useApp, type Route } from '../lib/store';
import { useItemsUI } from '../features/items';
import { useSavedSearches, matchSavedSearch, type SavedSearch } from '../features/items/savedSearches';
import { cn } from '../lib/utils';
import { useT, useI18n, translate, type Key } from '../i18n';
import { OnboardingBadge } from '../features/onboarding';
import { useSettings } from '../features/settings/useSettings';
import { useDragStore, canDropOnVault } from '../features/items/dragState';
import { runBulk } from '../features/items/BulkPane';
import { Button, Logo, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger, Tooltip, toast } from '../ui';

// True when the sidebar is collapsed to its icon-only rail.
const RailContext = React.createContext(false);

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
  const rail = React.useContext(RailContext);
  if (rail) {
    return (
      <Tooltip content={label} side="right">
        <button
          ref={ref}
          aria-label={label}
          aria-current={active ? 'page' : undefined}
          data-testid="sidebar-rail-item"
          onClick={onClick}
          onContextMenu={onContextMenu}
          onKeyDown={onKeyDown}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
          className={cn(
            'mx-auto flex h-9 w-9 items-center justify-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-accent',
            active ? 'bg-accent/15 text-accent ring-1 ring-inset ring-accent' : 'hover:bg-hover',
            className
          )}
        >
          <span className="flex justify-center" aria-hidden="true">{icon}</span>
        </button>
      </Tooltip>
    );
  }
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

const SECTIONS_KEY = 'gitpass.sidebar.collapsedSections';
export type SectionId = 'vaults' | 'tags' | 'searches';
/** Collapsed Vaults/Tags/Searches sections, remembered on this device only (localStorage, never written to the vault). */
function loadCollapsed(): Record<SectionId, boolean> {
  try {
    const raw = JSON.parse(localStorage.getItem(SECTIONS_KEY) ?? '{}') as Partial<Record<SectionId, unknown>>;
    return { vaults: raw.vaults === true, tags: raw.tags === true, searches: raw.searches === true };
  } catch {
    return { vaults: false, tags: false, searches: false };
  }
}
function useCollapsedSections() {
  const [collapsed, setCollapsed] = useState(loadCollapsed);
  const set = (id: SectionId, value: boolean) => setCollapsed((c) => {
    const next = { ...c, [id]: value };
    try { localStorage.setItem(SECTIONS_KEY, JSON.stringify(next)); } catch { /* storage unavailable: keep in memory */ }
    return next;
  });
  return [collapsed, set] as const;
}

/** Muted right-aligned count. Hidden from the accessible name so rows keep their plain label. */
function Count({ n, named }: { n: number; named?: boolean }) {
  return <span className="text-xs text-muted" data-testid="sidebar-count" aria-hidden={named ? undefined : true}>{n}</span>;
}

function SectionHeader({ label, name, expanded, onToggle, action }: { label: string; name: string; expanded: boolean; onToggle: (open: boolean) => void; action?: React.ReactNode }) {
  const t = useT();
  const rail = React.useContext(RailContext);
  if (rail) return <div role="separator" aria-label={label} className="mx-2 my-2 border-t border-border" />;
  return (
    <div className="mt-5 flex items-center justify-between pb-1 pr-2.5 text-[11px] font-semibold tracking-wider text-muted">
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={t(expanded ? 'sidebar.collapseSection' : 'sidebar.expandSection', { section: name })}
        onClick={() => onToggle(!expanded)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft' && expanded) { e.preventDefault(); onToggle(false); }
          else if (e.key === 'ArrowRight' && !expanded) { e.preventDefault(); onToggle(true); }
        }}
        className="flex items-center gap-1 rounded px-2.5 py-0.5 font-semibold tracking-wider hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
      >
        <ChevronRight size={12} className={cn('transition-transform duration-150 motion-reduce:transition-none', expanded && 'rotate-90')} />
        {label}
      </button>
      {action}
    </div>
  );
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
  count,
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
  count?: number;
}) {
  const t = useT();
  const rail = React.useContext(RailContext);
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
        right={count === undefined ? undefined : <span className="flex w-8 justify-end group-hover/tag:invisible"><Count n={count} /></span>}
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
              rail && !menuOpen ? 'hidden' : menuOpen ? 'visible' : 'invisible group-hover/tag:visible focus:visible'
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

function SidebarSavedSearchItem({
  search,
  active,
  isEditing,
  menuOpen,
  count,
  isMissingFilter,
  onOpenMenu,
  onCloseMenu,
  onStartRename,
  onCancelRename,
  onRename,
  onSelect,
  onDelete,
}: {
  search: SavedSearch;
  active: boolean;
  isEditing: boolean;
  menuOpen: boolean;
  count?: number;
  isMissingFilter: boolean;
  onOpenMenu: () => void;
  onCloseMenu: () => void;
  onStartRename: () => void;
  onCancelRename: () => void;
  onRename: (newName: string) => void;
  onSelect: () => void;
  onDelete: () => void;
}) {
  const t = useT();
  const rail = React.useContext(RailContext);
  const [val, setVal] = useState(search.name);
  const inputRef = useRef<HTMLInputElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const committingRef = useRef(false);

  useEffect(() => {
    if (isEditing) {
      setVal(search.name);
      committingRef.current = false;
      setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 0);
    }
  }, [isEditing, search.name]);

  const commit = () => {
    committingRef.current = true;
    const trimmed = val.trim();
    if (!trimmed || trimmed === search.name) {
      onCancelRename();
      committingRef.current = false;
      buttonRef.current?.focus();
      return;
    }
    onRename(trimmed);
    committingRef.current = false;
    buttonRef.current?.focus();
  };

  if (isEditing) {
    return (
      <form
        onSubmit={(e) => {
          e.preventDefault();
          commit();
        }}
        className="flex w-full items-center gap-2 rounded-md bg-active px-2 py-1 text-[14px]"
        data-testid="sidebar-saved-search-rename"
      >
        <Search size={14} className="shrink-0 text-accent" />
        <input
          ref={inputRef}
          type="text"
          value={val}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              commit();
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
          aria-label={t('savedSearch.renameAria', { name: search.name })}
        />
      </form>
    );
  }

  return (
    <div
      className="group/search relative"
      data-testid="sidebar-saved-search"
      data-search-id={search.id}
      onContextMenu={(e) => {
        e.preventDefault();
        onOpenMenu();
      }}
    >
      <NavItem
        ref={buttonRef}
        active={active}
        icon={<Search size={14} className={cn(active ? 'text-accent' : isMissingFilter ? 'text-muted' : 'text-accent/80')} />}
        label={search.name}
        onClick={onSelect}
        className={cn(isMissingFilter && 'opacity-60 text-muted')}
        right={
          count === undefined ? undefined : (
            <span className="flex w-8 justify-end group-hover/search:invisible">
              <Count n={count} />
            </span>
          )
        }
        onKeyDown={(e) => {
          if (e.key === 'F2') {
            e.preventDefault();
            onStartRename();
          } else if (e.key === 'Delete' || e.key === 'Backspace') {
            e.preventDefault();
            onDelete();
          } else if ((e.shiftKey && e.key === 'F10') || e.key === 'ContextMenu') {
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
            aria-label={`${t('savedSearch.more')}: ${search.name}`}
            className={cn(
              'absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted hover:bg-active hover:text-fg',
              rail && !menuOpen ? 'hidden' : menuOpen ? 'visible' : 'invisible group-hover/search:visible focus:visible'
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
            {t('savedSearch.rename')}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            className="text-danger"
            onSelect={onDelete}
          >
            <Trash2 size={14} />
            {t('savedSearch.delete')}
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
  const rail = React.useContext(RailContext);
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
              <Count n={vault.itemCount} named />
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
              rail && !menuOpen ? 'hidden' : menuOpen ? 'visible' : 'invisible group-hover/vault:visible'
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
  const { route, setRoute, account, lock, theme, setTheme, toggleSidebar, sidebarCollapsed: rail, refreshItems, openModal, modal } = useApp();
  const { vaults, tags } = useVaultsAndTags();
  const counts = useSidebarCounts();
  const [collapsed, setSectionCollapsed] = useCollapsedSections();
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

  // Saved searches state
  const { searches, activeSearchId, setActiveSearchId, renameSearch, deleteSearch, restoreSearch } = useSavedSearches();
  const [editingSearchId, setEditingSearchId] = useState<string | null>(null);
  const [menuForSearch, setMenuForSearch] = useState<string | null>(null);

  const handleSelectSearch = (s: SavedSearch) => {
    const hasVault = s.vaultId && vaults.some((v) => v.id === s.vaultId);
    const hasTag = s.tag && tags.includes(s.tag);

    useItemsUI.getState().setQuery(s.query ?? '');
    useItemsUI.getState().setCategory(s.category ?? null);

    let filter: Filter;
    if (hasVault) {
      filter = { kind: 'vault', vaultId: s.vaultId, ...(hasTag ? { tag: s.tag } : {}), sort: 'updated' };
    } else if (hasTag) {
      filter = { kind: 'tag', tag: s.tag, sort: 'updated' };
    } else {
      filter = { kind: 'all', sort: 'updated' };
    }

    setRoute({ name: 'items', filter });
    setActiveSearchId(s.id);
  };

  const isSearchActive = (s: SavedSearch): boolean => {
    if (activeSearchId === s.id) return true;
    if (route.name !== 'items') return false;
    const currentQuery = useItemsUI.getState().query.trim();
    const currentCategory = useItemsUI.getState().category;
    if (currentQuery !== (s.query?.trim() ?? '')) return false;
    if (currentCategory !== (s.category ?? null)) return false;
    const hasVault = s.vaultId && vaults.some((v) => v.id === s.vaultId);
    const hasTag = s.tag && tags.includes(s.tag);
    if (hasVault) {
      if (route.filter.kind !== 'vault' || route.filter.vaultId !== s.vaultId) return false;
      if (hasTag && route.filter.tag !== s.tag) return false;
    } else if (hasTag) {
      if (route.filter.kind !== 'tag' || route.filter.tag !== s.tag) return false;
    } else {
      if (route.filter.kind !== 'all') return false;
    }
    return true;
  };

  const handleDeleteSearch = (s: SavedSearch) => {
    const idx = searches.findIndex((item) => item.id === s.id);
    const deleted = deleteSearch(s.id);
    if (!deleted) return;
    const lang = useI18n.getState().lang;
    toast(translate(lang, 'savedSearch.deleted', { name: deleted.name }), {
      duration: 5000,
      action: {
        label: translate(lang, 'bulk.undo'),
        altText: translate(lang, 'bulk.undo'),
        onClick: () => {
          restoreSearch(deleted, idx);
          toast(translate(lang, 'bulk.undone'));
        },
      },
    });
  };

  const extraTop = features.filter((f) => f.sidebar?.section === 'top' && f.id !== 'items' && f.route);
  const extraBottom = features.filter((f) => f.sidebar?.section === 'bottom' && f.route);

  return (
    <RailContext.Provider value={rail}>
    <aside className={cn('flex h-full flex-col bg-sidebar', rail && 'w-14')} data-testid="sidebar" data-collapsed={rail ? 'true' : 'false'}>
      <div className={cn('flex items-center gap-1 border-b border-border p-2.5', rail && 'flex-col px-1.5')}>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            {rail ? (
              <button aria-label={`gitpass: ${accountName}`} className="rounded-md p-1 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"><Logo size={30} /></button>
            ) : (
            <button className="flex min-w-0 flex-1 items-center gap-2.5 rounded-md px-1.5 py-1.5 hover:bg-hover">
              <Logo size={30} />
              <span className="flex min-w-0 flex-1 flex-col text-left leading-tight"><span className="truncate font-medium">{accountName}</span><span className="truncate text-[11px] text-muted">gitpass</span></span><ChevronDown size={14} className="text-muted" />
            </button>
            )}
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
        {rail
          ? <Tooltip content={t("sidebar.expand")} side="right"><Button variant="ghost" size="iconSm" aria-label={t("sidebar.expand")} aria-expanded={false} onClick={toggleSidebar}><PanelLeftOpen size={15} /></Button></Tooltip>
          : <Tooltip content={t("sidebar.collapse")}><Button variant="ghost" size="iconSm" aria-label={t("sidebar.collapse")} aria-expanded onClick={toggleSidebar}><PanelLeftClose size={15} /></Button></Tooltip>}
      </div>

      <nav className={cn('flex-1 overflow-y-auto p-2', rail && 'flex flex-col gap-0.5 overflow-x-hidden px-1.5')}>
        {extraTop.filter((f) => f.id === 'home').map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} right={<OnboardingBadge />} onClick={() => setRoute({ name: f.route } as Route)} />; })}
        <NavItem active={same(route, { kind: 'all' })} icon={<LayoutGrid size={15} className="text-accent" />} label={t('nav.allItems')} right={<Count n={counts.all} />} onClick={() => go({ kind: 'all' })} />
        <NavItem active={same(route, { kind: 'favorites' })} icon={<Star size={15} className="fill-warn text-warn" />} label={t('nav.favorites')} right={<Count n={counts.favorites} />} onClick={() => go({ kind: 'favorites' })} />
        <NavItem active={route.name === 'watchtower'} icon={<ShieldCheck size={15} className="text-ok" />} label={t('nav.watchtower')} onClick={() => setRoute({ name: 'watchtower' })} />
        <NavItem active={modal?.id === 'password-generator'} icon={<KeyRound size={15} className="text-muted" />} label={t('generator.title')} onClick={() => openModal('password-generator')} />
        {extraTop.filter((f) => !['home', 'watchtower'].includes(f.id)).map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} onClick={() => setRoute({ name: f.route } as Route)} />; })}

        <SectionHeader label={t('nav.vaults').toUpperCase()} name={t('nav.vaults')} expanded={!collapsed.vaults} onToggle={(o) => setSectionCollapsed('vaults', !o)} action={<Tooltip content={t("sidebar.newVault")}><button aria-label={t("sidebar.newVault")} className="rounded p-0.5 hover:bg-hover" onClick={() => setNewVault(true)}><Plus size={14} /></button></Tooltip>} />
        {(rail || !collapsed.vaults) && vaults.map((v) => (
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

        {tags.length > 0 && <SectionHeader label={t('nav.tags').toUpperCase()} name={t('nav.tags')} expanded={!collapsed.tags} onToggle={(o) => setSectionCollapsed('tags', !o)} />}
        {(rail || !collapsed.tags) && tags.map((tName) => (
          <SidebarTagItem
            key={tName}
            tag={tName}
            count={counts.tags[tName] ?? 0}
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

        {searches.length > 0 && (
          rail ? (
            <Tooltip content={searches.map((s) => s.name).join(', ')} side="right">
              <button
                type="button"
                aria-label={`${t('nav.savedSearches')}: ${searches.map((s) => s.name).join(', ')}`}
                data-testid="sidebar-rail-saved-searches"
                onClick={() => handleSelectSearch(searches.find(isSearchActive) ?? searches[0])}
                className={cn(
                  'flex h-9 w-9 items-center justify-center rounded-md transition-colors',
                  searches.some(isSearchActive)
                    ? 'bg-accent/15 text-accent ring-1 ring-inset ring-accent'
                    : 'text-muted hover:bg-hover hover:text-fg'
                )}
              >
                <Search size={16} />
              </button>
            </Tooltip>
          ) : (
            <>
              <SectionHeader
                label={t('nav.savedSearches').toUpperCase()}
                name={t('nav.savedSearches')}
                expanded={!collapsed.searches}
                onToggle={(o) => setSectionCollapsed('searches', !o)}
              />
              {!collapsed.searches &&
                searches.map((s) => (
                  <SidebarSavedSearchItem
                    key={s.id}
                    search={s}
                    active={isSearchActive(s)}
                    isEditing={editingSearchId === s.id}
                    menuOpen={menuForSearch === s.id}
                    count={counts.items ? counts.items.filter((it) => matchSavedSearch(it, s)).length : 0}
                    isMissingFilter={Boolean(
                      (s.vaultId && !vaults.some((v) => v.id === s.vaultId)) ||
                      (s.tag && !tags.includes(s.tag))
                    )}
                    onOpenMenu={() => setMenuForSearch(s.id)}
                    onCloseMenu={() => setMenuForSearch(null)}
                    onStartRename={() => setEditingSearchId(s.id)}
                    onCancelRename={() => setEditingSearchId(null)}
                    onRename={(newName) => {
                      renameSearch(s.id, newName);
                      setEditingSearchId(null);
                    }}
                    onSelect={() => handleSelectSearch(s)}
                    onDelete={() => handleDeleteSearch(s)}
                  />
                ))}
            </>
          )
        )}
      </nav>

      <div className={cn('border-t border-border p-2', rail && 'flex flex-col gap-0.5 px-1.5')}>
        {extraBottom.map((f) => { const I = f.sidebar!.icon; return <NavItem key={f.id} active={route.name === f.route} icon={<I size={15} />} label={L(f.sidebar!.label)} onClick={() => setRoute({ name: f.route } as Route)} />; })}
        <NavItem active={same(route, { kind: 'archive' })} icon={<Archive size={15} />} label={t('nav.archive')} right={<Count n={counts.archive} />} onClick={() => go({ kind: 'archive' })} />
        <NavItem active={same(route, { kind: 'deleted' })} icon={<Trash2 size={15} />} label={t('nav.deleted')} right={<Count n={counts.deleted} />} onClick={() => go({ kind: 'deleted' })} />
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
    </RailContext.Provider>
  );
}
