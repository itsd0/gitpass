import { useEffect, useState } from 'react';
import type { BulkResult } from '../api/types';
import { api } from '../api/client';
import { useT } from '../i18n';
import { Button, Modal } from '../ui';

export function MergeTagDialog({
  prompt,
  onOpenChange,
  onMerged,
}: {
  prompt: { oldTag: string; newTag: string } | null;
  onOpenChange: (open: boolean) => void;
  onMerged: (oldTag: string, newTag: string) => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    setErr(null);
  }, [prompt]);

  const confirm = async () => {
    if (!prompt) return;
    setBusy(true);
    setErr(null);
    try {
      await api.renameTag({ old: prompt.oldTag, new: prompt.newTag });
      onMerged(prompt.oldTag, prompt.newTag);
      onOpenChange(false);
    } catch (e) {
      setErr(String((e as Error)?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={!!prompt}
      onOpenChange={onOpenChange}
      title={t('tag.mergeTitle', { tag: prompt?.newTag ?? '' })}
      description={t('tag.mergeDescription', { old: prompt?.oldTag ?? '', new: prompt?.newTag ?? '' })}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button
            type="button"
            variant="default"
            disabled={busy}
            onClick={confirm}
          >
            {t('tag.mergeConfirm')}
          </Button>
        </>
      }
    >
      {err && <p role="alert" className="text-sm text-danger">{err}</p>}
    </Modal>
  );
}

export function DeleteTagDialog({
  tag,
  onOpenChange,
  onDeleted,
}: {
  tag: string | null;
  onOpenChange: (open: boolean) => void;
  onDeleted: (tag: string, res: BulkResult) => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [count, setCount] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (tag) {
      setCount(null);
      setErr(null);
      api.listItems({ filter: { kind: 'tag', tag } })
        .then((items) => setCount(items.length))
        .catch(() => setCount(0));
    }
  }, [tag]);

  const confirm = async () => {
    if (!tag) return;
    setBusy(true);
    setErr(null);
    try {
      const res = await api.deleteTag({ name: tag });
      onDeleted(tag, res);
      onOpenChange(false);
    } catch (e) {
      setErr(String((e as Error)?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const warn = count === 1
    ? t('tag.deleteWarnOne')
    : t('tag.deleteWarn', { n: count ?? 0 });

  return (
    <Modal
      open={!!tag}
      onOpenChange={onOpenChange}
      title={t('tag.deleteTitle', { tag: tag ?? '' })}
      description={count !== null ? warn : undefined}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button
            type="button"
            variant="default"
            disabled={busy}
            onClick={confirm}
          >
            {t('tag.deleteConfirm')}
          </Button>
        </>
      }
    >
      {err && <p role="alert" className="text-sm text-danger">{err}</p>}
    </Modal>
  );
}
