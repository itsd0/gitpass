import { useState } from 'react';
import { Archive, ArchiveRestore, Copy, FolderInput, RotateCcw, Star, StarOff, Tag, Trash2, X } from 'lucide-react';
import { api } from '../../api/client';
import type { BulkAction, BulkRequest } from '../../api/types';
import { useApp } from '../../lib/store';
import { useVaultsAndTags } from '../../lib/data';
import { Button, Dialog, DialogContent, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Input, toast } from '../../ui';
import { ItemIcon } from './categories';
import { errText } from './actions';
import { useItemsUI } from './state';
import { readSel, refsFor, selectedIds } from './selection';

const DONE: Record<BulkAction, string> = {
  trash: 'Moved to Recently Deleted', restore: 'Restored', purge: 'Permanently deleted', archive: 'Archived', unarchive: 'Unarchived',
  favorite: 'Added to Favorites', unfavorite: 'Removed from Favorites', add_tag: 'Tag added', remove_tag: 'Tag removed', move: 'Moved', copy: 'Copied',
};
const plural = (n: number) => `${n} item${n === 1 ? '' : 's'}`;

/** Run a bulk action on the current selection (multi or single). Clears the selection when items leave the view. */
export async function runBulk(action: BulkAction, extra: Partial<BulkRequest> = {}): Promise<boolean> {
  const items = useItemsUI.getState().items;
  const refs = refsFor(items, selectedIds(readSel()));
  if (!refs.length) return false;
  try {
    const r = await api.bulkItems({ req: { action, items: refs, ...extra } });
    toast(`${DONE[action]}: ${plural(r.count)}`);
    const keep = action === 'favorite' || action === 'unfavorite' || action === 'add_tag' || action === 'remove_tag' || action === 'copy';
    if (!keep) useApp.setState({ selectedItemId: null, multiIds: [], editing: false });
    useApp.getState().refreshItems();
    return true;
  } catch (e) { toast(errText(e), 'danger'); return false; }
}

function viewKind() { const r = useApp.getState().route; return r.name === 'items' ? r.filter.kind : 'all'; }

export function BulkPane() {
  const multi = useApp((s) => s.multiIds);
  const route = useApp((s) => s.route);
  const items = useItemsUI((s) => s.items);
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
      <h2 className="text-xl font-semibold" data-testid="bulk-count">{n} items selected</h2>
      <button className="mt-1 flex items-center gap-1 text-xs text-muted hover:text-accent" onClick={clear}><X size={12} />Clear selection (Esc)</button>

      <div className="mt-6 grid w-full max-w-sm gap-2">
        {kind === 'deleted' ? (<>
          <Button onClick={() => void runBulk('restore')}><RotateCcw size={15} />Restore {plural(n)}</Button>
          <Button variant="danger" onClick={() => useApp.getState().setBulkConfirm('purge')}><Trash2 size={15} />Delete permanently…</Button>
        </>) : kind === 'archive' ? (<>
          <Button onClick={() => void runBulk('unarchive')}><ArchiveRestore size={15} />Unarchive {plural(n)}</Button>
          <Button variant="outline" className="text-danger" onClick={() => useApp.getState().setBulkConfirm('trash')}><Trash2 size={15} />Move to Trash…</Button>
        </>) : (<>
          {!allFav && <Button variant="outline" onClick={() => void runBulk('favorite')}><Star size={15} className="text-accent" />Add to Favorites</Button>}
          {anyFav && <Button variant="outline" onClick={() => void runBulk('unfavorite')}><StarOff size={15} className="text-accent" />Remove from Favorites</Button>}
          <div className="flex gap-2">
            <Input list="bulk-tags" value={tag} placeholder="Add tag…" aria-label="Tag to add" onChange={(e) => setTag(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void addTag(); } }} />
            <datalist id="bulk-tags">{tags.map((t) => <option key={t} value={t} />)}</datalist>
            <Button variant="outline" disabled={!tag.trim()} onClick={() => void addTag()}><Tag size={14} className="text-accent" />Add</Button>
          </div>
          <VaultMenu label="Move to vault" icon={<FolderInput size={15} className="text-accent" />} vaults={vaults} onPick={(id) => void runBulk('move', { toVaultId: id })} />
          <VaultMenu label="Copy to vault" icon={<Copy size={15} className="text-accent" />} vaults={vaults} onPick={(id) => void runBulk('copy', { toVaultId: id })} />
          <Button variant="outline" onClick={() => void runBulk('archive')}><Archive size={15} className="text-accent" />Archive</Button>
          <Button variant="outline" className="text-danger" onClick={() => useApp.getState().setBulkConfirm('trash')}><Trash2 size={15} />Move to Trash…</Button>
        </>)}
      </div>
      <p className="mt-6 max-w-sm text-center text-xs text-muted">⌘-click to add or remove items · ⇧-click to select a range · ⌘A selects all · Delete moves to Trash</p>
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
