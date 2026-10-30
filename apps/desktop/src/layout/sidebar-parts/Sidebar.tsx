import { useState } from 'react';
import { Archive, Check, ChevronDown, KeyRound, LayoutGrid, Lock, Moon, PanelLeftClose, PanelLeftOpen, Plus, Search, Settings as SettingsIcon, ShieldCheck, Star, Sun, Trash2 } from 'lucide-react';
import { VaultIcon } from '../../features/home/vaultIcons';
import { DeleteVaultDialog, EditVaultDialog } from '../../features/home/VaultDialogs';
import { NewVaultDialog } from '../../features/home/NewVaultDialog';
import { DeleteTagDialog, MergeTagDialog } from '../TagDialogs';
import type { BulkResult, VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import type { Filter } from '../../api/types';
import { features } from '../../features/registry';
import { useSidebarCounts, useVaultsAndTags } from '../../lib/data';
import { useApp, type Route } from '../../lib/store';
import { useItemsUI } from '../../features/items';
import { useSavedSearches, matchSavedSearch, type SavedSearch } from '../../features/items/savedSearches';
import { cn } from '../../lib/utils';
import { useT, useI18n, translate, type Key } from '../../i18n';
import { OnboardingBadge } from '../../features/onboarding';
import { useSettings } from '../../features/settings/useSettings';
import { Button, Logo, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger, Tooltip, toast } from '../../ui';
import { NavItem, RailContext } from './NavItem';
import { Count, SectionHeader, useCollapsedSections } from './sections';
import { SidebarTagItem } from './TagItem';
import { SidebarSavedSearchItem } from './SavedSearchItem';
import { SidebarVaultItem } from './VaultItem';

const same = (r: Route, f: Filter) => r.name === 'items' && r.filter.kind === f.kind && r.filter.vaultId === f.vaultId && r.filter.tag === f.tag;

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
