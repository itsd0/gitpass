import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { ItemSummary, VaultInfo } from '../api/types';
import { useApp } from './store';
/** Vaults + tags, refetched whenever items change. */
export function useVaultsAndTags() {
  const v = useApp((s) => s.itemsVersion);
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [tags, setTags] = useState<string[]>([]);
  useEffect(() => { let on = true; // independent: a tags failure must not hide the vault list
    api.listVaults().then((a) => { if (on) setVaults(a); }).catch((e) => console.error('list_vaults', e));
    api.listTags().then((b) => { if (on) setTags(b); }).catch(() => {});
    return () => { on = false; }; }, [v]);
  return { vaults, tags };
}

export interface SidebarCounts { all: number; favorites: number; archive: number; deleted: number; tags: Record<string, number>; items?: ItemSummary[]; }
const NO_COUNTS: SidebarCounts = { all: 0, favorites: 0, archive: 0, deleted: 0, tags: {} };
/** Counts shown beside the sidebar rows, derived from the same lists the item views show; refetched whenever items change. */
export function useSidebarCounts(): SidebarCounts {
  const v = useApp((s) => s.itemsVersion);
  const [counts, setCounts] = useState<SidebarCounts>(NO_COUNTS);
  useEffect(() => { let on = true;
    const part = <T,>(p: Promise<T>, apply: (x: T) => void) => p.then((x) => { if (on) apply(x); }).catch(() => {});
    part(api.listItems({ filter: { kind: 'all' } }), (xs) => setCounts((c) => {
      const tags: Record<string, number> = {};
      for (const x of xs) for (const tg of x.tags) tags[tg] = (tags[tg] ?? 0) + 1;
      return { ...c, all: xs.length, favorites: xs.filter((x) => x.favorite).length, tags, items: xs };
    }));
    part(api.listItems({ filter: { kind: 'archive' } }), (xs) => setCounts((c) => ({ ...c, archive: xs.length })));
    part(api.listItems({ filter: { kind: 'deleted' } }), (xs) => setCounts((c) => ({ ...c, deleted: xs.length })));
    return () => { on = false; }; }, [v]);
  return counts;
}
