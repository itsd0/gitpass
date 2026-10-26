import { describe, it, expect } from 'vitest';
import { groupRows } from './grouping';
import type { ItemSummary } from '../../api/types';
const it_ = (id: string, title: string, updatedAt: string): ItemSummary => ({ id, vaultId: 'v', category: 'login', title, subtitle: '', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt, createdAt: updatedAt });
describe('groupRows', () => {
  const items = [it_('1', 'b', '2026-10-20T00:00:00Z'), it_('2', 'a', '2026-10-01T00:00:00Z'), it_('3', 'c', '2026-09-30T00:00:00Z')];
  it('groups by month with uppercase headers, preserving order', () => {
    const rows = groupRows(items, 'updated');
    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual(['OCTOBER 2026', '1', '2', 'SEPTEMBER 2026', '3']);
  });
  it('uses letter headers for title sort', () => {
    const rows = groupRows([items[1], items[0]], 'title');
    expect(rows.filter((r) => r.type === 'header').map((r) => r.type === 'header' && r.label)).toEqual(['A', 'B']);
  });
  it('empty list -> no rows', () => expect(groupRows([], 'updated')).toEqual([]));
});
