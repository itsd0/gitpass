import { useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import { api } from '../../api/client';
import type { ItemDetail as Detail, VaultInfo } from '../../api/types';
import { useApp } from '../../lib/store';
import { useVaultsAndTags } from '../../lib/data';
import { Button, Skeleton } from '../../ui';
import { SshKeyDetail } from './SshKeyDetail';
import { PasskeyDetail } from './PasskeyDetail';
import { ItemEditor } from './ItemEditor';
import { EmptyIllustration } from './EmptyIllustration';
import { useItemsUI } from './state';
import { useT } from '../../i18n';
import { errText } from './actions';
import {
  DetailHeader,
  DetailTitle,
  DetailAttachments,
  useAttachmentDrop,
  DetailFindings,
  DetailSections,
  useSecretRef,
  DetailHistory,
  dropInsideElement,
} from './detail';

export { dropInsideElement };

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
  const deleted = !!detail.deletedAt;
  const ctx = { vaultId: detail.vaultId, itemId: detail.id };
  const { refTarget, copyPasswordRef } = useSecretRef(detail, vault, vaults);
  const { isDragging, dropPaneRef, dragHandlers, doAddAttachment } = useAttachmentDrop({
    vaultId: detail.vaultId,
    itemId: detail.id,
    deleted,
  });

  return (
    <div className="flex h-full flex-col outline-none" tabIndex={-1} data-detail-pane>
      <DetailHeader
        detail={detail}
        vault={vault}
        vaults={vaults}
        copyPasswordRef={copyPasswordRef}
      />
      <div
        ref={dropPaneRef}
        className="flex-1 overflow-y-auto px-6 py-6"
        {...dragHandlers}
      >
        <div className="mx-auto max-w-3xl">
          <DetailTitle detail={detail} />
          {detail.category === 'ssh_key' && <SshKeyDetail detail={detail} ctx={ctx} />}
          {detail.category === 'passkey' && <PasskeyDetail detail={detail} ctx={ctx} />}
          {deleted && (
            <div className="mb-4 rounded-lg border border-danger/40 bg-danger/10 px-4 py-2 text-sm">
              {t('detail.inTrash')}
            </div>
          )}
          <DetailFindings detail={detail} />
          <DetailSections detail={detail} refTarget={refTarget} deleted={deleted} />
          <DetailAttachments
            detail={detail}
            isDragging={isDragging}
            onAddAttachment={doAddAttachment}
          />
          <DetailHistory detail={detail} />
        </div>
      </div>
    </div>
  );
}
