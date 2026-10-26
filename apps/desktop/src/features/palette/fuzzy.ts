/** Subsequence fuzzy score. Higher = better; null = no match.
 * Rewards: exact > prefix > word-boundary hits > consecutive runs; penalises gaps and long targets. */
export function fuzzyScore(query: string, target: string): number | null {
  const q = query.trim().toLowerCase(); const s = target.toLowerCase();
  if (!q) return 0;
  if (s === q) return 1000;
  if (s.startsWith(q)) return 800 - s.length;
  const idx = s.indexOf(q);
  if (idx >= 0) return 600 - idx * 2 - s.length + (/[\s\-_/.]/.test(s[idx - 1] ?? ' ') ? 50 : 0);
  let score = 0, ti = 0, prev = -2;
  for (const ch of q) {
    if (ch === ' ') continue;
    const found = s.indexOf(ch, ti); if (found < 0) return null;
    const boundary = found === 0 || /[\s\-_/.]/.test(s[found - 1]!);
    score += 10 + (boundary ? 15 : 0) + (found === prev + 1 ? 12 : 0) - Math.min(found - ti, 10);
    prev = found; ti = found + 1;
  }
  return score - s.length * 0.5;
}
/** Secondary texts (subtitle, keywords) rank below primary-title matches of similar quality. */
export const SECONDARY_PENALTY = 250;
/** Rank by best score across texts; first text is primary. Stable on ties. */
export function rank<T>(query: string, items: T[], text: (t: T) => string | string[]): T[] {
  return items.map((it, i) => { const ts = ([] as string[]).concat(text(it)); let best: number | null = null;
      for (const [j, x] of ts.entries()) { const raw = fuzzyScore(query, x); const sc = raw === null ? null : raw - (j > 0 ? SECONDARY_PENALTY : 0); if (sc !== null && (best === null || sc > best)) best = sc; } return { it, i, best }; })
    .filter((x) => x.best !== null).sort((a, b) => b.best! - a.best! || a.i - b.i).map((x) => x.it);
}
