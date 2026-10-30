import { useEffect, useMemo, useRef, useState } from 'react';
import { Archive as VaultsTabIcon, Laptop, Lock, Info, RefreshCw, Search, SlidersHorizontal, Terminal } from 'lucide-react';
import { cn } from '../../lib/utils';
import { Button, Input } from '../../ui';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';
import { useSettings } from './useSettings';
import { SyncPanel } from './SyncPanel';
import { searchSettings, type SearchResult } from './searchIndex';
import { GeneralTab } from './tabs/GeneralTab';
import { SecurityTab } from './tabs/SecurityTab';
import { VaultsTab, VaultPanel } from './tabs/VaultsTab';
import { DevicesTab } from './tabs/DevicesTab';
import { SshAgentTab } from './tabs/SshAgentTab';
import { AboutTab } from './tabs/AboutTab';

const TABS = [
  { id: 'general', icon: SlidersHorizontal },
  { id: 'security', icon: Lock },
  { id: 'vaults', icon: VaultsTabIcon },
  { id: 'devices', icon: Laptop },
  { id: 'sync', icon: RefreshCw },
  { id: 'ssh', icon: Terminal },
  { id: 'about', icon: Info },
] as const;

type Tab = (typeof TABS)[number]['id'];

const Sync = SyncPanel;


export function SettingsPage({ initialTab }: { initialTab?: string } = {}) {
  const t = useT();
  const route = useApp((s) => s.route);
  const setRoute = useApp((s) => s.setRoute);
  const { settings, load, error } = useSettings();
  const rawTab = initialTab ?? (route.name === 'settings' ? route.tab : undefined);
  const vaultId = rawTab?.startsWith('vault:') ? rawTab.slice(6) : null;
  // 'help' (toolbar) → About; unknown → General.
  const fromRoute = rawTab === 'help' ? 'about' : rawTab;
  const tab: Tab = (TABS.some((x) => x.id === fromRoute) ? fromRoute : 'general') as Tab;
  const [local, setLocal] = useState<Tab>(tab);
  useEffect(() => setLocal(tab), [tab]);
  useEffect(() => {
    if (!settings) load();
  }, [settings, load]);

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const results = useMemo(() => searchSettings(searchQuery, t), [searchQuery, t]);

  // Global ⌘F / Ctrl+F shortcut focused when settings is open
  useEffect(() => {
    const onGlobalKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener('keydown', onGlobalKeyDown);
    return () => window.removeEventListener('keydown', onGlobalKeyDown);
  }, []);

  function selectResult(item: SearchResult) {
    setLocal(item.tab);
    setRoute({ name: 'settings', tab: item.tab });
    setHighlightedId(item.id);
    setTimeout(() => {
      const el =
        document.getElementById(item.id) ||
        document.querySelector(`[data-setting-id="${item.id}"]`);
      if (el) {
        const reduceMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches;
        el.scrollIntoView({ behavior: reduceMotion ? 'auto' : 'smooth', block: 'nearest' });
      }
    }, 50);
  }

  // Clear highlight after 1.5s
  useEffect(() => {
    if (highlightedId) {
      const timer = setTimeout(() => setHighlightedId(null), 1500);
      return () => clearTimeout(timer);
    }
  }, [highlightedId]);

  function onSearchKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Escape') {
      e.preventDefault();
      setSearchQuery('');
      setSelectedIndex(0);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((idx) => (results.length > 0 ? Math.min(idx + 1, results.length - 1) : 0));
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((idx) => Math.max(idx - 1, 0));
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (results[selectedIndex]) {
        selectResult(results[selectedIndex]);
      }
      return;
    }
  }

  function onTabListKeyDown(e: React.KeyboardEvent) {
    const curIdx = TABS.findIndex((x) => x.id === local);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const nextIdx = (curIdx + 1) % TABS.length;
      const nextId = TABS[nextIdx].id;
      setLocal(nextId);
      setRoute({ name: 'settings', tab: nextId });
      tabRefs.current[nextIdx]?.focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const prevIdx = (curIdx - 1 + TABS.length) % TABS.length;
      const prevId = TABS[prevIdx].id;
      setLocal(prevId);
      setRoute({ name: 'settings', tab: prevId });
      tabRefs.current[prevIdx]?.focus();
    }
  }

  const body = {
    general: <GeneralTab highlightedId={highlightedId} />,
    security: <SecurityTab highlightedId={highlightedId} />,
    vaults: <VaultsTab highlightedId={highlightedId} />,
    devices: <DevicesTab highlightedId={highlightedId} />,
    sync: <Sync highlightedId={highlightedId} />,
    ssh: <SshAgentTab highlightedId={highlightedId} />,
    about: <AboutTab highlightedId={highlightedId} />,
  }[local];

  return (
    <div className="h-full overflow-y-auto" data-testid="settings-scroll">
    <div className="mx-auto flex max-w-4xl gap-8 p-8">
      <nav aria-label={t('settings.title')} className="w-52 shrink-0">
        <h1 className="mb-3 text-2xl font-semibold">{t('settings.title')}</h1>
        <div className="relative mb-3">
          <Search
            className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted"
            aria-hidden
          />
          <Input
            ref={searchRef}
            type="search"
            data-testid="settings-search-input"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setSelectedIndex(0);
            }}
            onKeyDown={onSearchKeyDown}
            placeholder={t('settings.searchPlaceholder')}
            aria-label={t('settings.searchAria')}
            className="h-8 pl-8 pr-2 text-xs focus-visible:ring-accent"
          />
        </div>

        {searchQuery.trim() !== '' ? (
          results.length > 0 ? (
            <div
              role="listbox"
              aria-label={t('settings.searchAria')}
              className="space-y-1"
              data-testid="settings-search-results"
            >
              {results.map((r, i) => (
                <button
                  key={r.id}
                  role="option"
                  aria-selected={selectedIndex === i}
                  onClick={() => selectResult(r)}
                  className={cn(
                    'flex w-full flex-col items-start gap-0.5 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
                    selectedIndex === i
                      ? 'bg-hover font-medium ring-1 ring-accent/60'
                      : 'text-muted hover:bg-hover hover:text-fg'
                  )}
                >
                  <span className="font-medium text-fg line-clamp-1">{r.label}</span>
                  <span className="text-[10px] text-muted">{r.tabName}</span>
                </button>
              ))}
            </div>
          ) : (
            <div
              role="status"
              className="flex flex-col items-center justify-center p-4 text-center"
              data-testid="settings-search-empty"
            >
              <p className="mb-3 text-xs text-muted leading-relaxed">
                {t('settings.searchEmpty', { q: searchQuery })}
              </p>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs"
                onClick={() => {
                  setSearchQuery('');
                  searchRef.current?.focus();
                }}
              >
                {t('settings.clearSearch')}
              </Button>
            </div>
          )
        ) : (
          <ul
            role="tablist"
            aria-orientation="vertical"
            className="space-y-1"
            onKeyDown={onTabListKeyDown}
          >
            {TABS.map(({ id, icon: I }, idx) => (
              <li key={id}>
                <button
                  ref={(el) => {
                    tabRefs.current[idx] = el;
                  }}
                  role="tab"
                  aria-selected={!vaultId && local === id}
                  onClick={() => {
                    setLocal(id);
                    setRoute({ name: 'settings', tab: id });
                  }}
                  className={cn(
                    'flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
                    !vaultId && local === id
                      ? 'bg-hover font-medium'
                      : 'text-muted hover:bg-hover hover:text-fg'
                  )}
                >
                  <I className="h-4 w-4" aria-hidden />
                  {t(`settings.${id}`)}
                </button>
              </li>
            ))}
          </ul>
        )}
      </nav>
      <section
        role="tabpanel"
        aria-label={vaultId ? t('settings.vault') : t(`settings.${local}`)}
        className="min-w-0 flex-1 rounded-xl border border-border bg-card px-5 py-4"
      >
        {error && (
          <p role="alert" className="mb-4 py-3 text-sm text-danger">
            {t('common.error', { msg: error })}
          </p>
        )}
        {vaultId ? <VaultPanel vaultId={vaultId} /> : body}
      </section>
    </div>
    </div>
  );
}
