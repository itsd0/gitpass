import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { VaultInfo } from '../api/types';
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
