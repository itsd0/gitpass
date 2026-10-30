import * as React from 'react';
import { useEffect, useRef, useState } from 'react';
import { MoreHorizontal, Pencil, Tag, Trash2 } from 'lucide-react';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../../ui';
import { cn } from '../../lib/utils';
import { useT } from '../../i18n';
import { useDragStore } from '../../features/items/dragState';
import { runBulk } from '../../features/items/BulkPane';
import { NavItem, RailContext } from './NavItem';
import { Count } from './sections';

export function SidebarTagItem({
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
