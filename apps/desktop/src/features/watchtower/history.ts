import type { WatchtowerReport } from '../../api/types';
import { issueIds, type IssueKey } from './score';

/** One daily data point. Counts only: no item ids, titles or other vault content. */
export interface ScorePoint { date: string; score: number; counts: Record<string, number> }
export type HistoryRange = 30 | 90;

/** Per-device history (localStorage); never written to the vault repo or synced. */
export const SCORE_HISTORY_KEY = 'gitpass.watchtowerHistory';
export const MAX_HISTORY_DAYS = 90;

const COUNT_KEYS: IssueKey[] = ['weak', 'reused', 'reusedOldPassword', 'old', 'breached', 'missingTotp', 'totpAvailable', 'passkeyAvailable', 'expired', 'expiringSoon', 'insecureUrls', 'duplicates', 'weakSshKey', 'unused', 'rotationDue', 'rotationSoon'];
const DAY_MS = 86_400_000;

/** Local calendar day as YYYY-MM-DD. */
export function dayKey(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
/** Whole days between two YYYY-MM-DD keys (b - a). */
export function daysBetween(a: string, b: string): number {
  return Math.round((Date.parse(`${b}T00:00:00Z`) - Date.parse(`${a}T00:00:00Z`)) / DAY_MS);
}
const isPoint = (p: unknown): p is ScorePoint => {
  const q = p as ScorePoint | null;
  return !!q && typeof q.date === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(q.date) && typeof q.score === 'number' && Number.isFinite(q.score);
};

export function pointFromReport(report: WatchtowerReport, now: Date): ScorePoint {
  const counts: Record<string, number> = {};
  for (const k of COUNT_KEYS) { const n = issueIds(report, k).length; if (n) counts[k] = n; }
  return { date: dayKey(now), score: Math.max(0, Math.min(100, Math.round(report.score))), counts };
}

/** Adds a point, keeping only the latest one per day, then drops anything older than 90 days. Returns a sorted copy. */
export function addPoint(points: ScorePoint[], point: ScorePoint, now: Date = new Date()): ScorePoint[] {
  const today = dayKey(now);
  const byDay = new Map<string, ScorePoint>();
  for (const p of [...points, point]) byDay.set(p.date, p);
  return [...byDay.values()]
    .filter((p) => { const age = daysBetween(p.date, today); return age >= 0 && age < MAX_HISTORY_DAYS; })
    .sort((a, b) => a.date.localeCompare(b.date));
}

export function inRange(points: ScorePoint[], range: HistoryRange, now: Date = new Date()): ScorePoint[] {
  const today = dayKey(now);
  return points.filter((p) => daysBetween(p.date, today) < range);
}

/** Score change against the latest point at least 7 days old; before that, against the oldest point. */
export function weeklyDelta(points: ScorePoint[], now: Date = new Date()): { delta: number; since: string; week: boolean } | null {
  if (points.length < 2) return null;
  const today = dayKey(now);
  const last = points[points.length - 1]!;
  const olderThanWeek = points.filter((p) => daysBetween(p.date, today) >= 7);
  const base = olderThanWeek.length ? olderThanWeek[olderThanWeek.length - 1]! : points[0]!;
  if (base === last) return null;
  return { delta: last.score - base.score, since: base.date, week: olderThanWeek.length > 0 };
}

export function loadHistory(): ScorePoint[] {
  try {
    const raw = JSON.parse(localStorage.getItem(SCORE_HISTORY_KEY) ?? '[]');
    return Array.isArray(raw) ? raw.filter(isPoint).map((p) => ({ date: p.date, score: p.score, counts: p.counts && typeof p.counts === 'object' ? p.counts : {} })) : [];
  } catch { return []; }
}

/** Records the report as today's point and returns the stored history. */
export function recordReport(report: WatchtowerReport, now: Date = new Date()): ScorePoint[] {
  const next = addPoint(loadHistory(), pointFromReport(report, now), now);
  try { localStorage.setItem(SCORE_HISTORY_KEY, JSON.stringify(next)); } catch { /* storage unavailable: keep in memory only */ }
  return next;
}
