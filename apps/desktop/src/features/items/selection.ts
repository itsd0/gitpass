// Pure multi-select logic for the item list (1Password-style): plain click selects one,
// Cmd/Ctrl-click toggles, Shift-click selects the range from the anchor, Cmd-A selects all.
import type { ItemSummary, ItemRef } from '../../api/types';
import { useApp } from '../../lib/store';

export interface Sel { selected: string | null; multi: string[] }
export interface Mods { toggle?: boolean; range?: boolean }

/** Ids currently selected (multi when >1, else the single selection). */
export const selectedIds = (s: Sel): string[] => (s.multi.length > 1 ? s.multi : s.selected ? [s.selected] : []);

export function clickSelect(order: string[], s: Sel, id: string, m: Mods = {}): Sel {
  const norm = (ids: string[], focus: string | null): Sel => {
    const set = new Set(ids); const out = order.filter((x) => set.has(x));
    if (out.length > 1) return { selected: focus && set.has(focus) ? focus : out[out.length - 1], multi: out };
    return { selected: out[0] ?? null, multi: [] };
  };
  if (m.range) {
    const anchor = s.selected && order.includes(s.selected) ? s.selected : id;
    const a = order.indexOf(anchor), b = order.indexOf(id);
    if (a < 0 || b < 0) return { selected: id, multi: [] };
    const span = order.slice(Math.min(a, b), Math.max(a, b) + 1);
    const base = m.toggle ? selectedIds(s) : [];
    const r = norm([...base, ...span], anchor);
    return r.multi.length ? { ...r, selected: anchor } : r;
  }
  if (m.toggle) {
    const cur = selectedIds(s);
    const next = cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id];
    return norm(next, cur.includes(id) ? s.selected : id);
  }
  return { selected: id, multi: [] };
}

export const selectAll = (order: string[], s: Sel): Sel =>
  order.length > 1 ? { selected: s.selected && order.includes(s.selected) ? s.selected : order[0], multi: [...order] } : { selected: order[0] ?? null, multi: [] };

/** Keep only ids still present after a list refresh. */
export function prune(order: string[], s: Sel): Sel {
  const has = new Set(order);
  const multi = s.multi.filter((x) => has.has(x));
  if (multi.length > 1) return { selected: s.selected && has.has(s.selected) ? s.selected : multi[0], multi };
  if (s.multi.length) return { selected: multi[0] ?? null, multi: [] };
  return s;
}

/** Apply a Sel to the app store. */
export function applySel(s: Sel) { useApp.setState({ selectedItemId: s.selected, multiIds: s.multi, editing: false }); }
export const readSel = (): Sel => { const a = useApp.getState(); return { selected: a.selectedItemId, multi: a.multiIds }; };

/** Vault/item refs for the ids, in list order. */
export const refsFor = (items: ItemSummary[], ids: string[]): ItemRef[] => {
  const set = new Set(ids); return items.filter((x) => set.has(x.id)).map((x) => ({ vaultId: x.vaultId, itemId: x.id }));
};
