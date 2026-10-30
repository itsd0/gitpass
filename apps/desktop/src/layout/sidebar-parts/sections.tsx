import * as React from 'react';
import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useT } from '../../i18n';
import { RailContext } from './NavItem';

const SECTIONS_KEY = 'gitpass.sidebar.collapsedSections';
export type SectionId = 'vaults' | 'tags' | 'searches';
/** Collapsed Vaults/Tags/Searches sections, remembered on this device only (localStorage, never written to the vault). */
function loadCollapsed(): Record<SectionId, boolean> {
  try {
    const raw = JSON.parse(localStorage.getItem(SECTIONS_KEY) ?? '{}') as Partial<Record<SectionId, unknown>>;
    return { vaults: raw.vaults === true, tags: raw.tags === true, searches: raw.searches === true };
  } catch {
    return { vaults: false, tags: false, searches: false };
  }
}
export function useCollapsedSections() {
  const [collapsed, setCollapsed] = useState(loadCollapsed);
  const set = (id: SectionId, value: boolean) => setCollapsed((c) => {
    const next = { ...c, [id]: value };
    try { localStorage.setItem(SECTIONS_KEY, JSON.stringify(next)); } catch { /* storage unavailable: keep in memory */ }
    return next;
  });
  return [collapsed, set] as const;
}

/** Muted right-aligned count. Hidden from the accessible name so rows keep their plain label. */
export function Count({ n, named }: { n: number; named?: boolean }) {
  return <span className="text-xs text-muted" data-testid="sidebar-count" aria-hidden={named ? undefined : true}>{n}</span>;
}

export function SectionHeader({ label, name, expanded, onToggle, action }: { label: string; name: string; expanded: boolean; onToggle: (open: boolean) => void; action?: React.ReactNode }) {
  const t = useT();
  const rail = React.useContext(RailContext);
  if (rail) return <div role="separator" aria-label={label} className="mx-2 my-2 border-t border-border" />;
  return (
    <div className="mt-5 flex items-center justify-between pb-1 pr-2.5 text-[11px] font-semibold tracking-wider text-muted">
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={t(expanded ? 'sidebar.collapseSection' : 'sidebar.expandSection', { section: name })}
        onClick={() => onToggle(!expanded)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft' && expanded) { e.preventDefault(); onToggle(false); }
          else if (e.key === 'ArrowRight' && !expanded) { e.preventDefault(); onToggle(true); }
        }}
        className="flex items-center gap-1 rounded px-2.5 py-0.5 font-semibold tracking-wider hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
      >
        <ChevronRight size={12} className={cn('transition-transform duration-150 motion-reduce:transition-none', expanded && 'rotate-90')} />
        {label}
      </button>
      {action}
    </div>
  );
}
