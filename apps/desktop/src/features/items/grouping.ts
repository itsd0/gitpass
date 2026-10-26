import type { ItemSummary, Filter } from '../../api/types';
export type Row = { type: 'header'; key: string; label: string } | { type: 'item'; key: string; item: ItemSummary; index: number };

export const monthLabel = (iso: string) => new Date(iso).toLocaleString('en-US', { month: 'long', year: 'numeric', timeZone: 'UTC' }).toUpperCase();

/** Flatten items into header+item rows. Month headers for date sorts, A–Z letter headers for title sort. Preserves input order. */
export function groupRows(items: ItemSummary[], sort: Filter['sort'] = 'updated'): Row[] {
  const rows: Row[] = []; let last = '';
  items.forEach((item, index) => {
    const label = sort === 'title' ? (/[a-z]/i.test(item.title[0] ?? '') ? item.title[0].toUpperCase() : '#') : monthLabel(sort === 'created' ? item.createdAt : item.updatedAt);
    if (label !== last) { rows.push({ type: 'header', key: `h:${label}`, label }); last = label; }
    rows.push({ type: 'item', key: item.id, item, index });
  });
  return rows;
}
