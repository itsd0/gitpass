import { describe, it, expect } from 'vitest';
import { formatRelativeLastUsed, parseLastUsedDate } from './lastUsedAge';

describe('lastUsedAge', () => {
  const baseNow = new Date('2026-10-29T12:00:00Z');

  it('parses timestamps in seconds and milliseconds and Date objects', () => {
    expect(parseLastUsedDate(1793275200)?.toISOString()).toBe('2026-10-29T12:00:00.000Z');
    expect(parseLastUsedDate(1793275200000)?.toISOString()).toBe('2026-10-29T12:00:00.000Z');
    expect(parseLastUsedDate('1793275200')?.toISOString()).toBe('2026-10-29T12:00:00.000Z');
    expect(parseLastUsedDate(baseNow)?.toISOString()).toBe('2026-10-29T12:00:00.000Z');
    expect(parseLastUsedDate(null)).toBeNull();
    expect(parseLastUsedDate(undefined)).toBeNull();
    expect(parseLastUsedDate('invalid')).toBeNull();
  });

  it('formats "Last used: today" when used within the current day', () => {
    const today = new Date('2026-10-29T10:00:00Z');
    expect(formatRelativeLastUsed(today, baseNow)).toBe('Last used: today');
    expect(formatRelativeLastUsed(Math.floor(today.getTime() / 1000), baseNow)).toBe('Last used: today');
  });

  it('formats "Last used: 1 day ago"', () => {
    const yesterday = new Date('2026-10-28T10:00:00Z');
    expect(formatRelativeLastUsed(yesterday, baseNow)).toBe('Last used: 1 day ago');
  });

  it('formats "Last used: 3 days ago"', () => {
    const threeDaysAgo = new Date('2026-10-26T12:00:00Z');
    expect(formatRelativeLastUsed(threeDaysAgo, baseNow)).toBe('Last used: 3 days ago');
    expect(formatRelativeLastUsed(Math.floor(threeDaysAgo.getTime() / 1000), baseNow)).toBe('Last used: 3 days ago');
  });

  it('formats months and years ago', () => {
    const twoMonthsAgo = new Date('2026-08-20T12:00:00Z');
    expect(formatRelativeLastUsed(twoMonthsAgo, baseNow)).toBe('Last used: 2 months ago');

    const oneYearAgo = new Date('2025-10-29T12:00:00Z');
    expect(formatRelativeLastUsed(oneYearAgo, baseNow)).toBe('Last used: 1 year ago');

    const twoYearsAgo = new Date('2024-10-29T12:00:00Z');
    expect(formatRelativeLastUsed(twoYearsAgo, baseNow)).toBe('Last used: 2 years ago');
  });

  it('uses translation function when provided', () => {
    const t = (key: string, vars?: Record<string, string | number>) => {
      if (key === 'detail.lastUsed.days') return `Used ${vars?.n}d ago`;
      if (key === 'detail.lastUsed.today') return 'Used today';
      return key;
    };
    const threeDaysAgo = new Date('2026-10-26T12:00:00Z');
    expect(formatRelativeLastUsed(threeDaysAgo, baseNow, t as any)).toBe('Used 3d ago');
  });
});
