import { forwardRef, useRef, useState, useEffect } from 'react';
import { ArrowLeft, ArrowRight, Bookmark, KeyRound, Plus, Search } from 'lucide-react';
import { useApp } from '../lib/store';
import { useItemsUI } from '../features/items';
import { useVaultsAndTags } from '../lib/data';
import { useSavedSearches } from '../features/items/savedSearches';
import { Button, Kbd, Tooltip, toast } from '../ui';
import { useT, type Key } from '../i18n';
import { cn } from '../lib/utils';
import { SaveSearchPopover } from './SaveSearchPopover';
import {
  detectActiveOperator,
  applySuggestion,
  useSearchSuggestionsList,
  SearchSuggestions,
} from './SearchSuggestions';

export const Toolbar = forwardRef<HTMLInputElement>(function Toolbar(_, ref) {
  const { openModal, route, setRoute } = useApp();
  const { query, setQuery, category } = useItemsUI();
  const { vaults, tags } = useVaultsAndTags();
  const { savePopoverOpen, setSavePopoverOpen, saveSearch } = useSavedSearches();
  const t = useT();
  const isMac = typeof navigator !== 'undefined' && /Mac/.test(navigator.platform);

  const routeFilter = route.name === 'items' ? route.filter : null;
  const hasQuery = Boolean(query && query.trim());
  const hasCategory = Boolean(category);
  const hasVault = Boolean(routeFilter?.kind === 'vault' && routeFilter.vaultId);
  const hasTag = Boolean(routeFilter?.kind === 'tag' && routeFilter.tag);
  const canSave = hasQuery || hasCategory || hasVault || hasTag;

  const defaultSearchName =
    (hasQuery && query.trim()) ||
    (hasTag && routeFilter?.tag) ||
    (hasVault && vaults.find((v) => v.id === routeFilter?.vaultId)?.name) ||
    (hasCategory && category ? t(`category.${category}` as Key) : '') ||
    '';

  const internalRef = useRef<HTMLInputElement | null>(null);
  const [cursorPos, setCursorPos] = useState<number | null>(null);
  const [isOpen, setIsOpen] = useState(true);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const activeOperator = isOpen ? detectActiveOperator(query, cursorPos) : null;
  const suggestions = useSearchSuggestionsList(activeOperator, tags, vaults);

  // Reset selectedIndex when suggestions change
  useEffect(() => {
    setSelectedIndex(0);
  }, [suggestions.length, activeOperator?.op, activeOperator?.prefix]);

  const updateCursor = (el: HTMLInputElement) => {
    setCursorPos(el.selectionStart);
  };

  const handleSelect = (val: string) => {
    if (!activeOperator) return;
    const newQuery = applySuggestion(query, activeOperator, val);
    setQuery(newQuery);
    setIsOpen(false);
    if (route.name !== 'items') {
      setRoute({ name: 'items', filter: { kind: 'all', sort: 'updated' } });
    }
    setTimeout(() => {
      internalRef.current?.focus();
    }, 0);
  };

  return (
    <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border bg-panel px-3">
      <Button variant="ghost" size="iconSm" aria-label={t("toolbar.back")} onClick={() => history.back()}>
        <ArrowLeft size={15} />
      </Button>
      <Button variant="ghost" size="iconSm" aria-label={t("toolbar.forward")} onClick={() => history.forward()}>
        <ArrowRight size={15} />
      </Button>

      <div className="relative flex-1">
        <label className="flex h-8 w-full items-center gap-2 rounded-lg border border-border bg-bg px-2.5 focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/40">
          <Search size={15} className="text-muted" />
          <input
            ref={(node) => {
              internalRef.current = node;
              if (typeof ref === 'function') ref(node);
              else if (ref) (ref as React.MutableRefObject<HTMLInputElement | null>).current = node;
            }}
            aria-label={t("toolbar.search")}
            value={query}
            placeholder={t("toolbar.searchPlaceholder")}
            onChange={(e) => {
              setQuery(e.target.value);
              setIsOpen(true);
              updateCursor(e.target);
              if (route.name !== 'items') {
                setRoute({ name: 'items', filter: { kind: 'all', sort: 'updated' } });
              }
            }}
            onClick={(e) => updateCursor(e.currentTarget)}
            onKeyUp={(e) => updateCursor(e.currentTarget)}
            onBlur={() => {
              // Delay closing so onMouseDown on suggestions can fire
              setTimeout(() => setIsOpen(false), 150);
            }}
            onFocus={(e) => {
              setIsOpen(true);
              updateCursor(e.currentTarget);
            }}
            onKeyDown={(e) => {
              if (suggestions.length > 0 && isOpen) {
                if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  setSelectedIndex((i) => (i + 1) % suggestions.length);
                  return;
                }
                if (e.key === 'ArrowUp') {
                  e.preventDefault();
                  setSelectedIndex((i) => (i - 1 + suggestions.length) % suggestions.length);
                  return;
                }
                if (e.key === 'Enter' || e.key === 'Tab') {
                  e.preventDefault();
                  if (suggestions[selectedIndex]) {
                    handleSelect(suggestions[selectedIndex].value);
                  }
                  return;
                }
                if (e.key === 'Escape') {
                  e.preventDefault();
                  setIsOpen(false);
                  return;
                }
              }

              if (e.key === 'Escape') {
                setQuery('');
                (e.target as HTMLInputElement).blur();
              }
              if (e.key === 'ArrowDown') {
                (document.querySelector('[data-item-list]') as HTMLElement | null)?.focus();
              }
            }}
            className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted"
          />
          <Kbd>{isMac ? '⌘' : 'Ctrl'}</Kbd>
          <Kbd>F</Kbd>
          <Tooltip content={t('savedSearch.save')}>
            <button
              type="button"
              disabled={!canSave}
              aria-label={t('savedSearch.save')}
              data-testid="save-search-button"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                if (canSave) setSavePopoverOpen(!savePopoverOpen);
              }}
              className={cn(
                'rounded p-1 text-muted transition-colors hover:text-fg focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
                !canSave && 'cursor-not-allowed opacity-30 hover:text-muted',
                savePopoverOpen && 'text-accent'
              )}
            >
              <Bookmark size={15} />
            </button>
          </Tooltip>
        </label>

        {savePopoverOpen && canSave && (
          <SaveSearchPopover
            initialName={defaultSearchName}
            onSave={(name) => {
              saveSearch({
                name,
                query: hasQuery ? query.trim() : undefined,
                category: hasCategory ? category : undefined,
                vaultId: hasVault ? routeFilter?.vaultId : undefined,
                tag: hasTag ? routeFilter?.tag : undefined,
              });
              toast(t('savedSearch.saved'));
            }}
            onCancel={() => setSavePopoverOpen(false)}
          />
        )}

        {isOpen && activeOperator && (
          <SearchSuggestions
            active={activeOperator}
            tags={tags}
            vaults={vaults}
            selectedIndex={selectedIndex}
            onSelect={(item) => handleSelect(item.value)}
          />
        )}
      </div>
      <Tooltip content={t('generator.title')}>
        <Button variant="ghost" size="iconSm" aria-label={t('generator.title')} onClick={() => openModal('password-generator')}>
          <KeyRound size={15} />
        </Button>
      </Tooltip>
      <Button variant="link" size="sm" className="px-2" onClick={() => setRoute({ name: 'settings', tab: 'help' })}>
        {t("toolbar.help")}
      </Button>
      <Button onClick={() => openModal('new-item')}>
        <Plus size={15} />
        {t('nav.newItem')}
      </Button>
    </div>
  );
});
