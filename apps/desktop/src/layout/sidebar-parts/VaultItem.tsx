import * as React from 'react';
import { useEffect, useState } from 'react';
import { MoreHorizontal, Pencil, Settings as SettingsIcon, Trash2, Users } from 'lucide-react';
import { VaultIcon } from '../../features/home/vaultIcons';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../../ui';
import type { VaultInfo } from '../../api/types';
import { cn } from '../../lib/utils';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';
import { useDragStore, canDropOnVault } from '../../features/items/dragState';
import { runBulk } from '../../features/items/BulkPane';
import { NavItem, RailContext } from './NavItem';
import { Count } from './sections';

export function SidebarVaultItem({
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
