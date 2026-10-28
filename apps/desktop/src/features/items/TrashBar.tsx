import { useEffect, useState } from 'react';
import { Trash2 } from 'lucide-react';
import { api } from '../../api/client';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';
import { Button, Dialog, DialogContent, toast } from '../../ui';
import { errText } from './actions';

/** Count + "Empty Recently Deleted" bar; refreshes after every vault change. */
export function TrashBar() {
  const t = useT();
  const version = useApp((s) => s.itemsVersion);
  const [count, setCount] = useState<number | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let on = true;
    api.listItems({ filter: { kind: 'deleted' } }).then((xs) => on && setCount(xs.length), () => on && setCount(null));
    return () => { on = false; };
  }, [version]);

  async function empty() {
    setBusy(true);
    try {
      const n = await api.emptyTrash();
      setConfirm(false);
      setCount(0);
      toast(n === 1 ? t('trash.toastOne') : t('trash.toast', { n }));
      useApp.setState({ selectedItemId: null, multiIds: [] });
      useApp.getState().refreshItems();
    } catch (e) {
      toast(errText(e), 'danger');
    } finally {
      setBusy(false);
    }
  }

  if (!count) return null;
  return (
    <>
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-border px-3" data-testid="trash-bar">
        <span className="text-xs text-muted">{count === 1 ? t('trash.countOne') : t('trash.count', { n: count })}</span>
        <Button variant="outline" size="sm" className="text-danger" onClick={() => setConfirm(true)}>
          <Trash2 size={13} />{t('trash.emptyTrash')}
        </Button>
      </div>
      <Dialog open={confirm} onOpenChange={(o) => { if (!busy) setConfirm(o); }}>
        <DialogContent title={count === 1 ? t('trash.confirmTitleOne') : t('trash.confirmTitle', { n: count })} description={t('trash.confirmBody')}>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => setConfirm(false)}>{t('common.cancel')}</Button>
            <Button variant="danger" disabled={busy} onClick={() => void empty()}>{t('trash.confirm')}</Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

export function TrashEmptyState() {
  const t = useT();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center text-muted" data-testid="trash-empty">
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-hover"><Trash2 size={24} /></div>
      <div>
        <div className="text-base font-medium text-fg">{t('trash.emptyTitle')}</div>
        <div className="mt-1 text-sm">{t('trash.emptyBody')}</div>
      </div>
    </div>
  );
}
