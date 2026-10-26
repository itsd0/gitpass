import type { ReactNode } from 'react';
import type { ItemCategory } from '../api/types';

/** Style-B category glyphs: 24px stroke glyph in a 64x64 tinted circle, with one orange accent detail. */
const ACCENT = '#F28C28';
const STROKE = '#E5E7EB';

const GLYPHS: Record<ItemCategory, { base: ReactNode; accent: ReactNode }> = {
  login: {
    base: <><circle cx="8" cy="12" r="4" /><path d="M12 12h9M18 12v3M21 12v2" /></>,
    accent: <circle cx="8" cy="12" r="1.3" fill={ACCENT} />,
  },
  secure_note: {
    base: <><rect x="4.5" y="3.5" width="15" height="17" rx="2.5" /><path d="M8 8.5h8M8 12h8" /></>,
    accent: <path d="M8 15.5h5" />,
  },
  credit_card: {
    base: <><rect x="2.5" y="5.5" width="19" height="13" rx="2.5" /><path d="M2.5 10h19" /></>,
    accent: <path d="M6 15h4" />,
  },
  identity: {
    base: <><rect x="2.5" y="4.5" width="19" height="15" rx="2.5" /><path d="M5.5 16.5a3.5 3.5 0 0 1 6 0M14 10h4.5M14 13.5h3" /></>,
    accent: <circle cx="8.5" cy="10.5" r="2" />,
  },
  api_credential: {
    base: <path d="M8 7l-5 5 5 5M16 7l5 5-5 5" />,
    accent: <path d="M13.5 5.5l-3 13" />,
  },
  document: {
    base: <><path d="M14 3H7.5A2.5 2.5 0 0 0 5 5.5v13A2.5 2.5 0 0 0 7.5 21h9a2.5 2.5 0 0 0 2.5-2.5V8z" /><path d="M8.5 13h7M8.5 16.5h5" /></>,
    accent: <path d="M14 3v5h5" strokeLinejoin="round" />,
  },
  ssh_key: {
    base: <><rect x="2.5" y="4.5" width="19" height="15" rx="2.5" /><path d="M6.5 9.5l3 2.5-3 2.5" /></>,
    accent: <path d="M12 15h5" />,
  },
  passkey: {
    base: <><circle cx="9" cy="8" r="3.5" /><path d="M3 20a6 6 0 0 1 10.5-4" /></>,
    accent: <><circle cx="17.5" cy="13.5" r="2.5" /><path d="M17.5 16v5M17.5 18.5h2" /></>,
  },
  wifi: {
    base: <><path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0" /><circle cx="12" cy="19.2" r="1.1" fill={STROKE} stroke="none" /></>,
    accent: <path d="M8.5 16a5 5 0 0 1 7 0" />,
  },
};

export function CategoryIcon({ category, size = 36, className, title }: { category: ItemCategory; size?: number; className?: string; title?: string }) {
  const g = GLYPHS[category] ?? GLYPHS.secure_note;
  // Slightly heavier strokes at tiny sizes keep the glyph readable.
  const sw = size <= 20 ? 1.9 : size <= 28 ? 1.7 : 1.5;
  return (
    <svg viewBox="0 0 64 64" width={size} height={size} className={`shrink-0 ${className ?? ''}`} role={title ? 'img' : undefined}
      aria-hidden={title ? undefined : true} aria-label={title} data-testid={`category-icon-${category}`}>
      <circle cx="32" cy="32" r="30" fill={ACCENT} fillOpacity={0.14} />
      <circle cx="32" cy="32" r="29.5" fill="none" stroke={ACCENT} strokeOpacity={0.3} />
      <g transform="translate(14 14) scale(1.5)" fill="none" stroke={STROKE} strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round">{g.base}</g>
      <g transform="translate(14 14) scale(1.5)" fill="none" stroke={ACCENT} strokeWidth={sw} strokeLinecap="round">{g.accent}</g>
    </svg>
  );
}
