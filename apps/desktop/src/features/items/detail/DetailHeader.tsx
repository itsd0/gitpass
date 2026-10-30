import { useRef, useState } from 'react';
import {
  Archive,
  ArchiveRestore,
  ChevronRight,
  Copy,
  FolderInput,
  GitBranch,
  Link2,
  MoreVertical,
  Pencil,
  QrCode,
  RotateCcw,
  Share2,
  Star,
  Trash2,
  Users,
} from 'lucide-react';
import { api } from '../../../api/client';
import type { ItemDetail as Detail, VaultInfo } from '../../../api/types';
import { useApp } from '../../../lib/store';
import { useT } from '../../../i18n';
import { cn } from '../../../lib/utils';
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  Tooltip,
  toast,
} from '../../../ui';
import { ItemIcon } from '../categories';
import { formatRelativeLastUsed } from '../lastUsedAge';
import { rotationLabel } from '../rotation';
import { WifiQrDialog } from '../WifiQrDialog';
import { GitSigningDialog } from '../GitSigningDialog';
import { extractWifiFields } from '../wifiQr';
import { errText } from '../actions';

interface DetailHeaderProps {
  detail: Detail;
  vault?: VaultInfo;
  vaults: VaultInfo[];
  copyPasswordRef: () => void;
}

export function DetailHeader({ detail, vault, vaults, copyPasswordRef }: DetailHeaderProps) {
  const t = useT();
  const { setEditing, refreshItems, selectItem } = useApp();
  const ctx = { vaultId: detail.vaultId, itemId: detail.id };
  const deleted = !!detail.deletedAt;
  const run = (p: Promise<unknown>, msg: string, deselect = false) =>
    p.then(
      () => {
        toast(msg);
        if (deselect) selectItem(null);
        refreshItems();
      },
      (e) => toast(errText(e), 'danger')
    );

  const [confirmPurge, setConfirmPurge] = useState(false);
  const [showWifiQr, setShowWifiQr] = useState(false);
  const wifiQrBtnRef = useRef<HTMLButtonElement>(null);
  const [showGitSigning, setShowGitSigning] = useState(false);
  const gitSigningBtnRef = useRef<HTMLButtonElement>(null);
  const moreActionsBtnRef = useRef<HTMLButtonElement>(null);
  const lastTriggerRef = useRef<HTMLElement | null>(null);

  const wifiInfo = detail.category === 'wifi' ? extractWifiFields(detail) : null;
  const hasSsid = Boolean(wifiInfo && wifiInfo.ssid.length > 0);

  return (
    <>
      <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border px-4 text-sm">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 text-fg/90">
          <span className="inline-block h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: vault?.color ?? '#888' }} />
          <span className="shrink-0 truncate" data-testid="crumb-vault">{vault?.name ?? '…'}</span>
          <ChevronRight size={13} className="shrink-0 text-muted" />
          <span className="truncate font-medium" data-testid="crumb-item">{detail.title || detail.subtitle || '—'}</span>
          {vault && <span className="ml-2 flex items-center gap-1 text-muted"><Users size={13} />{vault.members.length}</span>}
        </div>
        {!deleted && (
          <>
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
            <Button variant="ghost" size="sm" onClick={() => useApp.getState().openModal('people', { vaultId: detail.vaultId })}>
              <Share2 size={14} />{t('detail.share')}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setEditing(true)}>
              <Pencil size={14} />{t('detail.edit')}
            </Button>
          </>
        )}
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
            {deleted ? (
              <>
                <DropdownMenuItem onSelect={() => run(api.restoreItem(ctx), t('detail.toast.restored'), true)}><RotateCcw size={14} />{t('detail.restore')}</DropdownMenuItem>
                <DropdownMenuItem danger onSelect={() => setConfirmPurge(true)}><Trash2 size={14} />{t('detail.deletePermanently')}</DropdownMenuItem>
              </>
            ) : (
              <>
                {detail.category === 'wifi' && (
                  hasSsid ? (
                    <DropdownMenuItem data-testid="menu-show-wifi-qr" onSelect={() => { lastTriggerRef.current = moreActionsBtnRef.current; setShowWifiQr(true); }}>
                      <QrCode size={14} />{t('wifi.showQrCode')}
                    </DropdownMenuItem>
                  ) : (
                    <DropdownMenuItem disabled data-testid="menu-show-wifi-qr" title={t('wifi.missingSsidTooltip')}>
                      <QrCode size={14} />{t('wifi.showQrCode')}
                    </DropdownMenuItem>
                  )
                )}
                {detail.category === 'ssh_key' && (
                  <DropdownMenuItem data-testid="menu-use-for-git-signing" onSelect={() => { lastTriggerRef.current = moreActionsBtnRef.current; setShowGitSigning(true); }}>
                    <GitBranch size={14} />{t('ssh.useForGitSigning')}
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem onSelect={() => run(api.setFavorite({ ...ctx, favorite: !detail.favorite }), detail.favorite ? t('bulk.toast.unfavorite') : t('bulk.toast.favorite'))}>
                  <Star size={14} />{detail.favorite ? t('bulk.unfavorite') : t('bulk.favorite')}
                </DropdownMenuItem>
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger><FolderInput size={14} />{t('bulk.moveToVault')}</DropdownMenuSubTrigger>
                  <DropdownMenuSubContent>
                    {vaults.filter((v) => v.id !== detail.vaultId).map((v) => (
                      <DropdownMenuItem key={v.id} onSelect={() => run(api.moveItem({ ...ctx, toVaultId: v.id }), t('detail.toast.movedToVault', { vault: v.name }))}>
                        <span className="h-2.5 w-2.5 rounded-full" style={{ background: v.color ?? '#888' }} />{v.name}
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
                {detail.sections.some((s) => s.fields.some((f) => f.id === 'password')) && (
                  <DropdownMenuItem onSelect={copyPasswordRef} data-testid="copy-password-ref"><Link2 size={14} />{t('ref.copyPassword')}</DropdownMenuItem>
                )}
                <DropdownMenuItem onSelect={() => api.duplicateItem(ctx).then((s) => { toast(t('detail.toast.duplicated')); refreshItems(); selectItem(s.id); }, (e) => toast(errText(e), 'danger'))}>
                  <Copy size={14} />{t('detail.duplicate')}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={() => run(api.archiveItem({ ...ctx, archived: !detail.archived }), detail.archived ? t('detail.toast.unarchived') : t('detail.toast.archived'), true)}>
                  {detail.archived ? <ArchiveRestore size={14} /> : <Archive size={14} />}{detail.archived ? t('detail.unarchive') : t('detail.archive')}
                </DropdownMenuItem>
                <DropdownMenuItem danger onSelect={() => run(api.deleteItem(ctx), t('bulk.toast.trash'), true)}>
                  <Trash2 size={14} />{t('detail.delete')}
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <Dialog open={confirmPurge} onOpenChange={setConfirmPurge}>
        <DialogContent title={t('detail.purgeTitle')} description={t('detail.purgeDesc', { title: detail.title })}>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setConfirmPurge(false)}>{t('common.cancel')}</Button>
            <Button variant="danger" onClick={() => { setConfirmPurge(false); void run(api.purgeItem(ctx), t('detail.toast.purged'), true); }}>{t('detail.purgeConfirm')}</Button>
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
    </>
  );
}

export function DetailTitle({ detail }: { detail: Detail }) {
  const t = useT();
  const { refreshItems } = useApp();
  const deleted = !!detail.deletedAt;
  const ctx = { vaultId: detail.vaultId, itemId: detail.id };
  const run = (p: Promise<unknown>, msg: string) =>
    p.then(
      () => {
        toast(msg);
        refreshItems();
      },
      (e) => toast(errText(e), 'danger')
    );

  return (
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
      {!deleted && (
        <Tooltip content={detail.favorite ? t('detail.unfavorite') : t('detail.favorite')}>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('detail.toggleFavorite')}
            onClick={() => run(api.setFavorite({ ...ctx, favorite: !detail.favorite }), detail.favorite ? t('bulk.toast.unfavorite') : t('bulk.toast.favorite'))}
          >
            <Star size={18} className={cn(detail.favorite ? 'fill-warn text-warn' : 'text-muted')} />
          </Button>
        </Tooltip>
      )}
    </div>
  );
}
