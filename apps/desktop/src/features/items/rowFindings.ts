import { create } from 'zustand';
import type { WatchtowerReport } from '../../api/types';
import { issueIds, type IssueKey } from '../watchtower/score';

/** Findings that can drive the item-list status icon, with the severity each carries. Rotation, unused and "available" suggestions are not security warnings here. */
export const ROW_FINDING_KINDS = [
  ['breached', 'danger'], ['reused', 'danger'], ['expired', 'danger'],
  ['weak', 'warn'], ['old', 'warn'], ['insecureUrls', 'warn'], ['expiringSoon', 'warn'], ['missingTotp', 'warn'], ['duplicates', 'warn'], ['weakSshKey', 'warn'], ['reusedOldPassword', 'warn'],
] as const satisfies readonly (readonly [IssueKey, 'danger' | 'warn'])[];
export type RowFindingKind = (typeof ROW_FINDING_KINDS)[number][0];
export type RowSeverity = 'danger' | 'warn';
export interface RowFinding { severity: RowSeverity; kinds: RowFindingKind[] }

/** One pass over the report the app already computed: item id -> its findings (most severe first). Ignored findings are not in the report. */
export function rowFindingsMap(r: WatchtowerReport | null, breachedExtra: readonly string[] = []): Map<string, RowFinding> {
  const out = new Map<string, RowFinding>();
  if (!r) return out;
  for (const [kind, sev] of ROW_FINDING_KINDS) {
    const ids = kind === 'breached' ? [...new Set([...issueIds(r, kind), ...breachedExtra])] : issueIds(r, kind);
    for (const id of ids) {
      const cur = out.get(id);
      if (cur) { cur.kinds.push(kind); if (sev === 'danger') cur.severity = 'danger'; }
      else out.set(id, { severity: sev, kinds: [kind] });
    }
  }
  return out;
}

/** The last report that included the HIBP check, so breached logins still show when the list recomputes without a network lookup. */
interface S { breached: string[]; remember: (r: WatchtowerReport) => void }
export const useBreachedIds = create<S>((set) => ({
  breached: [],
  remember: (r) => { if (r.hibpChecked) set({ breached: r.breached }); },
}));

/** Per-device preference (localStorage, like Text size); never written to the vault. On unless explicitly turned off. */
export const ROW_WARNINGS_KEY = 'gitpass.rowWarnings';
export function loadRowWarnings(): boolean {
  try { return localStorage.getItem(ROW_WARNINGS_KEY) !== 'off'; } catch { return true; }
}
interface W { enabled: boolean; set: (enabled: boolean) => void }
export const useRowWarnings = create<W>((set) => ({
  enabled: loadRowWarnings(),
  set: (enabled) => {
    try { if (enabled) localStorage.removeItem(ROW_WARNINGS_KEY); else localStorage.setItem(ROW_WARNINGS_KEY, 'off'); } catch { /* storage unavailable: keep in memory */ }
    set({ enabled });
  },
}));
