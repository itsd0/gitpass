import { useState } from 'react';
import { Archive, ArchiveRestore, Copy, Download, FolderInput, RotateCcw, Star, StarOff, Tag, Trash2, X } from 'lucide-react';
import { api } from '../../api/client';
import type { BulkAction, BulkRequest, ItemRef } from '../../api/types';
import { useT, useI18n, translate, type Key } from '../../i18n';
import { useApp } from '../../lib/store';
import { useVaultsAndTags } from '../../lib/data';
import { Button, Dialog, DialogContent, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Input, toast } from '../../ui';
import { ExportDialog } from '../export';
import { ItemIcon } from './categories';
import { errText } from './actions';
import { useItemsUI } from './state';
import { readSel, refsFor, selectedIds } from './selection';

const plural = (n: number) => `${n} item${n === 1 ? '' : 's'}`;

/** Run a bulk action on the current selection (multi or single). Clears the selection when items leave the view. */
export async function runBulk(action: BulkAction, extra: Partial<BulkRequest> = {}): Promise<boolean> {
  const items = useItemsUI.getState().items;
  const sIds = selectedIds(readSel());
  const refs = refsFor(items, sIds);
  if (!refs.length) return false;
  const targetItems = items.filter((x) => sIds.includes(x.id));
  try {
    const r = await api.bulkItems({ req: { action, items: refs, ...extra } });
    const lang = useI18n.getState().lang;
    const toastPrefix = translate(lang, `bulk.toast.${action}` as Key);
    const title = `${toastPrefix}: ${plural(r.count)}`;

    let undoFn: (() => Promise<void>) | null = null;
    if (action === 'trash') {
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'restore', items: refs } });
      };
    } else if (action === 'archive') {
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'unarchive', items: refs } });
      };
    } else if (action === 'restore') {
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'trash', items: refs } });
      };
    } else if (action === 'unarchive') {
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'archive', items: refs } });
      };
    } else if (action === 'favorite') {
      const changedRefs = refs.filter((ref) => !targetItems.find((x) => x.id === ref.itemId)?.favorite);
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'unfavorite', items: changedRefs.length ? changedRefs : refs } });
      };
    } else if (action === 'unfavorite') {
      const changedRefs = refs.filter((ref) => targetItems.find((x) => x.id === ref.itemId)?.favorite);
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'favorite', items: changedRefs.length ? changedRefs : refs } });
      };
    } else if (action === 'add_tag' && extra.tag) {
      const tag = extra.tag;
      const changedRefs = refs.filter((ref) => !targetItems.find((x) => x.id === ref.itemId)?.tags.includes(tag));
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'remove_tag', tag, items: changedRefs.length ? changedRefs : refs } });
      };
    } else if (action === 'remove_tag' && extra.tag) {
      const tag = extra.tag;
      const changedRefs = refs.filter((ref) => targetItems.find((x) => x.id === ref.itemId)?.tags.includes(tag));
      undoFn = async () => {
        await api.bulkItems({ req: { action: 'add_tag', tag, items: changedRefs.length ? changedRefs : refs } });
      };
    } else if (action === 'move' && extra.toVaultId) {
      const destItems = r.items;
      const byOrigVault = new Map<string, ItemRef[]>();
      destItems.forEach((destRef, idx) => {
        const origVaultId = refs[idx]?.vaultId;
        if (origVaultId) {
          const list = byOrigVault.get(origVaultId) || [];
          list.push(destRef);
          byOrigVault.set(origVaultId, list);
        }
      });
      undoFn = async () => {
        for (const [origVaultId, group] of byOrigVault.entries()) {
          await api.bulkItems({ req: { action: 'move', toVaultId: origVaultId, items: group } });
        }
      };
    }

    if (undoFn) {
      toast(title, {
        duration: 5000,
        action: {
          label: translate(lang, 'bulk.undo'),
          altText: translate(lang, 'bulk.undo'),
          onClick: async () => {
            try {
              await undoFn!();
              toast(translate(lang, 'bulk.undone'));
              useApp.getState().refreshItems();
            } catch (e) {
              toast(errText(e), 'danger');
            }
          },
        },
      });
    } else {
      toast(title);
    }

    const keep = action === 'favorite' || action === 'unfavorite' || action === 'add_tag' || action === 'remove_tag' || action === 'copy';
    if (!keep) useApp.setState({ selectedItemId: null, multiIds: [], editing: false });
    useApp.getState().refreshItems();
    return true;
  } catch (e) { toast(errText(e), 'danger'); return false; }
}

function viewKind() { const r = useApp.getState().route; return r.name === 'items' ? r.filter.kind : 'all'; }

export function BulkPane() {
  const t = useT();
  const multi = useApp((s) => s.multiIds);
  const route = useApp((s) => s.route);
  const items = useItemsUI((s) => s.items);
  const [exportRefs, setExportRefs] = useState<ItemRef[] | null>(null);
  const { vaults, tags } = useVaultsAndTags();
  const [tag, setTag] = useState('');
  const kind = route.name === 'items' ? route.filter.kind : 'all';
  const sel = items.filter((x) => multi.includes(x.id));
  const n = sel.length;
  const allFav = n > 0 && sel.every((x) => x.favorite);
  const anyFav = sel.some((x) => x.favorite);
  const clear = () => useApp.setState({ multiIds: [] });
  const addTag = async () => { const t = tag.trim(); if (t && await runBulk('add_tag', { tag: t })) setTag(''); };

  return (
    <div className="flex h-full flex-col items-center overflow-y-auto p-8" data-testid="bulk-pane" data-detail-pane tabIndex={-1}>
      <div className="relative mb-4 mt-6 h-16 w-20">
        {sel.slice(0, 3).map((it, i) => (
          <div key={it.id} className="absolute rounded-lg border border-accent/40 bg-card p-1 shadow" style={{ left: i * 14, top: i * 6, zIndex: 3 - i }}><ItemIcon item={it} /></div>
        ))}
      </div>
      <h2 className="text-xl font-semibold" data-testid="bulk-count">{t(n === 1 ? 'bulk.oneSelected' : 'bulk.nSelected', { n })}</h2>
      <button className="mt-1 flex items-center gap-1 text-xs text-muted hover:text-accent" onClick={clear}><X size={12} />{t('bulk.clear')}</button>

      <div className="mt-6 grid w-full max-w-sm gap-2">
        {kind === 'deleted' ? (<>
          <Button onClick={() => void runBulk('restore')}><RotateCcw size={15} />{t('bulk.restore', { n: plural(n) })}</Button>
          <Button variant="danger" onClick={() => useApp.getState().setBulkConfirm('purge')}><Trash2 size={15} />{t('bulk.deletePermanently')}</Button>
        </>) : kind === 'archive' ? (<>
          <Button onClick={() => void runBulk('unarchive')}><ArchiveRestore size={15} />{t('bulk.unarchive', { n: plural(n) })}</Button>
          <Button variant="outline" className="text-danger" onClick={() => useApp.getState().setBulkConfirm('trash')}><Trash2 size={15} />{t('bulk.moveToTrash')}</Button>
        </>) : (<>
          {!allFav && <Button variant="outline" onClick={() => void runBulk('favorite')}><Star size={15} className="text-accent" />{t('bulk.favorite')}</Button>}
          {anyFav && <Button variant="outline" onClick={() => void runBulk('unfavorite')}><StarOff size={15} className="text-accent" />{t('bulk.unfavorite')}</Button>}
          <div className="flex gap-2">
            <Input list="bulk-tags" value={tag} placeholder={t('bulk.addTag')} aria-label="Tag to add" onChange={(e) => setTag(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void addTag(); } }} />
            <datalist id="bulk-tags">{tags.map((t) => <option key={t} value={t} />)}</datalist>
            <Button variant="outline" disabled={!tag.trim()} onClick={() => void addTag()}><Tag size={14} className="text-accent" />{t('bulk.add')}</Button>
          </div>
          <VaultMenu label={t('bulk.moveToVault')} icon={<FolderInput size={15} className="text-accent" />} vaults={vaults} onPick={(id) => void runBulk('move', { toVaultId: id })} />
          <VaultMenu label={t('bulk.copyToVault')} icon={<Copy size={15} className="text-accent" />} vaults={vaults} onPick={(id) => void runBulk('copy', { toVaultId: id })} />
          <Button variant="outline" onClick={() => void runBulk('archive')}><Archive size={15} className="text-accent" />{t('bulk.archive')}</Button>
          <Button variant="outline" className="text-danger" onClick={() => useApp.getState().setBulkConfirm('trash')}><Trash2 size={15} />{t('bulk.moveToTrash')}</Button>
        </>)}
        {kind !== 'deleted' && <Button variant="outline" disabled={!n} onClick={() => { const refs = refsFor(items, selectedIds(readSel())); if (refs.length) setExportRefs(refs); }}><Download size={15} className="text-accent" />{t('export.buttonSelected')}</Button>}
      </div>
      <p className="mt-6 max-w-sm text-center text-xs text-muted">{t('bulk.hint')}</p>
      <ExportDialog open={!!exportRefs} onOpenChange={(o) => !o && setExportRefs(null)} items={exportRefs ?? []} />
    </div>
  );
}

function VaultMenu({ label, icon, vaults, onPick }: { label: string; icon: React.ReactNode; vaults: { id: string; name: string }[]; onPick: (id: string) => void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild><Button variant="outline">{icon}{label}</Button></DropdownMenuTrigger>
      <DropdownMenuContent align="center">
        {vaults.map((v) => <DropdownMenuItem key={v.id} onSelect={() => onPick(v.id)}>{v.name}</DropdownMenuItem>)}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Confirm for Move to Trash / Delete permanently (keyboard Delete or bulk pane). */
export function BulkConfirm() {
  const c = useApp((s) => s.bulkConfirm);
  const set = useApp((s) => s.setBulkConfirm);
  const multi = useApp((s) => s.multiIds);
  const selected = useApp((s) => s.selectedItemId);
  const items = useItemsUI((s) => s.items);
  const ids = selectedIds({ selected, multi });
  const n = refsFor(items, ids).length;
  const one = n === 1 ? items.find((x) => x.id === ids[0]) : undefined;
  const what = one ? `“${one.title}”` : plural(n);
  const purge = c === 'purge';
  return (
    <Dialog open={!!c && n > 0} onOpenChange={(o) => !o && set(null)}>
      <DialogContent title={purge ? 'Delete permanently?' : 'Move to Trash?'}
        description={purge ? `${what} will be purged. This can't be undone.` : `${what} will move to Recently Deleted. You can restore from there.`}>
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={() => set(null)}>Cancel</Button>
          <Button variant="danger" autoFocus onClick={() => { set(null); void runBulk(purge ? 'purge' : 'trash'); }}>{purge ? 'Delete permanently' : 'Move to Trash'}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Delete-key handler for the list: trash in normal/archive views, purge in Recently Deleted. */
export function requestDelete() { if (selectedIds(readSel()).length) useApp.getState().setBulkConfirm(viewKind() === 'deleted' ? 'purge' : 'trash'); }
