import { beforeEach, describe, expect, it } from 'vitest';
import type { WatchtowerReport } from '../../api/types';
import { addPoint, dayKey, inRange, loadHistory, pointFromReport, recordReport, SCORE_HISTORY_KEY, weeklyDelta, type ScorePoint } from './history';

const report = (score: number, over: Partial<WatchtowerReport> = {}): WatchtowerReport => ({ score, weak: ['secret-id-1', 'secret-id-2'], reused: [['a', 'b']], old: [], breached: [], missingTotp: [], totpAvailable: [], passkeyAvailable: [], expired: [], expiringSoon: [], insecureUrls: [], duplicates: [], ignored: [], checkedAt: '', hibpChecked: false, ...over });
const day = (iso: string) => new Date(`${iso}T12:00:00`);
const pt = (date: string, score: number): ScorePoint => ({ date, score, counts: {} });

beforeEach(() => localStorage.clear());

describe('watchtower score history', () => {
  it('keeps counts only, never item ids', () => {
    const p = pointFromReport(report(71.6), day('2026-10-30'));
    expect(p).toEqual({ date: '2026-10-30', score: 72, counts: { weak: 2, reused: 2 } });
    recordReport(report(71), day('2026-10-30'));
    expect(localStorage.getItem(SCORE_HISTORY_KEY)).not.toContain('secret-id');
  });

  it('keeps one point per day, the latest of the day', () => {
    let h = addPoint([], pt('2026-10-30', 60), day('2026-10-30'));
    h = addPoint(h, pt('2026-10-30', 75), day('2026-10-30'));
    expect(h).toEqual([pt('2026-10-30', 75)]);
    h = addPoint(h, pt('2026-10-29', 50), day('2026-10-30'));
    expect(h.map((p) => p.date)).toEqual(['2026-10-29', '2026-10-30']);
  });

  it('prunes points older than 90 days', () => {
    const now = day('2026-10-30');
    const old = pt('2026-07-30', 40); // 92 days ago
    const edge = pt('2026-08-02', 50); // 89 days ago
    const h = addPoint([old, edge], pt('2026-10-30', 80), now);
    expect(h.map((p) => p.date)).toEqual(['2026-08-02', '2026-10-30']);
  });

  it('persists through recordReport and ignores corrupt storage', () => {
    recordReport(report(60), day('2026-10-29'));
    recordReport(report(80), day('2026-10-30'));
    expect(loadHistory().map((p) => p.score)).toEqual([60, 80]);
    localStorage.setItem(SCORE_HISTORY_KEY, '{nope');
    expect(loadHistory()).toEqual([]);
    localStorage.setItem(SCORE_HISTORY_KEY, JSON.stringify([{ date: 'x', score: 1 }, pt('2026-10-30', 5)]));
    expect(loadHistory()).toEqual([pt('2026-10-30', 5)]);
  });

  it('filters to the 30 or 90 day range', () => {
    const now = day('2026-10-30');
    const pts = [pt('2026-08-15', 50), pt('2026-10-10', 60), pt('2026-10-30', 70)];
    expect(inRange(pts, 30, now)).toHaveLength(2);
    expect(inRange(pts, 90, now)).toHaveLength(3);
  });

  it('computes the weekly delta against a week-old point, else the oldest', () => {
    const now = day('2026-10-30');
    expect(weeklyDelta([pt('2026-10-30', 70)], now)).toBeNull();
    expect(weeklyDelta([pt('2026-10-20', 60), pt('2026-10-23', 70), pt('2026-10-28', 90), pt('2026-10-30', 76)], now)).toEqual({ delta: 6, since: '2026-10-23', week: true });
    expect(weeklyDelta([pt('2026-10-28', 80), pt('2026-10-30', 76)], now)).toEqual({ delta: -4, since: '2026-10-28', week: false });
  });

  it('dayKey uses the local calendar day', () => {
    expect(dayKey(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
  });
});
