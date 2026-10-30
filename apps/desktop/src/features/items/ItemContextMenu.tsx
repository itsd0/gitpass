import { useEffect, useState } from 'react';
import { Archive, ArchiveRestore, ExternalLink, FolderInput, KeyRound, Pencil, RotateCcw, ShieldCheck, Star, StarOff, Trash2, User } from 'lucide-react';
import { api } from '../../api/client';
import type { ItemDetail, ItemSummary } from '../../api/types';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';
import { useVaultsAndTags } from '../../lib/data';
import { formatCombo } from '../palette/shortcuts';
import { pickFields } from '../quick-access/QuickAccess';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  Kbd,
  toast,
} from '../../ui';
import { copyPrimarySecret, copyTotpCode, copyUsername, errText, openItemWebsite } from './actions';
import { runBulk } from './BulkPane';

/** Where the menu opens (viewport px) and the items it acts on (the right-clicked row, or the whole multi-selection). */
export interface ItemMenuTarget {
  x: number;
  y: number;
  items: ItemSummary[];
}

/** Which of the per-field actions an item offers, from the fields it really has. */
export interface MenuFields {
  username: boolean;
  password: boolean;
  totp: boolean;
  website: boolean;
}
export const NO_FIELDS: MenuFields = { username: false, password: false, totp: false, website: false };

export function menuFields(d: ItemDetail): MenuFields {
  const p = pickFields(d);
  const all = d.sections.flatMap((s) => s.fields);
  const secret = all.find((f) => f.kind === 'concealed');
  return {
    username: !!p.username,
    password: !!secret,
    totp: !!p.totp,
    website: [d.url, ...all.filter((f) => f.kind === 'url').map((f) => f.value)].some((u) => !!u && /^https?:\/\//i.test(u)),
  };
}

const Shortcut = ({ combo }: { combo: string }) => (
  <span className="ml-auto flex items-center gap-0.5 pl-6">
    {formatCombo(combo).map((k) => (
      <Kbd key={k}>{k}</Kbd>
    ))}
  </span>
);

/**
 * Right-click / Shift+F10 menu for the item list. The menu is anchored to a zero-size element at the pointer (or
 * the focused row); Escape closes it and `onClose` hands focus back to the list.
 */
export function ItemContextMenu({ target, onClose }: { target: ItemMenuTarget | null; onClose: () => void }) {
  const t = useT();
  const { vaults } = useVaultsAndTags();
  const [fields, setFields] = useState<MenuFields>(NO_FIELDS);
  const items = target?.items ?? [];
  const n = items.length;
  const one = n === 1 ? items[0] : undefined;
  const key = target ? `${target.x},${target.y},${items.map((i) => i.id).join(',')}` : '';

  // The per-field entries only appear for fields the item has; read them when the menu opens.
  useEffect(() => {
    setFields(NO_FIELDS);
    if (!one || one.deletedAt) return;
    let on = true;
    api.getItem({ vaultId: one.vaultId, itemId: one.id }).then((d) => on && setFields(menuFields(d)), () => {});
    return () => {
      on = false;
    };
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!target || n === 0) return null;
  const deleted = items.every((x) => !!x.deletedAt);
  const archived = items.every((x) => x.archived);
  const allFav = items.every((x) => x.favorite);
  const sameVault = new Set(items.map((x) => x.vaultId)).size === 1 ? items[0].vaultId : null;
  const destinations = vaults.filter((v) => v.id !== sameVault);
  const done = (msg: string, deselect = false) => {
    toast(msg);
    if (deselect) useApp.getState().selectItem(null);
    useApp.getState().refreshItems();
  };
  const ctx = one ? { vaultId: one.vaultId, itemId: one.id } : null;
  const fail = (e: unknown) => toast(errText(e), 'danger');

  const favorite = () =>
    one && ctx
      ? api.setFavorite({ ...ctx, favorite: !one.favorite }).then(() => done(t(one.favorite ? 'bulk.toast.unfavorite' : 'bulk.toast.favorite')), fail)
      : void runBulk(allFav ? 'unfavorite' : 'favorite', {}, items);
  const archive = () =>
    one && ctx
      ? api.archiveItem({ ...ctx, archived: !one.archived }).then(() => done(t(one.archived ? 'detail.toast.unarchived' : 'detail.toast.archived'), true), fail)
      : void runBulk(archived ? 'unarchive' : 'archive', {}, items);
  const moveTo = (toVaultId: string, name: string) =>
    one && ctx
      ? api.moveItem({ ...ctx, toVaultId }).then(() => done(t('detail.toast.movedToVault', { vault: name }), true), fail)
      : void runBulk('move', { toVaultId }, items);
  const trash = () =>
    one && ctx ? api.deleteItem(ctx).then(() => done(t('detail.toast.trash'), true), fail) : useApp.getState().setBulkConfirm('trash');
  const restore = () =>
    one && ctx ? api.restoreItem(ctx).then(() => done(t('detail.toast.restored'), true), fail) : void runBulk('restore', {}, items);
  const purge = () => useApp.getState().setBulkConfirm('purge');

  return (
    <DropdownMenu open onOpenChange={(o) => !o && onClose()}>
      <DropdownMenuTrigger asChild>
        <span aria-hidden="true" data-testid="item-menu-anchor" style={{ position: 'fixed', left: target.x, top: target.y, width: 0, height: 0 }} />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        side="bottom"
        aria-label={t('ctx.menuLabel')}
        data-testid="item-context-menu"
        className="min-w-[230px]"
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          onClose();
        }}
      >
        {deleted ? (
          <>
            <DropdownMenuItem data-testid="ctx-restore" onSelect={restore}>
              <RotateCcw size={14} />
              {n > 1 ? t('ctx.restoreN', { n }) : t('detail.restore')}
            </DropdownMenuItem>
            <DropdownMenuItem danger data-testid="ctx-purge" onSelect={purge}>
              <Trash2 size={14} />
              {n > 1 ? t('ctx.purgeN', { n }) : t('detail.deletePermanently')}
            </DropdownMenuItem>
          </>
        ) : (
          <>
            {one && (fields.username || fields.password || fields.totp || fields.website) && (
              <>
                {fields.username && (
                  <DropdownMenuItem data-testid="ctx-copy-username" onSelect={() => void copyUsername(one.vaultId, one.id)}>
                    <User size={14} />
                    {t('itemKeys.copyUsername')}
                    <Shortcut combo="mod+shift+c" />
                  </DropdownMenuItem>
                )}
                {fields.password && (
                  <DropdownMenuItem data-testid="ctx-copy-password" onSelect={() => void copyPrimarySecret(one.vaultId, one.id)}>
                    <KeyRound size={14} />
                    {t('itemKeys.copyPassword')}
                    <Shortcut combo="mod+c" />
                  </DropdownMenuItem>
                )}
                {fields.totp && (
                  <DropdownMenuItem data-testid="ctx-copy-totp" onSelect={() => void copyTotpCode(one.vaultId, one.id)}>
                    <ShieldCheck size={14} />
                    {t('itemKeys.copyTotp')}
                    <Shortcut combo="mod+alt+c" />
                  </DropdownMenuItem>
                )}
                {fields.website && (
                  <DropdownMenuItem data-testid="ctx-open-website" onSelect={() => void openItemWebsite(one.vaultId, one.id)}>
                    <ExternalLink size={14} />
                    {t('itemKeys.openWebsite')}
                    <Shortcut combo="mod+shift+o" />
                  </DropdownMenuItem>
                )}
                <DropdownMenuSeparator />
              </>
            )}
            <DropdownMenuItem data-testid="ctx-favorite" onSelect={favorite}>
              {allFav ? <StarOff size={14} /> : <Star size={14} />}
              {n > 1 ? t(allFav ? 'ctx.unfavoriteN' : 'ctx.favoriteN', { n }) : t(allFav ? 'bulk.unfavorite' : 'bulk.favorite')}
            </DropdownMenuItem>
            {one && (
              <DropdownMenuItem data-testid="ctx-edit" onSelect={() => useApp.getState().setEditing(true)}>
                <Pencil size={14} />
                {t('detail.edit')}
                <Shortcut combo="mod+e" />
              </DropdownMenuItem>
            )}
            <DropdownMenuSub>
              <DropdownMenuSubTrigger data-testid="ctx-move" disabled={destinations.length === 0}>
                <FolderInput size={14} />
                {n > 1 ? t('ctx.moveN', { n }) : t('ctx.moveToVault')}
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent>
                {destinations.map((v) => (
                  <DropdownMenuItem key={v.id} onSelect={() => moveTo(v.id, v.name)}>
                    <span className="h-2.5 w-2.5 rounded-full" style={{ background: v.color ?? '#888' }} />
                    {v.name}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuItem data-testid="ctx-archive" onSelect={archive}>
              {archived ? <ArchiveRestore size={14} /> : <Archive size={14} />}
              {n > 1 ? t(archived ? 'ctx.unarchiveN' : 'ctx.archiveN', { n }) : t(archived ? 'detail.unarchive' : 'detail.archive')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem danger data-testid="ctx-trash" onSelect={trash}>
              <Trash2 size={14} />
              {n > 1 ? t('ctx.trashN', { n }) : t('ctx.moveToTrash')}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
