import { useSyncExternalStore } from 'react';
import { api } from '../../api/client';
import { menuFields, type MenuFields } from './ItemContextMenu';

/**
 * Per-item field presence (username/password/totp/website) for row quick actions. ItemSummary carries no field data
 * and a summary-level hasTotp would need a Rust change, so fields are read lazily from the item detail - the same
 * getItem + menuFields path the context menu uses when it opens - and cached per item. The list clears the cache
 * whenever itemsVersion bumps so edits are picked up.
 */
const fields = new Map<string, MenuFields>();
const inflight = new Set<string>();
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function clearRowFields() {
  fields.clear();
  inflight.clear();
  emit();
}

/** Start loading an item's field presence if it is not already cached or in flight. Idempotent and cheap to call on every hover. */
export function ensureRowFields(vaultId: string, itemId: string) {
  const key = `${vaultId}:${itemId}`;
  if (fields.has(key) || inflight.has(key)) return;
  inflight.add(key);
  api.getItem({ vaultId, itemId }).then(
    (d) => {
      inflight.delete(key);
      fields.set(key, menuFields(d));
      emit();
    },
    () => {
      inflight.delete(key);
    }
  );
}

export function useRowFields(vaultId: string, itemId: string): MenuFields | undefined {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => fields.get(`${vaultId}:${itemId}`)
  );
}
