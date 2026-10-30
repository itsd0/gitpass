export interface ParsedSemver {
  major: number;
  minor: number;
  patch: number;
  prerelease: string | null;
}

export function parseSemver(v: string): ParsedSemver {
  const clean = v.trim().replace(/^v/i, '');
  const [versionCore, ...preParts] = clean.split('-');
  const prerelease = preParts.length > 0 ? preParts.join('-') : null;
  const numParts = versionCore.split('.').map((p) => {
    const n = parseInt(p, 10);
    return Number.isFinite(n) ? n : 0;
  });

  return {
    major: numParts[0] ?? 0,
    minor: numParts[1] ?? 0,
    patch: numParts[2] ?? 0,
    prerelease,
  };
}

/**
 * Returns:
 * > 0 if a > b
 *   0 if a == b
 * < 0 if a < b
 */
export function compareSemver(a: string, b: string): number {
  const parsedA = parseSemver(a);
  const parsedB = parseSemver(b);

  if (parsedA.major !== parsedB.major) return parsedA.major - parsedB.major;
  if (parsedA.minor !== parsedB.minor) return parsedA.minor - parsedB.minor;
  if (parsedA.patch !== parsedB.patch) return parsedA.patch - parsedB.patch;

  // If core numbers are equal, non-prerelease is greater than prerelease (e.g. 0.6.5 > 0.6.5-beta.1)
  if (!parsedA.prerelease && parsedB.prerelease) return 1;
  if (parsedA.prerelease && !parsedB.prerelease) return -1;
  if (parsedA.prerelease && parsedB.prerelease) {
    return parsedA.prerelease.localeCompare(parsedB.prerelease);
  }

  return 0;
}

export type WhatsNewAction = 'show' | 'record_silent' | 'ignore';

/**
 * Determines whether to show the What's New sheet, record the version silently (fresh install),
 * or ignore (same version or downgrade).
 */
export function determineWhatsNewAction(
  runningVersion: string,
  lastSeenVersion?: string | null
): WhatsNewAction {
  if (!lastSeenVersion || lastSeenVersion.trim() === '') {
    // Fresh install: record silently, don't show sheet
    return 'record_silent';
  }

  const cmp = compareSemver(runningVersion, lastSeenVersion);
  if (cmp > 0) {
    // Upgrade: show sheet
    return 'show';
  }

  // Same version or downgrade: do nothing
  return 'ignore';
}
