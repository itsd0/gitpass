import type { WatchtowerReport } from '../../api/types';
export function scoreBand(score: number) {
  const s = Math.max(0, Math.min(100, Math.round(score)));
  if (s >= 85) return { score: s, label: 'Excellent', color: '#22c55e' };
  if (s >= 70) return { score: s, label: 'Good', color: '#84cc16' };
  if (s >= 50) return { score: s, label: 'Fair', color: '#f59e0b' };
  return { score: s, label: 'Poor', color: '#ef4444' };
}
/** Arc geometry for a 270° gauge. Returns stroke-dasharray values. */
export function gaugeDash(score: number, r = 52) { const c = 2 * Math.PI * r, arc = c * 0.75; return { circumference: c, arc, filled: arc * scoreBand(score).score / 100 }; }
export type IssueKey = 'weak' | 'reused' | 'old' | 'breached' | 'missingTotp' | 'totpAvailable' | 'passkeyAvailable' | 'expired' | 'expiringSoon' | 'insecureUrls';
/** Item ids for a card. Known 2FA sites appear once, as "2FA available", not also as generic "Missing 2FA". */
export function issueIds(r: WatchtowerReport, k: IssueKey): string[] {
  if (k === 'reused') return [...new Set(r.reused.flat())];
  if (k === 'missingTotp') { const specific = new Set(r.totpAvailable); return r.missingTotp.filter((id) => !specific.has(id)); }
  return r[k] ?? [];
}
