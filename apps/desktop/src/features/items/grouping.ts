import type { ItemSummary, Filter } from '../../api/types';
export type Row =
  | { type: 'header'; key: string; label: string }
  | { type: 'item'; key: string; item: ItemSummary; index: number };

export const monthLabel = (iso: string) =>
  new Date(iso)
    .toLocaleString('en-US', { month: 'long', year: 'numeric', timeZone: 'UTC' })
    .toUpperCase();

export interface GroupRowsOptions {
  showRecentSection?: boolean;
  recentLabel?: string;
  neverUsedLabel?: string;
}

/** Flatten items into header+item rows. Month headers for date sorts, A–Z letter headers for title sort. Preserves input order. */
export function groupRows(
  items: ItemSummary[],
  sort: Filter['sort'] = 'updated',
  options?: GroupRowsOptions
): Row[] {
  const rows: Row[] = [];
  const recentLabel = options?.recentLabel ?? 'RECENTLY USED';
  const neverUsedLabel = options?.neverUsedLabel ?? 'NEVER USED';

  if (sort === 'recent') {
    let last = '';
    items.forEach((item, index) => {
      const label = item.lastUsed && item.lastUsed > 0 ? recentLabel : neverUsedLabel;
      if (label !== last) {
        rows.push({ type: 'header', key: `h:${label}`, label });
        last = label;
      }
      rows.push({ type: 'item', key: item.id, item, index });
    });
    return rows;
  }

  let remaining = items;
  if (options?.showRecentSection) {
    const recent = items
      .filter((x) => x.lastUsed && x.lastUsed > 0)
      .sort((a, b) => (b.lastUsed ?? 0) - (a.lastUsed ?? 0))
      .slice(0, 6);
    if (recent.length > 0) {
      const recentIds = new Set(recent.map((x) => x.id));
      rows.push({ type: 'header', key: 'h:recent', label: recentLabel });
      recent.forEach((item, index) => {
        rows.push({ type: 'item', key: item.id, item, index });
      });
      remaining = items.filter((x) => !recentIds.has(x.id));
    }
  }

  let last = '';
  remaining.forEach((item, index) => {
    const label = sort === 'title'
      ? (/[a-z]/i.test(item.title[0] ?? '') ? item.title[0].toUpperCase() : '#')
      : monthLabel(sort === 'created' ? item.createdAt : item.updatedAt);
    if (label !== last) {
      rows.push({ type: 'header', key: `h:${label}`, label });
      last = label;
    }
    rows.push({ type: 'item', key: item.id, item, index });
  });
  return rows;
}
