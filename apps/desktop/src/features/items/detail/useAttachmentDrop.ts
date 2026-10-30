import { useEffect, useRef, useState } from 'react';
import { api } from '../../../api/client';
import { useApp } from '../../../lib/store';
import { useT } from '../../../i18n';
import { toast } from '../../../ui';
import { errText } from '../actions';

/** True when a native (Tauri, physical-pixel) drop position lands inside `el`. */
export function dropInsideElement(
  el: HTMLElement | null,
  pos?: { x: number; y: number },
  scale = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1
): boolean {
  if (!el || !pos) return false;
  const r = el.getBoundingClientRect();
  const x = pos.x / scale;
  const y = pos.y / scale;
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
}

export const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

interface UseAttachmentDropOptions {
  vaultId: string;
  itemId: string;
  deleted: boolean;
}

export function useAttachmentDrop({ vaultId, itemId, deleted }: UseAttachmentDropOptions) {
  const t = useT();
  const { refreshItems } = useApp();
  const [isDragging, setIsDragging] = useState(false);
  const dropPaneRef = useRef<HTMLDivElement>(null);

  const doAddAttachment = async (path: string) => {
    try {
      const a = await api.addAttachment({ vaultId, itemId, path });
      toast(t('detail.toast.addedAttachment', { name: a.name }));
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
  }, [vaultId, itemId, deleted]); // eslint-disable-line react-hooks/exhaustive-deps

  const onDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!deleted) setIsDragging(true);
  };

  const onDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  };

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
    if (deleted) return;
    const file = e.dataTransfer.files?.[0];
    if (file) {
      const p = (file as any).path || file.name;
      void doAddAttachment(p);
    }
  };

  return {
    dropPaneRef,
    isDragging,
    setIsDragging,
    doAddAttachment,
    dragHandlers: {
      onDragOver,
      onDragLeave,
      onDrop,
    },
  };
}
