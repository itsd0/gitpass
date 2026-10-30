import * as React from 'react';
import { useEffect, useRef, useState } from 'react';
import { MoreHorizontal, Pencil, Search, Trash2 } from 'lucide-react';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../../ui';
import { cn } from '../../lib/utils';
import { useT } from '../../i18n';
import type { SavedSearch } from '../../features/items/savedSearches';
import { NavItem, RailContext } from './NavItem';
import { Count } from './sections';

export function SidebarSavedSearchItem({
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
