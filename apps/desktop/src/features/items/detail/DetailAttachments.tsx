import { useRef, useState } from 'react';
import { Download, Paperclip, Plus, Trash2 } from 'lucide-react';
import { api } from '../../../api/client';
import type { ItemDetail as Detail } from '../../../api/types';
import { useApp } from '../../../lib/store';
import { useT } from '../../../i18n';
import { cn, fmtBytes } from '../../../lib/utils';
import { Button, Dialog, DialogContent, toast } from '../../../ui';
import { errText } from '../actions';
import { isTauri } from './useAttachmentDrop';

interface DetailAttachmentsProps {
  detail: Detail;
  isDragging?: boolean;
  onAddAttachment: (path: string) => Promise<void> | void;
}

export function DetailAttachments({ detail, isDragging, onAddAttachment }: DetailAttachmentsProps) {
  const t = useT();
  const { refreshItems } = useApp();
  const ctx = { vaultId: detail.vaultId, itemId: detail.id };
  const deleted = !!detail.deletedAt;
  const [removingAtt, setRemovingAtt] = useState<Detail['attachments'][number] | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const doAddAttachment = async (path: string) => {
    try {
      await onAddAttachment(path);
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

  return (
    <>
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
    </>
  );
}
