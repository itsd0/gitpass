import type { FieldKind, ItemCategory, Section } from "../../api/types";
import type { Key } from "../../i18n";

export type ExpiryStatus = "expired" | "expiring_soon";

export interface ExpiryResult {
  status: ExpiryStatus;
  date: Date;
  diffDays: number;
  relativeHint: string;
}

export type TranslateFn = (key: Key, vars?: Record<string, string | number>) => string;

const MS_PER_DAY = 24 * 60 * 60 * 1000;

/**
 * Calendar day difference (date - now) based on UTC calendar dates.
 * Positive = future, negative = past.
 */
export function calendarDiffDays(date: Date, now: Date): number {
  const startOfNow = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
  const startOfDate = Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate());
  return Math.round((startOfDate - startOfNow) / MS_PER_DAY);
}

/**
 * Parse month_year format (e.g. "05/2026", "2026/05", "2026-05", "05-2026", "05/26", "202605").
 * Counts as valid until the very end of that month (23:59:59.999 UTC).
 */
export function parseMonthYear(raw: string): Date | null {
  const s = raw.trim();
  if (!s) return null;
  let year: number | null = null;
  let month: number | null = null;

  if (/^\d{4}[-/]\d{1,2}$/.test(s)) {
    const parts = s.split(/[-/]/);
    year = Number(parts[0]);
    month = Number(parts[1]);
  } else if (/^\d{1,2}[-/]\d{4}$/.test(s)) {
    const parts = s.split(/[-/]/);
    month = Number(parts[0]);
    year = Number(parts[1]);
  } else if (/^\d{1,2}[-/]\d{2}$/.test(s)) {
    const parts = s.split(/[-/]/);
    month = Number(parts[0]);
    year = 2000 + Number(parts[1]);
  } else if (/^\d{6}$/.test(s)) {
    year = Number(s.slice(0, 4));
    month = Number(s.slice(4, 6));
  } else {
    return null;
  }

  if (!year || !month || isNaN(year) || isNaN(month) || month < 1 || month > 12 || year < 1900 || year > 2100) {
    return null;
  }

  // Date.UTC(year, month, 0) gives the last day of month (since month is 1-indexed here,
  // month in Date.UTC is 0-indexed, so Date.UTC(year, month, 0) is day 0 of next month).
  return new Date(Date.UTC(year, month, 0, 23, 59, 59, 999));
}

/**
 * Parse date format (e.g. "2026-10-31", "2026/10/31", "10/31/2026", or ISO-8601).
 * Date without time counts as valid until the very end of that day (23:59:59.999 UTC).
 */
export function parseDate(raw: string): Date | null {
  const s = raw.trim();
  if (!s) return null;

  if (s.includes("T")) {
    const d = new Date(s);
    return isNaN(d.getTime()) ? null : d;
  }

  if (/^\d{4}[-/]\d{1,2}[-/]\d{1,2}$/.test(s)) {
    const [y, m, d] = s.split(/[-/]/).map(Number);
    if (m < 1 || m > 12 || d < 1 || d > 31 || y < 1900 || y > 2100) return null;
    const endOfDay = new Date(Date.UTC(y, m - 1, d, 23, 59, 59, 999));
    if (endOfDay.getUTCMonth() !== m - 1) return null; // invalid day, e.g. Feb 31
    return endOfDay;
  }

  if (/^\d{1,2}[-/]\d{1,2}[-/]\d{4}$/.test(s)) {
    const [m, d, y] = s.split(/[-/]/).map(Number);
    if (m < 1 || m > 12 || d < 1 || d > 31 || y < 1900 || y > 2100) return null;
    const endOfDay = new Date(Date.UTC(y, m - 1, d, 23, 59, 59, 999));
    if (endOfDay.getUTCMonth() !== m - 1) return null;
    return endOfDay;
  }

  const t = Date.parse(s);
  return isNaN(t) ? null : new Date(t);
}

export function parseExpiry(raw: string | null | undefined, kind?: FieldKind | "month_year" | "date"): Date | null {
  if (!raw || typeof raw !== "string") return null;
  const s = raw.trim();
  if (!s) return null;

  if (kind === "month_year") return parseMonthYear(s);
  if (kind === "date") return parseDate(s);

  // Auto-detect format
  if (s.includes("T") || (s.match(/[-/]/g) || []).length === 2) {
    return parseDate(s) ?? parseMonthYear(s);
  }
  return parseMonthYear(s) ?? parseDate(s);
}

export function formatRelativeExpiry(date: Date, now: Date, t?: TranslateFn): string {
  const diffMs = date.getTime() - now.getTime();
  const diffDays = calendarDiffDays(date, now);

  if (diffMs < 0) {
    // Expired
    const pastDays = Math.max(0, -diffDays);
    const monthDiff = (now.getUTCFullYear() - date.getUTCFullYear()) * 12 + (now.getUTCMonth() - date.getUTCMonth());
    if (monthDiff >= 12) {
      const years = Math.floor(monthDiff / 12);
      return years === 1
        ? (t ? t("expiry.expiredYearOne") : "Expired 1 year ago")
        : (t ? t("expiry.expiredYears", { n: years }) : `Expired ${years} years ago`);
    }
    if (monthDiff >= 2) {
      return t ? t("expiry.expiredMonths", { n: monthDiff }) : `Expired ${monthDiff} months ago`;
    }
    if (monthDiff === 1 && pastDays >= 28) {
      return t ? t("expiry.expiredMonthOne") : "Expired 1 month ago";
    }
    if (pastDays > 1) {
      return t ? t("expiry.expiredDays", { n: pastDays }) : `Expired ${pastDays} days ago`;
    }
    if (pastDays === 1) {
      return t ? t("expiry.expiredDayOne") : "Expired 1 day ago";
    }
    return t ? t("expiry.expiredToday") : "Expired today";
  }

  // Future / Expiring soon
  const futureDays = Math.max(0, diffDays);
  if (futureDays === 0) {
    return t ? t("expiry.expiresToday") : "Expires today";
  }
  if (futureDays === 1) {
    return t ? t("expiry.expiresInDayOne") : "Expires in 1 day";
  }
  if (futureDays <= 30) {
    return t ? t("expiry.expiresInDays", { n: futureDays }) : `Expires in ${futureDays} days`;
  }
  const monthDiff = (date.getUTCFullYear() - now.getUTCFullYear()) * 12 + (date.getUTCMonth() - now.getUTCMonth());
  if (monthDiff >= 12) {
    const years = Math.floor(monthDiff / 12);
    return years === 1
      ? (t ? t("expiry.expiresInYearOne") : "Expires in 1 year")
      : (t ? t("expiry.expiresInYears", { n: years }) : `Expires in ${years} years`);
  }
  return monthDiff <= 1
    ? (t ? t("expiry.expiresInMonthOne") : "Expires in 1 month")
    : (t ? t("expiry.expiresInMonths", { n: monthDiff }) : `Expires in ${monthDiff} months`);
}

export function checkExpiry(raw: string | null | undefined, kind?: FieldKind | "month_year" | "date", now = new Date(), t?: TranslateFn): ExpiryResult | null {
  const date = parseExpiry(raw, kind);
  if (!date) return null;

  const diffMs = date.getTime() - now.getTime();
  const diffDays = calendarDiffDays(date, now);

  if (diffMs < 0) {
    return { status: "expired", date, diffDays, relativeHint: formatRelativeExpiry(date, now, t) };
  }

  const sameMonthYear = date.getUTCFullYear() === now.getUTCFullYear() && date.getUTCMonth() === now.getUTCMonth();
  if (diffDays <= 30 || sameMonthYear) {
    return { status: "expiring_soon", date, diffDays, relativeHint: formatRelativeExpiry(date, now, t) };
  }

  return null;
}

export function isExpiryField(field: { id?: string; label?: string; kind?: FieldKind }, category?: ItemCategory): boolean {
  if (category) {
    if (category !== "credit_card" && category !== "api_credential") return false;
    if (category === "credit_card") {
      return field.id === "expiry" || /expiry/i.test(field.label ?? "") || (field.kind === "month_year" && field.id !== "valid_from" && !/valid\s*from/i.test(field.label ?? ""));
    }
    if (category === "api_credential") {
      return field.id === "expires" || /expires?/i.test(field.label ?? "") || (field.kind === "date" && field.id !== "birth" && !/birth/i.test(field.label ?? ""));
    }
  }
  if (field.id === "expiry" || field.id === "expires") return true;
  if (/valid\s*from|birth/i.test(field.label ?? "") || field.id === "valid_from" || field.id === "birth") return false;
  return (field.kind === "month_year" || field.kind === "date") && /expiry|expires?/i.test(field.label ?? "");
}

export function getItemExpiry(item: { category?: ItemCategory; expiry?: string | null; sections?: Section[] } | null | undefined, now?: Date, t?: TranslateFn): ExpiryResult | null {
  if (!item || (item.category !== "credit_card" && item.category !== "api_credential")) return null;
  if (item.expiry) return checkExpiry(item.expiry, item.category === "credit_card" ? "month_year" : "date", now, t);
  if (item.sections) {
    const fields = item.sections.flatMap((s) => s.fields);
    if (item.category === "credit_card") {
      const f = fields.find((x) => isExpiryField(x, "credit_card"));
      if (f?.value) return checkExpiry(f.value, "month_year", now, t);
    } else if (item.category === "api_credential") {
      const f = fields.find((x) => isExpiryField(x, "api_credential"));
      if (f?.value) return checkExpiry(f.value, "date", now, t);
    }
  }
  return null;
}
