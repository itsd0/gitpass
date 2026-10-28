import { useEffect, useState } from 'react';
import { Archive, ArchiveRestore, ChevronRight, Copy, Download, ExternalLink, EyeOff, FolderInput, MoreVertical, Paperclip, Pencil, Plus, RotateCcw, Share2, ShieldAlert, Star, Trash2, Users } from 'lucide-react';
import { api } from '../../api/client';
import type { ItemDetail as Detail, VaultInfo } from '../../api/types';
import { useApp } from '../../lib/store';
import { useVaultsAndTags } from '../../lib/data';
import { cn, fmtBytes, fmtDateLong } from '../../lib/utils';
import { Badge, Button, Dialog, DialogContent, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger, Skeleton, Tooltip, toast } from '../../ui';
import { ItemIcon } from './categories';
import { FieldRow } from './FieldView';
import { ItemEditor } from './ItemEditor';
import { errText } from './actions';
import { EmptyIllustration } from './EmptyIllustration';
import { useItemsUI } from './state';
import { useT } from '../../i18n';

export function ItemDetailPane() {
  const id = useApp((s) => s.selectedItemId);
  const editing = useApp((s) => s.editing);
  const version = useApp((s) => s.itemsVersion);
  const item = useSelectedSummary(id);
  const [detail, setDetail] = useState<Detail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const { vaults } = useVaultsAndTags();

  useEffect(() => {
    if (!id || !item) { setDetail(null); return; }
    let on = true; setErr(null);
    api.getItem({ vaultId: item.vaultId, itemId: id }).then((d) => on && setDetail(d), (e) => on && setErr(errText(e)));
    return () => { on = false; };
  }, [id, item?.vaultId, version]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!id || !item) return <EmptyState />;
  if (err) return <div className="p-8 text-danger">{err}</div>;
  if (!detail || detail.id !== id) return <div className="space-y-4 p-6"><Skeleton className="h-12 w-64" /><Skeleton className="h-40 w-full" /></div>;
  const vault = vaults.find((v) => v.id === detail.vaultId);
  if (editing) return <ItemEditor detail={detail} vaults={vaults} />;
  return <DetailView detail={detail} vault={vault} vaults={vaults} />;
}

function useSelectedSummary(id: string | null) {
  const items = useItemsUI((s) => s.items);
  return items.find((x) => x.id === id) ?? null;
}

function EmptyState() {
  const t = useT();
  const items = useItemsUI((s) => s.items); const loading = useItemsUI((s) => s.loading);
  const route = useApp((s) => s.route);
  const vaultEmpty = !loading && items.length === 0 && route.name === 'items' && route.filter.kind === 'vault' && !route.filter.query;
  if (vaultEmpty) return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center text-muted" data-testid="empty-vault">
      <EmptyIllustration />
      <div><div className="text-base font-medium text-fg">{t('detail.emptyTitle')}</div><div className="mt-1 text-sm">{t('detail.emptyBody')}</div></div>
      <Button onClick={() => useApp.getState().openModal('new-item', route.name === 'items' ? { vaultId: route.filter.vaultId } : undefined)}><Plus size={15} />{t('detail.newItem')}</Button>
    </div>
  );
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center text-muted" data-testid="empty-detail">
      <EmptyIllustration />
      <div><div className="text-base font-medium text-fg">{t('detail.selectTitle')}</div><div className="mt-1 text-sm">{t('detail.selectBody')}</div></div>
    </div>
  );
}

function DetailView({ detail, vault, vaults }: { detail: Detail; vault?: VaultInfo; vaults: VaultInfo[] }) {
  const t = useT();
  const { setEditing, refreshItems, selectItem } = useApp();
  const ctx = { vaultId: detail.vaultId, itemId: detail.id };
  const deleted = !!detail.deletedAt;
  const run = (p: Promise<unknown>, msg: string, deselect = false) => p.then(() => { toast(msg); if (deselect) selectItem(null); refreshItems(); }, (e) => toast(errText(e), 'danger'));
  const [confirmPurge, setConfirmPurge] = useState(false);
  const findings = detail.findings ?? [];
  const canChangePassword = !!detail.changePasswordUrl && findings.some((f) => ['weak', 'reused', 'pwned', 'insecure_url'].includes(f.kind));
  const changePassword = () => api.openChangePassword({ vaultId: detail.vaultId, itemId: detail.id }).catch((e) => toast(errText(e), 'danger'));

  return (
    <div className="flex h-full flex-col outline-none" tabIndex={-1} data-detail-pane>
      <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border px-4 text-sm">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 text-fg/90">
          <span className="inline-block h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: vault?.color ?? '#888' }} />
          <span className="shrink-0 truncate" data-testid="crumb-vault">{vault?.name ?? '…'}</span><ChevronRight size={13} className="shrink-0 text-muted" />
          <span className="truncate font-medium" data-testid="crumb-item">{detail.title || detail.subtitle || '—'}</span>
          {vault && <span className="ml-2 flex items-center gap-1 text-muted"><Users size={13} />{vault.members.length}</span>}
        </div>
        {!deleted && <>
          <Button variant="ghost" size="sm" onClick={() => useApp.getState().openModal('people', { vaultId: detail.vaultId })}><Share2 size={14} />{t('detail.share')}</Button>
          <Button variant="ghost" size="sm" onClick={() => setEditing(true)}><Pencil size={14} />{t('detail.edit')}</Button>
        </>}
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" aria-label={t('detail.moreActions')}><MoreVertical size={16} /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {deleted ? <>
              <DropdownMenuItem onSelect={() => run(api.restoreItem(ctx), t('detail.toast.restored'), true)}><RotateCcw size={14} />{t('detail.restore')}</DropdownMenuItem>
              <DropdownMenuItem danger onSelect={() => setConfirmPurge(true)}><Trash2 size={14} />{t('detail.deletePermanently')}</DropdownMenuItem>
            </> : <>
              <DropdownMenuItem onSelect={() => run(api.setFavorite({ ...ctx, favorite: !detail.favorite }), detail.favorite ? t('bulk.toast.unfavorite') : t('bulk.toast.favorite'))}><Star size={14} />{detail.favorite ? t('bulk.unfavorite') : t('bulk.favorite')}</DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger><FolderInput size={14} />{t('bulk.moveToVault')}</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {vaults.filter((v) => v.id !== detail.vaultId).map((v) => <DropdownMenuItem key={v.id} onSelect={() => run(api.moveItem({ ...ctx, toVaultId: v.id }), t('detail.toast.movedToVault', { vault: v.name }))}><span className="h-2.5 w-2.5 rounded-full" style={{ background: v.color ?? '#888' }} />{v.name}</DropdownMenuItem>)}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuItem onSelect={() => api.duplicateItem(ctx).then((s) => { toast(t('detail.toast.duplicated')); refreshItems(); selectItem(s.id); }, (e) => toast(errText(e), 'danger'))}><Copy size={14} />{t('detail.duplicate')}</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={() => run(api.archiveItem({ ...ctx, archived: !detail.archived }), detail.archived ? t('detail.toast.unarchived') : t('detail.toast.archived'), true)}>{detail.archived ? <ArchiveRestore size={14} /> : <Archive size={14} />}{detail.archived ? t('detail.unarchive') : t('detail.archive')}</DropdownMenuItem>
              <DropdownMenuItem danger onSelect={() => run(api.deleteItem(ctx), t('bulk.toast.trash'), true)}><Trash2 size={14} />{t('detail.delete')}</DropdownMenuItem>
            </>}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <div className="flex-1 overflow-y-auto px-6 py-6">
        <div className="mx-auto max-w-3xl">
          <div className="mb-6 flex items-center gap-4">
            <ItemIcon item={detail} size={48} />
            <h1 className="min-w-0 flex-1 truncate text-xl font-semibold">{detail.title}</h1>
            {!deleted && <Tooltip content={detail.favorite ? t('detail.unfavorite') : t('detail.favorite')}>
              <Button variant="ghost" size="icon" aria-label={t('detail.toggleFavorite')} onClick={() => run(api.setFavorite({ ...ctx, favorite: !detail.favorite }), detail.favorite ? t('bulk.toast.unfavorite') : t('bulk.toast.favorite'))}>
                <Star size={18} className={cn(detail.favorite ? 'fill-warn text-warn' : 'text-muted')} />
              </Button></Tooltip>}
          </div>
          {deleted && <div className="mb-4 rounded-lg border border-danger/40 bg-danger/10 px-4 py-2 text-sm">{t('detail.inTrash')}</div>}
          {findings.length > 0 && (
            <div
              className="mb-4 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-warn/40 bg-warn/10 px-4 py-2 text-xs"
              data-testid="watchtower-findings"
            >
              <div className="flex min-w-0 items-start gap-1.5">
                <ShieldAlert size={14} className="mt-0.5 shrink-0 text-warn" />
                <div className="min-w-0">
                  <div className="font-medium text-fg">{t('detail.watchtowerFindings')}</div>
                  <div className="text-muted">{findings.map((f) => f.description).join(' · ')}</div>
                </div>
              </div>
              {canChangePassword && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={changePassword}
                  aria-label={`${t('watchtower.changePassword')} for ${detail.title}`}
                >
                  <ExternalLink size={13} className="text-accent" />{t('watchtower.changePassword')}
                </Button>
              )}
            </div>
          )}
          {detail.ignoredFindings && detail.ignoredFindings.length > 0 && (
            <div
              className="mb-4 flex items-center justify-between rounded-lg border border-border bg-card/60 px-4 py-2 text-xs text-muted"
              data-testid="watchtower-ignored-note"
            >
              <div className="flex items-center gap-1.5">
                <EyeOff size={14} className="text-muted" />
                <span>{t('detail.watchtowerIgnored')}</span>
              </div>
              <button
                type="button"
                onClick={async () => {
                  try {
                    for (const k of detail.ignoredFindings ?? []) {
                      await api.unignoreFinding({ vaultId: detail.vaultId, itemId: detail.id, kind: k });
                    }
                    toast(t('detail.warningRestored'));
                    refreshItems();
                    selectItem(detail.id);
                  } catch (e) {
                    toast(errText(e), 'danger');
                  }
                }}
                className="font-medium text-accent hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
              >
                {t('common.undo')}
              </button>
            </div>
          )}

          {detail.sections.filter((s) => s.fields.length).map((s) => (
            <div key={s.id} className="mb-4">
              {s.title && <div className="mb-1.5 px-1 text-xs font-semibold uppercase tracking-wider text-muted">{s.title}</div>}
              <div className="overflow-hidden rounded-lg border border-border bg-card">
                {s.fields.map((f) => <FieldRow key={f.id} field={f} ctx={ctx} category={detail.category} strength={detail.passwordStrength} />)}
              </div>
            </div>
          ))}

          {detail.notes && (
            <div className="mb-4 rounded-lg border border-border bg-card px-4 py-2.5">
              <div className="text-xs text-label">{t('detail.notes')}</div>
              <div className="whitespace-pre-wrap text-[14px]">{detail.notes}</div>
            </div>
          )}

          {detail.attachments.length > 0 && (
            <div className="mb-4 overflow-hidden rounded-lg border border-border bg-card">
              <div className="px-4 pt-2.5 text-xs text-label">{t('detail.attachments')}</div>
              {detail.attachments.map((a) => (
                <div key={a.id} className="group flex items-center gap-3 px-4 py-2">
                  <Paperclip size={14} className="text-muted" />
                  <div className="flex-1 truncate">{a.name}</div><span className="text-xs text-muted">{fmtBytes(a.size)}</span>
                  <Button variant="ghost" size="iconSm" aria-label={t('detail.saveAttachment', { name: a.name })} onClick={() => api.saveAttachment({ ...ctx, attachmentId: a.id, destPath: `~/Downloads/${a.name}` }).then(() => toast(t('detail.toast.savedAttachment', { name: a.name })), (e) => toast(errText(e), 'danger'))}><Download size={13} /></Button>
                </div>
              ))}
            </div>
          )}

          {detail.tags.length > 0 && <div className="mb-4 flex flex-wrap gap-1.5 px-1">{detail.tags.map((t) => <Badge key={t} tone="accent">{t}</Badge>)}</div>}

          <History detail={detail} />
        </div>
      </div>

      <Dialog open={confirmPurge} onOpenChange={setConfirmPurge}>
        <DialogContent title={t('detail.purgeTitle')} description={t('detail.purgeDesc', { title: detail.title })}>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setConfirmPurge(false)}>{t('common.cancel')}</Button>
            <Button variant="danger" onClick={() => { setConfirmPurge(false); void run(api.purgeItem(ctx), t('detail.toast.purged'), true); }}>{t('detail.purgeConfirm')}</Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function History({ detail }: { detail: Detail }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<Detail | null>(null);
  const versions = [...detail.history].sort((a, b) => b.version - a.version);
  return (
    <div className="mt-2 text-sm" data-testid="history">
      <button className="flex items-center gap-1.5 text-fg/90 hover:text-fg" onClick={() => setOpen(!open)} aria-expanded={open}>
        <ChevronRight size={14} className={cn('transition-transform', open && 'rotate-90')} />{t('detail.lastEdited', { date: fmtDateLong(detail.updatedAt) })}
      </button>
      {open && (
        <div className="ml-5 mt-2 space-y-1">
          <div className="text-xs text-muted">{t('detail.created', { date: fmtDateLong(detail.createdAt) })}</div>
          {versions.map((h) => (
            <button key={h.version} className="block text-xs text-accent hover:underline"
              onClick={() => api.itemVersion({ vaultId: detail.vaultId, itemId: detail.id, version: h.version }).then(setView, (e) => toast(errText(e), 'danger'))}>
              {t('detail.versionHistory', { version: h.version, date: fmtDateLong(h.updatedAt) })}
            </button>
          ))}
        </div>
      )}
      <Dialog open={!!view} onOpenChange={(o) => !o && setView(null)}>
        <DialogContent title={view ? t('detail.versionSnapshotTitle', { version: view.history.length, title: view.title }) : ''} description={t('detail.versionSnapshotDesc')}>
          {view && <div className="max-h-80 overflow-y-auto rounded-lg border border-border">
            {view.sections.flatMap((s) => s.fields).map((f) => (
              <div key={f.id} className="border-b border-border px-3 py-2 last:border-0"><div className="text-xs text-label">{f.label}</div><div className="font-mono text-sm">{f.value ?? '••••••••••'}</div></div>
            ))}
          </div>}
        </DialogContent>
      </Dialog>
    </div>
  );
}
