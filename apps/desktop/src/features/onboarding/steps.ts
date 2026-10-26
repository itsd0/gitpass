import type { Onboarding } from '../../api/types';
export const STEPS = [
  { key: 'extensionInstalled', label: 'Install the gitpass browser extension', desc: 'Fill and save logins as you browse.' },
  { key: 'emergencyKitSaved', label: 'Save your Emergency Kit', desc: 'Print it and store it somewhere safe.' },
  { key: 'repoConnected', label: 'Connect a GitHub repo', desc: 'Sync your encrypted vault through your own empty private repo (Settings → Sync).' },
  { key: 'imported', label: 'Import your data', desc: 'Bring passwords over from another manager or browser.' },
] as const satisfies readonly { key: keyof Onboarding; label: string; desc: string }[];
export const stepsLeft = (o: Onboarding | null) => o ? STEPS.filter(s => !o[s.key]).length : STEPS.length;
