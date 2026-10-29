import { describe, it, expect } from 'vitest';
import { groupRows } from './grouping';
import type { ItemSummary } from '../../api/types';

const it_ = (id: string, title: string, updatedAt: string, lastUsed?: number): ItemSummary => ({
  id,
  vaultId: 'v',
  category: 'login',
  title,
  subtitle: '',
  favorite: false,
  archived: false,
  deletedAt: null,
  tags: [],
  updatedAt,
  createdAt: updatedAt,
  lastUsed,
});

describe('groupRows', () => {
  const items = [
    it_('1', 'b', '2026-10-20T00:00:00Z', 1793200000),
    it_('2', 'a', '2026-10-01T00:00:00Z', 1793210000),
    it_('3', 'c', '2026-09-30T00:00:00Z'),
  ];

  it('groups by month with uppercase headers, preserving order', () => {
    const rows = groupRows(items, 'updated');
    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual([
      'OCTOBER 2026',
      '1',
      '2',
      'SEPTEMBER 2026',
      '3',
    ]);
  });

  it('uses letter headers for title sort', () => {
    const rows = groupRows([items[1], items[0]], 'title');
    expect(rows.filter((r) => r.type === 'header').map((r) => r.type === 'header' && r.label)).toEqual([
      'A',
      'B',
    ]);
  });

  it('groups by recently used and never used when sort is recent', () => {
    const sorted = [items[1], items[0], items[2]];
    const rows = groupRows(sorted, 'recent');
    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual([
      'RECENTLY USED',
      '2',
      '1',
      'NEVER USED',
      '3',
    ]);
  });

  it('renders a recently used section at the top when showRecentSection is true (max 6)', () => {
    const rows = groupRows(items, 'updated', { showRecentSection: true });
    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual([
      'RECENTLY USED',
      '2', // item 2 was used more recently (1793210000)
      '1', // item 1 was used (1793200000)
      'SEPTEMBER 2026',
      '3',
    ]);
  });

  it('hides the recently used section when no items have been used', () => {
    const unusedItems = [
      it_('1', 'b', '2026-10-20T00:00:00Z'),
      it_('2', 'a', '2026-10-01T00:00:00Z'),
    ];
    const rows = groupRows(unusedItems, 'updated', { showRecentSection: true });
    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual([
      'OCTOBER 2026',
      '1',
      '2',
    ]);
  });

  it('empty list -> no rows', () => expect(groupRows([], 'updated')).toEqual([]));
});
