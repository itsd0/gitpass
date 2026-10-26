import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { VaultInfo } from '../api/types';
import { useApp } from './store';
/** Vaults + tags, refetched whenever items change. */
export function useVaultsAndTags() {
  const v = useApp((s) => s.itemsVersion);
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [tags, setTags] = useState<string[]>([]);
  useEffect(() => { let on = true; Promise.all([api.listVaults(), api.listTags()]).then(([a, b]) => { if (on) { setVaults(a); setTags(b); } }).catch(() => {}); return () => { on = false; }; }, [v]);
  return { vaults, tags };
}
