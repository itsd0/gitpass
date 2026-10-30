import type { WatchtowerReport } from '../../api/types';

/** Actionable finding kinds for the security checkup, in impact order (#401). */
export type CheckupKind = 'breached' | 'reused' | 'weak' | 'old' | 'missingTotp';

export interface CheckupFinding {
  itemId: string;
  kind: CheckupKind;
  /** kind string expected by the ignore_finding API. */
  ignoreKind: string;
}

const ORDER: CheckupKind[] = ['breached', 'reused', 'weak', 'old', 'missingTotp'];

const IGNORE_KIND: Record<CheckupKind, string> = {
  breached: 'pwned',
  reused: 'reused',
  weak: 'weak',
  old: 'old',
  missingTotp: 'missing_totp',
};

/**
 * Flatten a Watchtower report into the checkup queue: one entry per
 * (item, finding kind), ordered breached → reused → weak → old/rotation →
 * missing 2FA. Rotation-due/sooner items fold into 'old'. Ignored findings
 * are excluded. HIBP-breached entries only appear when the check ran.
 */
export function buildCheckupQueue(report: WatchtowerReport): CheckupFinding[] {
  const ignored = new Set((report.ignored ?? []).map((i) => `${i.itemId}:${i.kind}`));
  const seen = new Set<string>();
  const out: CheckupFinding[] = [];
  const push = (kind: CheckupKind, ids: string[]) => {
    for (const itemId of ids) {
      const key = `${itemId}:${kind}`;
      if (seen.has(key) || ignored.has(`${itemId}:${IGNORE_KIND[kind]}`)) continue;
      seen.add(key);
      out.push({ itemId, kind, ignoreKind: IGNORE_KIND[kind] });
    }
  };
  for (const kind of ORDER) {
    if (kind === 'reused') push(kind, [...new Set((report.reused ?? []).flat())]);
    else if (kind === 'old') {
      push(kind, report.old ?? []);
      push(kind, (report.rotationDue ?? []).map((r) => r.itemId));
      push(kind, (report.rotationSoon ?? []).map((r) => r.itemId));
    } else if (kind === 'missingTotp') {
      // Sites with a known 2FA option appear as "2FA available", not generic missing 2FA.
      const specific = new Set(report.totpAvailable ?? []);
      push(kind, (report.missingTotp ?? []).filter((id) => !specific.has(id)));
    } else push(kind, report[kind] ?? []);
  }
  return out;
}

/** One-line explanation i18n key per finding kind. */
export function explanationKey(kind: CheckupKind): string {
  return `checkup.explain.${kind}`;
}
