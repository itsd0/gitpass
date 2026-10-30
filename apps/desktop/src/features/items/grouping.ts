import type { ItemSummary, Filter } from '../../api/types';

export type Row =
  | { type: 'header'; key: string; label: string }
  | { type: 'item'; key: string; item: ItemSummary; index: number; headerKey?: string };

export const monthLabel = (iso: string, locale: string = 'en') => {
  const d = new Date(iso);
  return isNaN(d.getTime())
    ? ''
    : new Intl.DateTimeFormat(locale, { month: 'long', year: 'numeric' }).format(d);
};

export type DateBucketType =
  | 'today'
  | 'yesterday'
  | 'previous7Days'
  | 'previous30Days'
  | 'month';

export interface DateBucket {
  type: DateBucketType;
  key: string;
  label: string;
}

export interface DateBucketOptions {
  locale?: string;
  todayLabel?: string;
  yesterdayLabel?: string;
  previous7DaysLabel?: string;
  previous30DaysLabel?: string;
  now?: Date;
}

export interface GroupRowsOptions extends DateBucketOptions {
  /** Return plain item rows with no section headers (e.g. search results, which are ranked by relevance). */
  flat?: boolean;
}

export function parseItemDate(value: string | number | Date | null | undefined): Date | null {
  if (value == null) return null;
  if (value instanceof Date) return isNaN(value.getTime()) ? null : value;
  if (typeof value === 'number') {
    if (value <= 0) return null;
    return new Date(value > 1e11 ? value : value * 1000);
  }
  const trimmed = String(value).trim();
  if (/^\d+$/.test(trimmed)) {
    const num = Number(trimmed);
    if (num <= 0) return null;
    return new Date(num > 1e11 ? num : num * 1000);
  }
  const d = new Date(trimmed);
  return isNaN(d.getTime()) ? null : d;
}

export function getDateBucket(
  dateInput: Date | string | number | null | undefined,
  options?: DateBucketOptions
): DateBucket | null {
  const d = parseItemDate(dateInput);
  if (!d) return null;

  const now = options?.now ?? new Date();
  const startOfToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
    0,
    0,
    0,
    0
  ).getTime();

  const startOfYesterday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() - 1,
    0,
    0,
    0,
    0
  ).getTime();

  const startOf7DaysAgo = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() - 7,
    0,
    0,
    0,
    0
  ).getTime();

  const startOf30DaysAgo = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() - 30,
    0,
    0,
    0,
    0
  ).getTime();

  const time = d.getTime();

  if (time >= startOfToday) {
    return {
      type: 'today',
      key: 'today',
      label: options?.todayLabel ?? 'Today',
    };
  }

  if (time >= startOfYesterday) {
    return {
      type: 'yesterday',
      key: 'yesterday',
      label: options?.yesterdayLabel ?? 'Yesterday',
    };
  }

  if (time >= startOf7DaysAgo) {
    return {
      type: 'previous7Days',
      key: 'previous7Days',
      label: options?.previous7DaysLabel ?? 'Previous 7 days',
    };
  }

  if (time >= startOf30DaysAgo) {
    return {
      type: 'previous30Days',
      key: 'previous30Days',
      label: options?.previous30DaysLabel ?? 'Previous 30 days',
    };
  }

  const locale = options?.locale ?? 'en';
  const label = new Intl.DateTimeFormat(locale, {
    month: 'long',
    year: 'numeric',
  }).format(d);
  const monthKey = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;

  return {
    type: 'month',
    key: `month:${monthKey}`,
    label,
  };
}

/** Flatten items into header+item rows. Date headers for date sorts, A–Z letter headers for title sort. Preserves input order. */
export function groupRows(
  items: ItemSummary[],
  sort: Filter['sort'] = 'updated',
  options?: GroupRowsOptions
): Row[] {
  const rows: Row[] = [];
  if (options?.flat) {
    return items.map((item, index) => ({ type: 'item', key: item.id, item, index }));
  }

  let lastKey = '';
  items.forEach((item, index) => {
    let headerKey = '';
    let headerLabel = '';

    if (sort === 'title') {
      const letter = /[a-z]/i.test(item.title[0] ?? '') ? item.title[0].toUpperCase() : '#';
      headerKey = `h:letter:${letter}`;
      headerLabel = letter;
    } else {
      const dateVal = sort === 'created' ? item.createdAt : item.updatedAt;
      const bucket = getDateBucket(dateVal, options);
      if (bucket) {
        headerKey = `h:${bucket.key}`;
        headerLabel = bucket.label;
      }
    }

    if (headerKey && headerKey !== lastKey) {
      rows.push({ type: 'header', key: headerKey, label: headerLabel });
      lastKey = headerKey;
    }
    rows.push({ type: 'item', key: item.id, item, index, headerKey });
  });

  return rows;
}
