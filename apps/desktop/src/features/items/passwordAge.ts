import type { Key } from '../../i18n';

export type TranslateFn = (key: Key, vars?: Record<string, string | number>) => string;

export function parseDate(s: string): Date | null {
  if (!s) return null;
  const trimmed = s.trim();
  if (/^\d+$/.test(trimmed)) {
    const secs = Number(trimmed);
    return new Date(secs * 1000);
  }
  const d = new Date(trimmed);
  return isNaN(d.getTime()) ? null : d;
}

/**
 * Format when the password was changed into a relative string:
 * "Password changed today", "Password changed 1 day ago", "Password changed {n} days ago",
 * "Password changed 1 month ago", "Password changed {n} months ago",
 * "Password changed 1 year ago", "Password changed {n} years ago".
 */
export function formatRelativePasswordChanged(
  dateOrStr: Date | string,
  now: Date = new Date(),
  t?: TranslateFn
): string {
  const date = typeof dateOrStr === 'string' ? parseDate(dateOrStr) : dateOrStr;
  if (!date) return '';

  const diffMs = now.getTime() - date.getTime();
  if (diffMs < 0) {
    return t ? t('passwordChanged.today') : 'Password changed today';
  }

  const diffDays = Math.floor(diffMs / 86400000);
  const monthDiff = (now.getUTCFullYear() - date.getUTCFullYear()) * 12 + (now.getUTCMonth() - date.getUTCMonth());

  if (monthDiff >= 12) {
    const years = Math.floor(monthDiff / 12);
    return years === 1
      ? (t ? t('passwordChanged.yearOne') : 'Password changed 1 year ago')
      : (t ? t('passwordChanged.years', { n: years }) : `Password changed ${years} years ago`);
  }
  if (monthDiff >= 2) {
    return t ? t('passwordChanged.months', { n: monthDiff }) : `Password changed ${monthDiff} months ago`;
  }
  if (monthDiff === 1 && diffDays >= 28) {
    return t ? t('passwordChanged.monthOne') : 'Password changed 1 month ago';
  }
  if (diffDays > 1) {
    return t ? t('passwordChanged.days', { n: diffDays }) : `Password changed ${diffDays} days ago`;
  }
  if (diffDays === 1) {
    return t ? t('passwordChanged.dayOne') : 'Password changed 1 day ago';
  }
  return t ? t('passwordChanged.today') : 'Password changed today';
}
