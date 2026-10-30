import { describe, it, expect } from 'vitest';
import { getDateBucket, groupRows, monthLabel, parseItemDate } from './grouping';
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

describe('getDateBucket', () => {
  // Use a fixed local time: 2026-10-29 at 15:30:00 local time
  const now = new Date(2026, 9, 29, 15, 30, 0);

  it('handles invalid or empty dates', () => {
    expect(getDateBucket(null)).toBeNull();
    expect(getDateBucket(undefined)).toBeNull();
    expect(getDateBucket('invalid')).toBeNull();
    expect(getDateBucket(0)).toBeNull();
    expect(getDateBucket(-100)).toBeNull();
  });

  it('parses timestamps in seconds and milliseconds', () => {
    // 1793200000 s is in 2026
    const parsedSec = parseItemDate(1793200000);
    expect(parsedSec).not.toBeNull();
    expect(parsedSec?.getFullYear()).toBe(2026);

    const parsedMs = parseItemDate(1793200000 * 1000);
    expect(parsedMs?.getTime()).toBe(parsedSec?.getTime());
  });

  it('buckets correctly at the local midnight boundary between Today and Yesterday', () => {
    // Local midnight today: 2026-10-29 00:00:00.000
    const midnightToday = new Date(2026, 9, 29, 0, 0, 0, 0);
    // 1 millisecond before midnight today: 2026-10-28 23:59:59.999
    const beforeMidnight = new Date(2026, 9, 28, 23, 59, 59, 999);

    const todayBucket = getDateBucket(midnightToday, { now });
    expect(todayBucket?.type).toBe('today');
    expect(todayBucket?.label).toBe('Today');

    const yesterdayBucket = getDateBucket(beforeMidnight, { now });
    expect(yesterdayBucket?.type).toBe('yesterday');
    expect(yesterdayBucket?.label).toBe('Yesterday');
  });

  it('buckets correctly at the local midnight boundary between Yesterday and Previous 7 days', () => {
    // Local midnight yesterday: 2026-10-28 00:00:00.000
    const midnightYesterday = new Date(2026, 9, 28, 0, 0, 0, 0);
    // 1 millisecond before midnight yesterday: 2026-10-27 23:59:59.999
    const beforeYesterday = new Date(2026, 9, 27, 23, 59, 59, 999);

    const yesterdayBucket = getDateBucket(midnightYesterday, { now });
    expect(yesterdayBucket?.type).toBe('yesterday');

    const prev7Bucket = getDateBucket(beforeYesterday, { now });
    expect(prev7Bucket?.type).toBe('previous7Days');
    expect(prev7Bucket?.label).toBe('Previous 7 days');
  });

  it('buckets correctly at the 7-day boundary', () => {
    // 7 days ago local midnight: 2026-10-22 00:00:00.000
    const sevenDaysAgoMidnight = new Date(2026, 9, 22, 0, 0, 0, 0);
    // 1 ms before: 2026-10-21 23:59:59.999
    const eightDaysAgo = new Date(2026, 9, 21, 23, 59, 59, 999);

    expect(getDateBucket(sevenDaysAgoMidnight, { now })?.type).toBe('previous7Days');
    expect(getDateBucket(eightDaysAgo, { now })?.type).toBe('previous30Days');
  });

  it('buckets correctly at the 30-day boundary and formats month for older items', () => {
    // 30 days ago local midnight: 2026-09-29 00:00:00.000
    const thirtyDaysAgo = new Date(2026, 8, 29, 0, 0, 0, 0);
    // 1 ms before: 2026-09-28 23:59:59.999
    const olderItem = new Date(2026, 8, 28, 23, 59, 59, 999);

    expect(getDateBucket(thirtyDaysAgo, { now })?.type).toBe('previous30Days');

    const monthBucket = getDateBucket(olderItem, { now, locale: 'en' });
    expect(monthBucket?.type).toBe('month');
    expect(monthBucket?.label).toBe('September 2026');
  });

  it('formats month names using locale in en, my, th', () => {
    const older = new Date(2026, 7, 15, 12, 0, 0); // August 2026

    const enBucket = getDateBucket(older, { now, locale: 'en' });
    expect(enBucket?.label).toBe('August 2026');

    const myBucket = getDateBucket(older, { now, locale: 'my' });
    expect(myBucket?.label).toBe(new Intl.DateTimeFormat('my', { month: 'long', year: 'numeric' }).format(older));

    const thBucket = getDateBucket(older, { now, locale: 'th' });
    expect(thBucket?.label).toBe(new Intl.DateTimeFormat('th', { month: 'long', year: 'numeric' }).format(older));
  });

  it('respects custom label overrides', () => {
    const today = new Date(2026, 9, 29, 10, 0, 0);
    const yesterday = new Date(2026, 9, 28, 10, 0, 0);
    const fourDays = new Date(2026, 9, 25, 10, 0, 0);
    const twentyDays = new Date(2026, 9, 9, 10, 0, 0);

    const opts = {
      now,
      todayLabel: 'Hôm nay',
      yesterdayLabel: 'Hôm qua',
      previous7DaysLabel: '7 ngày trước',
      previous30DaysLabel: '30 ngày trước',
    };

    expect(getDateBucket(today, opts)?.label).toBe('Hôm nay');
    expect(getDateBucket(yesterday, opts)?.label).toBe('Hôm qua');
    expect(getDateBucket(fourDays, opts)?.label).toBe('7 ngày trước');
    expect(getDateBucket(twentyDays, opts)?.label).toBe('30 ngày trước');
  });
});

describe('groupRows', () => {
  const now = new Date(2026, 9, 29, 12, 0, 0);

  const itemToday = it_('1', 'Alpha', new Date(2026, 9, 29, 9, 0, 0).toISOString(), Math.floor(new Date(2026, 9, 29, 9, 0, 0).getTime() / 1000));
  const itemYesterday = it_('2', 'Beta', new Date(2026, 9, 28, 14, 0, 0).toISOString(), Math.floor(new Date(2026, 9, 28, 14, 0, 0).getTime() / 1000));
  const itemPrev7 = it_('3', 'Charlie', new Date(2026, 9, 25, 10, 0, 0).toISOString(), Math.floor(new Date(2026, 9, 25, 10, 0, 0).getTime() / 1000));
  const itemPrev30 = it_('4', 'Delta', new Date(2026, 9, 10, 10, 0, 0).toISOString(), Math.floor(new Date(2026, 9, 10, 10, 0, 0).getTime() / 1000));
  const itemOlder1 = it_('5', 'Echo', new Date(2026, 7, 20, 10, 0, 0).toISOString(), Math.floor(new Date(2026, 7, 20, 10, 0, 0).getTime() / 1000)); // Aug 2026
  const itemOlder2 = it_('6', 'Foxtrot', new Date(2026, 6, 15, 10, 0, 0).toISOString()); // Jul 2026, never used
  const itemNeverUsed = it_('7', 'Golf', new Date(2026, 9, 29, 8, 0, 0).toISOString()); // today, but lastUsed is undefined

  it('groups items into date buckets when sorted by updated', () => {
    const list = [itemToday, itemYesterday, itemPrev7, itemPrev30, itemOlder1];
    const rows = groupRows(list, 'updated', { now, locale: 'en' });

    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual([
      'Today',
      '1',
      'Yesterday',
      '2',
      'Previous 7 days',
      '3',
      'Previous 30 days',
      '4',
      'August 2026',
      '5',
    ]);
  });

  it('groups items into date buckets when sorted by created', () => {
    const list = [itemToday, itemYesterday];
    const rows = groupRows(list, 'created', { now, locale: 'en' });
    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual([
      'Today',
      '1',
      'Yesterday',
      '2',
    ]);
  });

  it('groups by recently used date buckets and puts items with no last-used time under Never used', () => {
    const list = [itemToday, itemYesterday, itemNeverUsed];
    const rows = groupRows(list, 'recent', { now, locale: 'en', neverUsedLabel: 'Never used' });

    expect(rows.map((r) => (r.type === 'header' ? r.label : r.item.id))).toEqual([
      'Today',
      '1',
      'Yesterday',
      '2',
      'Never used',
      '7',
    ]);
  });

  it('uses letter headers and no date headers when sorted by title', () => {
    const list = [itemToday, itemYesterday, itemPrev7]; // Alpha (A), Beta (B), Charlie (C)
    const rows = groupRows(list, 'title', { now });

    expect(rows.filter((r) => r.type === 'header').map((r) => r.label)).toEqual([
      'A',
      'B',
      'C',
    ]);
    expect(rows.filter((r) => r.type === 'header').some((r) => r.label === 'Today')).toBe(false);
  });

  it('renders a Recently used section at top when showRecentSection is true for updated sort', () => {
    const list = [itemToday, itemYesterday, itemPrev7];
    const rows = groupRows(list, 'updated', {
      now,
      showRecentSection: true,
      recentLabel: 'Recently used',
    });

    expect(rows[0]).toEqual({
      type: 'header',
      key: 'h:recent',
      label: 'Recently used',
    });
  });

  it('returns plain item rows with no headers when flat is set', () => {
    const list = [itemYesterday, itemToday, itemOlder1];
    const rows = groupRows(list, 'updated', { flat: true, now, showRecentSection: true });
    expect(rows.every((r) => r.type === 'item')).toBe(true);
    expect(rows.map((r) => (r.type === 'item' ? r.item.id : ''))).toEqual(['2', '1', '5']);
  });

  it('hides empty sections', () => {
    // Only today and August 2026 items
    const list = [itemToday, itemOlder1];
    const rows = groupRows(list, 'updated', { now, locale: 'en' });

    const headers = rows.filter((r) => r.type === 'header').map((r) => r.label);
    expect(headers).toEqual(['Today', 'August 2026']);
    expect(headers).not.toContain('Yesterday');
    expect(headers).not.toContain('Previous 7 days');
    expect(headers).not.toContain('Previous 30 days');
  });

  it('returns empty array when items list is empty', () => {
    expect(groupRows([], 'updated')).toEqual([]);
  });

  it('monthLabel helper formats ISO string to localized month name', () => {
    expect(monthLabel('2026-10-20T00:00:00Z', 'en')).toBe('October 2026');
    expect(monthLabel('invalid')).toBe('');
  });
});
