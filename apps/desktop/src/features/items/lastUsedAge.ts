import type { Key } from '../../i18n';

export type TranslateFn = (key: Key, vars?: Record<string, string | number>) => string;

export function parseLastUsedDate(s: string | number | Date | null | undefined): Date | null {
  if (s == null) return null;
  if (s instanceof Date) return isNaN(s.getTime()) ? null : s;
  if (typeof s === 'number') return new Date(s > 1e11 ? s : s * 1000);
  const trimmed = String(s).trim();
  if (/^\d+$/.test(trimmed)) {
    const num = Number(trimmed);
    return new Date(num > 1e11 ? num : num * 1000);
  }
  const d = new Date(trimmed);
  return isNaN(d.getTime()) ? null : d;
}

/**
 * Format when an item was last used into a relative string:
 * "Last used: today", "Last used: 1 day ago", "Last used: 3 days ago",
 * "Last used: 1 month ago", "Last used: 2 months ago",
 * "Last used: 1 year ago", "Last used: 2 years ago".
 */
export function formatRelativeLastUsed(
  dateOrTimestamp: Date | string | number | null | undefined,
  now: Date = new Date(),
  t?: TranslateFn
): string {
  const date = parseLastUsedDate(dateOrTimestamp);
  if (!date) return '';

  const diffMs = now.getTime() - date.getTime();
  if (diffMs < 0) {
    return t ? t('detail.lastUsed.today') : 'Last used: today';
  }

  const diffDays = Math.floor(diffMs / 86400000);
  const monthDiff = (now.getUTCFullYear() - date.getUTCFullYear()) * 12 + (now.getUTCMonth() - date.getUTCMonth());

  if (monthDiff >= 12) {
    const years = Math.floor(monthDiff / 12);
    return years === 1
      ? (t ? t('detail.lastUsed.yearOne') : 'Last used: 1 year ago')
      : (t ? t('detail.lastUsed.years', { n: years }) : `Last used: ${years} years ago`);
  }
  if (monthDiff >= 2) {
    return t ? t('detail.lastUsed.months', { n: monthDiff }) : `Last used: ${monthDiff} months ago`;
  }
  if (monthDiff === 1 && diffDays >= 28) {
    return t ? t('detail.lastUsed.monthOne') : 'Last used: 1 month ago';
  }
  if (diffDays > 1) {
    return t ? t('detail.lastUsed.days', { n: diffDays }) : `Last used: ${diffDays} days ago`;
  }
  if (diffDays === 1) {
    return t ? t('detail.lastUsed.dayOne') : 'Last used: 1 day ago';
  }
  return t ? t('detail.lastUsed.today') : 'Last used: today';
}
