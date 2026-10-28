import { create } from 'zustand';
import type { ItemSummary } from '../../api/types';
import type { Key } from '../../i18n';

export interface DragData {
  items: ItemSummary[];
}

interface DragStore {
  dragged: DragData | null;
  startDrag: (items: ItemSummary[]) => void;
  endDrag: () => void;
}

export const useDragStore = create<DragStore>((set) => ({
  dragged: null,
  startDrag: (items) => set({ dragged: { items } }),
  endDrag: () => set({ dragged: null }),
}));

if (typeof window !== 'undefined') {
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && useDragStore.getState().dragged) {
      useDragStore.getState().endDrag();
    }
  });
}

/** Text for the drag preview: item title for a single item, "N items" for multi-selection. */
export function getDragPreviewText(
  items: ItemSummary[],
  t: (key: Key, vars?: Record<string, string | number>) => string
): string {
  if (items.length > 1) {
    return t('drag.nItems', { n: items.length });
  }
  return items[0]?.title ?? '';
}

/** Check if a vault can accept a drop of the currently dragged items. */
export function canDropOnVault(
  vault: { id: string; readOnly?: boolean },
  draggedItems: ItemSummary[]
): boolean {
  if (!draggedItems.length) return false;
  if (vault.readOnly) return false;
  // Refuse if all dragged items are already in this vault ("same vault")
  const allInSameVault = draggedItems.every((it) => it.vaultId === vault.id);
  if (allInSameVault) return false;
  return true;
}
