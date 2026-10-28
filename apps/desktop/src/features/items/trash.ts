import type { Key } from "../../i18n";

const MS_PER_DAY = 24 * 60 * 60 * 1000;

/** Mirrors gitpass-core `PURGE_AFTER_SECS` (30 days). */
export const PURGE_AFTER_DAYS = 30;

export type TranslateFn = (key: Key, vars?: Record<string, string | number>) => string;

/**
 * Whole days left until an item is purged (rounded up so the final day reads 0),
 * or null when `deletedAt` is missing/invalid. Past the deadline it stays 0.
 */
export function daysUntilPurge(deletedAt: string | null | undefined, now: number = Date.now()): number | null {
  if (!deletedAt) return null;
  const deleted = Date.parse(deletedAt);
  if (isNaN(deleted)) return null;
  return Math.max(0, Math.ceil((deleted + PURGE_AFTER_DAYS * MS_PER_DAY - now) / MS_PER_DAY));
}

export interface TrashHint { text: string; danger: boolean; days: number }

/** Row secondary line: "Deletes in N days" / "Deletes today", danger with ≤ 3 days left. */
export function trashHint(deletedAt: string | null | undefined, now: number, t: TranslateFn): TrashHint | null {
  const days = daysUntilPurge(deletedAt, now);
  if (days === null) return null;
  const text = days === 0 ? t("trash.deletesToday") : days === 1 ? t("trash.deletesInDayOne") : t("trash.deletesInDays", { n: days });
  return { text, danger: days <= 3, days };
}
