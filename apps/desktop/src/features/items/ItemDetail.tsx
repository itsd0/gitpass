import { useEffect, useMemo, useRef, useState } from 'react';
import { Archive, ArchiveRestore, ChevronRight, Copy, Download, ExternalLink, Eye, EyeOff, FolderInput, GitBranch, Link2, MoreVertical, Paperclip, Pencil, Plus, QrCode, RotateCcw, Share2, ShieldAlert, Star, Trash2, Users, X } from 'lucide-react';
import { api } from '../../api/client';
import type { ItemDetail as Detail, VaultInfo } from '../../api/types';
import { useApp } from '../../lib/store';
import { formatRelativeLastUsed } from './lastUsedAge';
import { rotationLabel } from './rotation';
import { useVaultsAndTags } from '../../lib/data';
import { cn, fmtBytes, fmtDateLong } from '../../lib/utils';
import { Badge, Button, Dialog, DialogContent, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger, Skeleton, Tooltip, toast } from '../../ui';
import { ItemIcon } from './categories';
import { FieldRow } from './FieldView';
import { SshKeyDetail, SSH_DETAIL_FIELD_IDS } from './SshKeyDetail';
import { groupCardNumber } from '../../lib/cardMask';
import { PasskeyDetail, isPasskeyDetailField } from './PasskeyDetail';
import { ItemEditor } from './ItemEditor';
import { SafeMarkdown } from './SafeMarkdown';
import { useSettings } from '../settings/useSettings';
import { WifiQrDialog } from './WifiQrDialog';
import { GitSigningDialog } from './GitSigningDialog';
import { extractWifiFields } from './wifiQr';
import { errText } from './actions';
import { EmptyIllustration } from './EmptyIllustration';
import { useItemsUI } from './state';
import { useT } from '../../i18n';
import { refTarget as buildRefTarget, secretRef } from '../../lib/secretRef';

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

/** True when a native (Tauri, physical-pixel) drop position lands inside `el`. */
export function dropInsideElement(el: HTMLElement | null, pos?: { x: number; y: number }, scale = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1): boolean {
  if (!el || !pos) return false;
  const r = el.getBoundingClientRect();
  const x = pos.x / scale;
  const y = pos.y / scale;
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
}

function DetailView({ detail, vault, vaults }: { detail: Detail; vault?: VaultInfo; vaults: VaultInfo[] }) {
  const t = useT();
  const { setEditing, refreshItems, selectItem } = useApp();
  const ctx = { vaultId: detail.vaultId, itemId: detail.id };
  const siblings = useItemsUI((s) => s.items);
  const refTarget = useMemo(
    () => buildRefTarget(vault, vaults, detail, siblings.filter((i) => i.vaultId === detail.vaultId), detail.sections.flatMap((s) => s.fields)),
    [vault, vaults, detail, siblings],
  );
  const copyPasswordRef = () => {
    const f = detail.sections.flatMap((s) => s.fields).find((x) => x.id === 'password');
    if (!refTarget || !f || !navigator.clipboard) { toast(t('ref.failed'), 'danger'); return; }
    navigator.clipboard.writeText(secretRef(refTarget, f)).then(() => toast(t('ref.copied')), () => toast(t('ref.failed'), 'danger'));
  };
  const deleted = !!detail.deletedAt;
  const run = (p: Promise<unknown>, msg: string, deselect = false) => p.then(() => { toast(msg); if (deselect) selectItem(null); refreshItems(); }, (e) => toast(errText(e), 'danger'));
  const [confirmPurge, setConfirmPurge] = useState(false);
  const [removingAtt, setRemovingAtt] = useState<Detail['attachments'][number] | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const dropPaneRef = useRef<HTMLDivElement>(null);
  const [showWifiQr, setShowWifiQr] = useState(false);
  const wifiQrBtnRef = useRef<HTMLButtonElement>(null);
  const [showGitSigning, setShowGitSigning] = useState(false);
  const gitSigningBtnRef = useRef<HTMLButtonElement>(null);
  const moreActionsBtnRef = useRef<HTMLButtonElement>(null);
  const lastTriggerRef = useRef<HTMLElement | null>(null);

  const wifiInfo = detail.category === 'wifi' ? extractWifiFields(detail) : null;
  const hasSsid = Boolean(wifiInfo && wifiInfo.ssid.length > 0);

  const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  const doAddAttachment = async (path: string) => {
    try {
      const a = await api.addAttachment({ ...ctx, path });
      toast(t('detail.toast.addedAttachment', { name: a.name }));
      refreshItems();
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  const onAddFile = async () => {
    if (isTauri()) {
      try {
        const { open } = await import('@tauri-apps/plugin-dialog');
        const res = await open({ multiple: false });
        if (typeof res === 'string' && res) {
          await doAddAttachment(res);
        }
      } catch (e) {
        toast(errText(e), 'danger');
      }
    } else {
      fileInputRef.current?.click();
    }
  };

  const doSaveAttachment = async (a: Detail['attachments'][number]) => {
    try {
      let destPath = `~/Downloads/${a.name}`;
      if (isTauri()) {
        const { save } = await import('@tauri-apps/plugin-dialog');
        const chosen = await save({ defaultPath: a.name });
        if (!chosen) return;
        destPath = chosen;
      }
      await api.saveAttachment({ ...ctx, attachmentId: a.id, destPath });
      toast(t('detail.toast.savedAttachment', { name: a.name }));
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  const doRemoveAttachment = async (a: Detail['attachments'][number]) => {
    try {
      await api.removeAttachment({ ...ctx, attachmentId: a.id });
      toast(t('detail.toast.removedAttachment', { name: a.name }));
      refreshItems();
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  useEffect(() => {
    if (!isTauri() || deleted) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const inPane = (pos?: { x: number; y: number }) => dropInsideElement(dropPaneRef.current, pos);
    import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) => {
        if (cancelled) return;
        return getCurrentWebview()
          .onDragDropEvent((event) => {
            const pl = event.payload as { type: string; paths?: string[]; position?: { x: number; y: number } };
            if (pl.type === 'over') {
              setIsDragging(inPane(pl.position));
            } else if (pl.type === 'leave') {
              setIsDragging(false);
            } else if (pl.type === 'drop') {
              setIsDragging(false);
              if (!inPane(pl.position)) return;
              const p = pl.paths?.[0];
              if (p) void doAddAttachment(p);
            }
          })
          .then((u) => {
            // The effect may have been cleaned up while the subscription was pending.
            if (cancelled) u();
            else unlisten = u;
          });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
      setIsDragging(false);
    };
  }, [detail.id, detail.vaultId, deleted]);

  const renderNotesMarkdown = useSettings((s) => s.settings?.renderNotesMarkdown ?? true);
  const findings = detail.findings ?? [];
  const canChangePassword = !!detail.changePasswordUrl && findings.some((f) => ['weak', 'reused', 'reused_old_password', 'pwned', 'insecure_url'].includes(f.kind));
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
          {detail.category === 'wifi' && (
            hasSsid ? (
              <Button
                ref={wifiQrBtnRef}
                variant="ghost"
                size="sm"
                onClick={() => {
                  lastTriggerRef.current = wifiQrBtnRef.current;
                  setShowWifiQr(true);
                }}
                aria-label={t('wifi.showQrCode')}
                data-testid="show-wifi-qr-btn"
              >
                <QrCode size={14} />{t('wifi.showQrCode')}
              </Button>
            ) : (
              <Tooltip content={t('wifi.missingSsidTooltip')}>
                <span className="inline-block">
                  <Button
                    ref={wifiQrBtnRef}
                    variant="ghost"
                    size="sm"
                    disabled
                    aria-label={t('wifi.showQrCode')}
                    data-testid="show-wifi-qr-btn"
                  >
                    <QrCode size={14} />{t('wifi.showQrCode')}
                  </Button>
                </span>
              </Tooltip>
            )
          )}
          {detail.category === 'ssh_key' && (
            <Button
              ref={gitSigningBtnRef}
              variant="ghost"
              size="sm"
              onClick={() => {
                lastTriggerRef.current = gitSigningBtnRef.current;
                setShowGitSigning(true);
              }}
              aria-label={t('ssh.useForGitSigning')}
              data-testid="use-for-git-signing-btn"
            >
              <GitBranch size={14} />{t('ssh.useForGitSigning')}
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={() => useApp.getState().openModal('people', { vaultId: detail.vaultId })}><Share2 size={14} />{t('detail.share')}</Button>
          <Button variant="ghost" size="sm" onClick={() => setEditing(true)}><Pencil size={14} />{t('detail.edit')}</Button>
        </>}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              ref={moreActionsBtnRef}
              variant="ghost"
              size="icon"
              aria-label={t('detail.moreActions')}
              data-testid="detail-more-actions-btn"
            >
              <MoreVertical size={16} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {deleted ? <>
              <DropdownMenuItem onSelect={() => run(api.restoreItem(ctx), t('detail.toast.restored'), true)}><RotateCcw size={14} />{t('detail.restore')}</DropdownMenuItem>
              <DropdownMenuItem danger onSelect={() => setConfirmPurge(true)}><Trash2 size={14} />{t('detail.deletePermanently')}</DropdownMenuItem>
            </> : <>
              {detail.category === 'wifi' && (
                hasSsid ? (
                  <DropdownMenuItem
                    data-testid="menu-show-wifi-qr"
                    onSelect={() => {
                      lastTriggerRef.current = moreActionsBtnRef.current;
                      setShowWifiQr(true);
                    }}
                  >
                    <QrCode size={14} />{t('wifi.showQrCode')}
                  </DropdownMenuItem>
                ) : (
                  <DropdownMenuItem
                    disabled
                    data-testid="menu-show-wifi-qr"
                    title={t('wifi.missingSsidTooltip')}
                  >
                    <QrCode size={14} />{t('wifi.showQrCode')}
                  </DropdownMenuItem>
                )
              )}
              {detail.category === 'ssh_key' && (
                <DropdownMenuItem
                  data-testid="menu-use-for-git-signing"
                  onSelect={() => {
                    lastTriggerRef.current = moreActionsBtnRef.current;
                    setShowGitSigning(true);
                  }}
                >
                  <GitBranch size={14} />{t('ssh.useForGitSigning')}
                </DropdownMenuItem>
              )}
              <DropdownMenuItem onSelect={() => run(api.setFavorite({ ...ctx, favorite: !detail.favorite }), detail.favorite ? t('bulk.toast.unfavorite') : t('bulk.toast.favorite'))}><Star size={14} />{detail.favorite ? t('bulk.unfavorite') : t('bulk.favorite')}</DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger><FolderInput size={14} />{t('bulk.moveToVault')}</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {vaults.filter((v) => v.id !== detail.vaultId).map((v) => <DropdownMenuItem key={v.id} onSelect={() => run(api.moveItem({ ...ctx, toVaultId: v.id }), t('detail.toast.movedToVault', { vault: v.name }))}><span className="h-2.5 w-2.5 rounded-full" style={{ background: v.color ?? '#888' }} />{v.name}</DropdownMenuItem>)}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              {detail.sections.some((s) => s.fields.some((f) => f.id === 'password')) && <DropdownMenuItem onSelect={copyPasswordRef} data-testid="copy-password-ref"><Link2 size={14} />{t('ref.copyPassword')}</DropdownMenuItem>}
              <DropdownMenuItem onSelect={() => api.duplicateItem(ctx).then((s) => { toast(t('detail.toast.duplicated')); refreshItems(); selectItem(s.id); }, (e) => toast(errText(e), 'danger'))}><Copy size={14} />{t('detail.duplicate')}</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={() => run(api.archiveItem({ ...ctx, archived: !detail.archived }), detail.archived ? t('detail.toast.unarchived') : t('detail.toast.archived'), true)}>{detail.archived ? <ArchiveRestore size={14} /> : <Archive size={14} />}{detail.archived ? t('detail.unarchive') : t('detail.archive')}</DropdownMenuItem>
              <DropdownMenuItem danger onSelect={() => run(api.deleteItem(ctx), t('bulk.toast.trash'), true)}><Trash2 size={14} />{t('detail.delete')}</DropdownMenuItem>
            </>}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <div
        ref={dropPaneRef}
        className="flex-1 overflow-y-auto px-6 py-6"
        onDragOver={(e) => {
          e.preventDefault();
          e.stopPropagation();
          if (!deleted) setIsDragging(true);
        }}
        onDragLeave={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setIsDragging(false);
        }}
        onDrop={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setIsDragging(false);
          if (deleted) return;
          const file = e.dataTransfer.files?.[0];
          if (file) {
            const p = (file as any).path || file.name;
            void doAddAttachment(p);
          }
        }}
      >
        <input
          type="file"
          ref={fileInputRef}
          className="hidden"
          data-testid="attachment-file-input"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) {
              const p = (file as any).path || file.name;
              void doAddAttachment(p);
              e.target.value = '';
            }
          }}
        />
        <div className="mx-auto max-w-3xl">
          <div className="mb-6 flex items-center gap-4">
            <ItemIcon item={detail} size={40} />
            <div className="min-w-0 flex-1">
              <h1 className="truncate text-xl font-semibold">{detail.title}</h1>
              {detail.lastUsed ? (
                <div className="mt-0.5 text-xs text-muted" data-testid="last-used">
                  {formatRelativeLastUsed(detail.lastUsed, undefined, t)}
                </div>
              ) : null}
              {(() => {
                const label = rotationLabel(detail.rotation, t);
                return label ? (
                  <Badge tone="accent" className="mt-1" data-testid="rotation-pill" aria-label={label}>{label}</Badge>
                ) : null;
              })()}
            </div>
            {!deleted && <Tooltip content={detail.favorite ? t('detail.unfavorite') : t('detail.favorite')}>
              <Button variant="ghost" size="icon" aria-label={t('detail.toggleFavorite')} onClick={() => run(api.setFavorite({ ...ctx, favorite: !detail.favorite }), detail.favorite ? t('bulk.toast.unfavorite') : t('bulk.toast.favorite'))}>
                <Star size={18} className={cn(detail.favorite ? 'fill-warn text-warn' : 'text-muted')} />
              </Button></Tooltip>}
          </div>
          {detail.category === 'ssh_key' && <SshKeyDetail detail={detail} ctx={ctx} />}
          {detail.category === 'passkey' && <PasskeyDetail detail={detail} ctx={ctx} />}
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
                  aria-label={t('watchtower.changePasswordFor', { title: detail.title })}
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

          {detail.sections.map((s) => detail.category === 'ssh_key'
            ? { ...s, fields: s.fields.filter((f) => !(SSH_DETAIL_FIELD_IDS as readonly string[]).includes(f.id)) }
            : detail.category === 'passkey'
            ? { ...s, fields: s.fields.filter((f) => !isPasskeyDetailField(f)) }
            : s).filter((s) => s.fields.length).map((s) => (
            <div key={s.id} className="mb-4">
              {s.title && <div className="mb-1.5 px-1 text-xs font-semibold uppercase tracking-wider text-muted">{s.title}</div>}
              <div className="overflow-hidden rounded-lg border border-border bg-card">
                {s.fields.map((f) => (
                  <FieldRow
                    key={f.id}
                    field={f}
                    ctx={ctx}
                    category={detail.category}
                    strength={detail.passwordStrength}
                    passwordChangedAt={/password/i.test(f.label) ? detail.passwordChangedAt : undefined}
                    refTarget={refTarget}
                    itemTitle={detail.title}
                    username={detail.subtitle || detail.sections.flatMap((sec) => sec.fields).find((x) => x.id === 'username')?.value || undefined}
                    passwordHistory={/password/i.test(f.label) && detail.category === 'login' ? detail.passwordHistory : undefined}
                    onHistoryCleared={refreshItems}
                  />
                ))}
              </div>
            </div>
          ))}

          {detail.notes && (
            <div className="mb-4 rounded-lg border border-border bg-card px-4 py-2.5" data-testid="detail-notes">
              <div className="flex items-center justify-between pb-1">
                <div className="text-xs text-label">{t('detail.notes')}</div>
                <Tooltip content={t('common.copy')}>
                  <Button
                    variant="ghost"
                    size="iconSm"
                    aria-label={`Copy ${t('detail.notes')}`}
                    data-testid="copy-notes-btn"
                    onClick={() =>
                      navigator.clipboard
                        ?.writeText(detail.notes ?? '')
                        .then(() => toast(t('common.copied')))
                    }
                  >
                    <Copy size={13} />
                  </Button>
                </Tooltip>
              </div>
              {renderNotesMarkdown ? (
                <SafeMarkdown content={detail.notes} />
              ) : (
                <div className="whitespace-pre-wrap text-[14px]">{detail.notes}</div>
              )}
            </div>
          )}

          {detail.relatedItems && detail.relatedItems.length > 0 && (
            <div
              className="mb-4 overflow-hidden rounded-lg border border-border bg-card"
              data-testid="related-items-card"
            >
              <div className="px-4 pt-2.5 pb-1">
                <div className="text-xs text-label">{t('related.title')}</div>
              </div>
              <div className="divide-y divide-border/50">
                {detail.relatedItems.map((item) => (
                  <div
                    key={item.id}
                    tabIndex={item.available ? 0 : -1}
                    role={item.available ? "button" : "row"}
                    aria-label={item.available ? item.title : t('related.notAvailable')}
                    data-testid={`related-item-${item.id}`}
                    onClick={() => {
                      if (item.available) {
                        const a = useApp.getState();
                        if (
                          a.route.name !== 'items' ||
                          (a.route.filter.kind === 'vault' && a.route.filter.vaultId !== item.vaultId) ||
                          (a.route.filter.kind === 'category' && a.route.filter.category !== item.category)
                        ) {
                          a.setRoute({ name: 'items', filter: { kind: 'all' } });
                        }
                        a.selectItem(item.id);
                      }
                    }}
                    onKeyDown={(e) => {
                      if ((e.key === 'Enter' || e.key === ' ') && item.available) {
                        e.preventDefault();
                        const a = useApp.getState();
                        if (
                          a.route.name !== 'items' ||
                          (a.route.filter.kind === 'vault' && a.route.filter.vaultId !== item.vaultId) ||
                          (a.route.filter.kind === 'category' && a.route.filter.category !== item.category)
                        ) {
                          a.setRoute({ name: 'items', filter: { kind: 'all' } });
                        }
                        a.selectItem(item.id);
                      }
                    }}
                    className={cn(
                      "group flex items-center gap-3 px-4 py-2.5 outline-none transition-colors",
                      item.available
                        ? "cursor-pointer hover:bg-hover/50 focus-visible:bg-hover focus-visible:ring-1 focus-visible:ring-accent"
                        : "opacity-80"
                    )}
                  >
                    <ItemIcon item={{ category: item.category ?? 'login', title: item.title, subtitle: item.subtitle ?? '' }} size={24} />
                    <div className="min-w-0 flex-1">
                      <div
                        className={cn(
                          "truncate text-sm font-medium",
                          item.available ? "text-fg group-hover:text-accent" : "text-muted italic"
                        )}
                      >
                        {item.available ? item.title : t('related.notAvailable')}
                      </div>
                      {item.available && item.subtitle && (
                        <div className="truncate text-xs text-muted">{item.subtitle}</div>
                      )}
                    </div>
                    {!item.available && !deleted && (
                      <Button
                        variant="ghost"
                        size="iconSm"
                        className="text-muted hover:text-danger"
                        aria-label={t('related.removeLink')}
                        data-testid={`remove-unavailable-related-${item.id}`}
                        onClick={async (e) => {
                          e.stopPropagation();
                          try {
                            await api.removeRelatedItem({
                              vaultId: detail.vaultId,
                              itemId: detail.id,
                              relatedId: item.id,
                            });
                            toast(t('related.toast.unlinked'));
                            refreshItems();
                          } catch (err) {
                            toast(errText(err), 'danger');
                          }
                        }}
                      >
                        <X size={13} />
                      </Button>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}

          <div
            className={cn(
              "mb-4 overflow-hidden rounded-lg border bg-card transition-colors",
              isDragging ? "border-accent bg-accent/5 ring-2 ring-accent/30" : "border-border"
            )}
            data-testid="attachments-card"
          >
            <div className="flex items-center justify-between px-4 pt-2.5 pb-1">
              <div className="text-xs text-label">{t('detail.attachments')}</div>
              {!deleted && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 text-xs text-accent hover:text-accent font-medium gap-1 px-1.5"
                  onClick={onAddFile}
                >
                  <Plus size={12} />
                  {t('detail.addAttachment')}
                </Button>
              )}
            </div>

            {isDragging && (
              <div className="mx-4 my-2 rounded-md border border-dashed border-accent bg-accent/10 p-3 text-center text-xs font-medium text-accent">
                {t('detail.dragDropHint')}
              </div>
            )}

            {detail.attachments.length > 0 ? (
              <div className="divide-y divide-border/50">
                {detail.attachments.map((a) => (
                  <div
                    key={a.id}
                    tabIndex={0}
                    role="row"
                    aria-label={a.name}
                    onKeyDown={(e) => {
                      if (!deleted && (e.key === 'Delete' || e.key === 'Backspace')) {
                        e.preventDefault();
                        setRemovingAtt(a);
                      }
                    }}
                    className="group flex items-center gap-3 px-4 py-2 outline-none focus-visible:bg-hover hover:bg-hover/50"
                  >
                    <Paperclip size={14} className="text-muted group-focus-visible:text-accent" />
                    <div className="flex-1 truncate text-sm">{a.name}</div>
                    <span className="text-xs text-muted">{fmtBytes(a.size)}</span>
                    <div className="flex items-center gap-1">
                      <Button
                        variant="ghost"
                        size="iconSm"
                        aria-label={t('detail.saveAttachment', { name: a.name })}
                        onClick={() => void doSaveAttachment(a)}
                      >
                        <Download size={13} />
                      </Button>
                      {!deleted && (
                        <Button
                          variant="ghost"
                          size="iconSm"
                          className="text-muted hover:text-danger"
                          aria-label={t('detail.removeAttachment', { name: a.name })}
                          onClick={() => setRemovingAtt(a)}
                        >
                          <Trash2 size={13} />
                        </Button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="px-4 pb-3 pt-1 text-xs text-muted">
                {t('detail.noAttachments')}
              </div>
            )}
          </div>

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

      <Dialog open={!!removingAtt} onOpenChange={(open) => !open && setRemovingAtt(null)}>
        <DialogContent
          title={t('detail.removeAttachmentTitle')}
          description={t('detail.removeAttachmentDesc', { name: removingAtt?.name ?? '' })}
        >
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setRemovingAtt(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                const target = removingAtt;
                setRemovingAtt(null);
                if (target) void doRemoveAttachment(target);
              }}
            >
              {t('detail.removeAttachmentConfirm')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {showWifiQr && (
        <WifiQrDialog
          open={showWifiQr}
          onOpenChange={setShowWifiQr}
          detail={detail}
          triggerRef={lastTriggerRef}
        />
      )}

      {showGitSigning && (
        <GitSigningDialog
          open={showGitSigning}
          onOpenChange={setShowGitSigning}
          detail={detail}
          triggerRef={lastTriggerRef}
        />
      )}
    </div>
  );
}

function History({ detail }: { detail: Detail }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<Detail | null>(null);
  const [selectedVersion, setSelectedVersion] = useState<number | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const versions = [...detail.history].sort((a, b) => b.version - a.version);

  const openSnapshot = (ver: number) => {
    setConfirming(false);
    setRevealed({});
    setSelectedVersion(ver);
    api.itemVersion({ vaultId: detail.vaultId, itemId: detail.id, version: ver })
      .then(setView, (e) => toast(errText(e), 'danger'));
  };

  const toggleReveal = async (fieldId: string) => {
    if (revealed[fieldId] !== undefined) {
      setRevealed((prev) => {
        const next = { ...prev };
        delete next[fieldId];
        return next;
      });
      return;
    }
    try {
      const val = await api.revealField({
        vaultId: detail.vaultId,
        itemId: detail.id,
        fieldId,
        version: selectedVersion,
      });
      setRevealed((prev) => ({ ...prev, [fieldId]: val }));
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  const handleRestore = async () => {
    if (!view || selectedVersion === null) return;
    const currentVersionBefore = detail.version ?? (detail.history[0]?.version ? detail.history[0].version + 1 : 1);
    const targetVersion = selectedVersion;
    setView(null);
    setConfirming(false);
    try {
      await api.restoreItemVersion({ vaultId: detail.vaultId, itemId: detail.id, version: targetVersion });
      useApp.getState().refreshItems();
      toast(t('detail.toast.versionRestored', { version: targetVersion }), {
        duration: 6000,
        action: {
          label: t('common.undo'),
          altText: t('common.undo'),
          onClick: async () => {
            try {
              await api.restoreItemVersion({ vaultId: detail.vaultId, itemId: detail.id, version: currentVersionBefore });
              useApp.getState().refreshItems();
              toast(t('detail.toast.versionUndone'));
            } catch (e) {
              toast(errText(e), 'danger');
            }
          },
        },
      });
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  return (
    <div className="mt-2 text-sm" data-testid="history">
      <button className="flex items-center gap-1.5 text-fg/90 hover:text-fg" onClick={() => setOpen(!open)} aria-expanded={open}>
        <ChevronRight size={14} className={cn('transition-transform', open && 'rotate-90')} />{t('detail.lastEdited', { date: fmtDateLong(detail.updatedAt) })}
      </button>
      {open && (
        <div className="ml-5 mt-2 space-y-1">
          <div className="text-xs text-muted">{t('detail.created', { date: fmtDateLong(detail.createdAt) })}</div>
          {detail.lastUsed ? (
            <div className="text-xs text-muted">
              {formatRelativeLastUsed(detail.lastUsed, undefined, t)}
            </div>
          ) : null}
          {versions.map((h) => (
            <button key={h.version} className="block text-xs text-accent hover:underline"
              onClick={() => openSnapshot(h.version)}>
              {t('detail.versionHistory', { version: h.version, date: fmtDateLong(h.updatedAt) })}
            </button>
          ))}
        </div>
      )}
      <Dialog open={!!view} onOpenChange={(o) => { if (!o) { setView(null); setConfirming(false); } }}>
        <DialogContent
          title={view && selectedVersion !== null ? t('detail.versionSnapshotTitle', { version: selectedVersion, title: view.title }) : ''}
          description={t('detail.versionSnapshotDesc')}
        >
          {view && (
            <div className="space-y-4">
              <div className="max-h-80 overflow-y-auto rounded-lg border border-border">
                {view.sections.flatMap((s) => s.fields).map((f) => {
                  const isConcealed = f.kind === 'concealed' || f.kind === 'totp';
                  const isRevealed = revealed[f.id] !== undefined;
                  const isApi = view.category === 'api_credential';
                  const shouldGroup = !isApi && Boolean(f.masked);
                  const displayValue = isConcealed
                    ? (isRevealed ? (shouldGroup ? groupCardNumber(revealed[f.id]) : revealed[f.id]) : (f.masked || '••••••••••'))
                    : (f.value ?? '');
                  return (
                    <div key={f.id} className="flex items-center justify-between border-b border-border px-3 py-2 last:border-0">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="text-xs text-label">{f.label}</span>
                          {f.diff === 'changed' && (
                            <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400 border border-amber-500/20" data-testid={`diff-${f.id}`}>
                              {t('detail.diff.changed')}
                            </span>
                          )}
                          {f.diff === 'added' && (
                            <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20" data-testid={`diff-${f.id}`}>
                              {t('detail.diff.added')}
                            </span>
                          )}
                          {f.diff === 'removed' && (
                            <span className="rounded bg-rose-500/10 px-1.5 py-0.5 text-[10px] font-medium text-rose-600 dark:text-rose-400 border border-rose-500/20" data-testid={`diff-${f.id}`}>
                              {t('detail.diff.removed')}
                            </span>
                          )}
                        </div>
                        <div className={cn("font-mono text-sm", isConcealed && !isRevealed && "tracking-widest")}>
                          {displayValue}
                        </div>
                      </div>
                      {isConcealed && (
                        <Button
                          variant="ghost"
                          size="iconSm"
                          aria-label={isRevealed ? `Conceal ${f.label}` : `Reveal ${f.label}`}
                          onClick={() => toggleReveal(f.id)}
                        >
                          {isRevealed ? <EyeOff size={13} /> : <Eye size={13} />}
                        </Button>
                      )}
                    </div>
                  );
                })}
              </div>

              {confirming ? (
                <div className="flex flex-col gap-2 rounded-lg border border-border bg-hover/50 p-3" data-testid="restore-confirm-pane">
                  <div className="text-sm font-medium text-fg">
                    {t('detail.restoreVersionConfirmTitle', { version: selectedVersion ?? 0 })}
                  </div>
                  <div className="text-xs text-muted">
                    {t('detail.restoreVersionConfirmDesc', { version: selectedVersion ?? 0 })}
                  </div>
                  <div className="mt-2 flex justify-end gap-2">
                    <Button variant="outline" size="sm" onClick={() => setConfirming(false)}>
                      {t('common.cancel')}
                    </Button>
                    <Button variant="default" size="sm" onClick={handleRestore} data-testid="confirm-restore-btn">
                      {t('detail.restoreVersionConfirmBtn')}
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="flex justify-end gap-2">
                  <Button variant="outline" onClick={() => setView(null)}>
                    {t('common.close')}
                  </Button>
                  <Button variant="default" onClick={() => setConfirming(true)} data-testid="restore-version-btn">
                    {t('detail.restoreVersion')}
                  </Button>
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
